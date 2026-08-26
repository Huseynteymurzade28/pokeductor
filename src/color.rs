//! Terminal colour depth: what the terminal can show, and how to get there.
//!
//! Sprites are drawn as 24-bit RGB half-blocks. A terminal that does not
//! understand those escapes does not fall back gracefully — it drops them, and
//! the artwork comes out as a solid block of whatever the default colour is.
//! That is not a niche configuration: `screen`, an older `tmux`, a plain
//! `TERM=xterm-256color` with no `COLORTERM`, and a fair number of corporate
//! emulators all land there.
//!
//! So we work out what the terminal can do and meet it: 24-bit where it is
//! supported, the xterm-256 palette where it is not, and no colour at all when
//! the user says so. The mapping happens once per frame over the finished
//! buffer rather than at each call site, which is what keeps every colour in
//! the app — sprite pixels, borders, type chips — degrading by the same rule.

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// What the terminal can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// 24-bit RGB, emitted as written.
    Truecolor,
    /// The 256-colour palette: the 6x6x6 cube plus the greyscale ramp.
    Ansi256,
    /// No colour at all. Sprites are skipped rather than drawn as a field of
    /// identical blocks, which is also what makes the interface legible to a
    /// screen reader or in a captured log.
    None,
}

/// What the user asked for, ahead of what we would have detected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Choice {
    /// Work it out from the environment.
    #[default]
    Auto,
    Truecolor,
    Ansi256,
    Never,
}

/// The environment variables detection reads.
///
/// Taken as a value rather than read inline so [`detect`] stays a pure
/// function of its inputs — testable across the combinations that matter
/// without a test mutating the process environment out from under its
/// neighbours.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub colorterm: Option<String>,
    pub term: Option<String>,
    pub no_color: Option<String>,
}

impl Env {
    /// The real environment. A variable set to the empty string reads as unset:
    /// that is what [`NO_COLOR`](https://no-color.org/) specifies, and an empty
    /// `TERM` carries no more information than a missing one.
    pub fn from_process() -> Self {
        let var = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
        Env {
            colorterm: var("COLORTERM"),
            term: var("TERM"),
            no_color: var("NO_COLOR"),
        }
    }
}

/// The depth to render at, given the flag and the environment.
///
/// An explicit `--color` outranks everything, `NO_COLOR` included: someone who
/// names a depth on the command line has already answered the question the
/// environment is being consulted about.
pub fn resolve(choice: Choice, env: &Env) -> Depth {
    match choice {
        Choice::Truecolor => Depth::Truecolor,
        Choice::Ansi256 => Depth::Ansi256,
        Choice::Never => Depth::None,
        Choice::Auto => detect(env),
    }
}

/// Tells crossterm what we decided, where that differs from what it would
/// have concluded on its own.
///
/// crossterm reads `NO_COLOR` itself and drops every colour sequence when it is
/// set. That is the right default and it agrees with [`detect`] — but it also
/// means an explicit `--color=truecolor` would be silently stripped on the way
/// out, leaving us claiming a depth the frames never reach the terminal in.
/// A depth the user named outranks the environment, so we say so here.
/// Modifiers are untouched by this, so the reverse-video highlight survives
/// either way.
pub fn enforce(choice: Choice, depth: Depth) {
    if choice != Choice::Auto {
        crossterm::style::force_color_output(depth != Depth::None);
    }
}

/// Works out what the terminal supports from `COLORTERM` and `TERM`.
///
/// This is a heuristic — there is no reliable query — so it is deliberately
/// conservative in the middle. `COLORTERM` is the only positive claim of
/// 24-bit support that can be trusted, and a terminal that announces itself
/// through `TERM` alone gets the 256-colour path even when it would in fact
/// have managed truecolor: a slightly coarser sprite is a much smaller cost
/// than an unreadable one, and `--color=truecolor` is there for anyone who
/// knows better.
///
/// An absent `TERM` is the exception, and it is Windows: the modern console
/// sets neither variable and handles 24-bit fine. It is also what this app
/// assumed unconditionally before any of this existed, so it is the least
/// surprising place for the unknown case to land.
pub fn detect(env: &Env) -> Depth {
    if env.no_color.is_some() {
        return Depth::None;
    }

    let term = env.term.as_deref();
    if term == Some("dumb") {
        return Depth::None;
    }

    let claims_truecolor = |value: &str| {
        value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
    };
    if env.colorterm.as_deref().is_some_and(claims_truecolor) {
        return Depth::Truecolor;
    }

    match term {
        Some(term) if term.contains("truecolor") || term.contains("24bit") => Depth::Truecolor,
        Some(_) => Depth::Ansi256,
        None => Depth::Truecolor,
    }
}

/// Rewrites a rendered frame into what the terminal can actually show.
///
/// Running over the finished buffer rather than at each call site is what makes
/// this one rule instead of a condition threaded through every widget: whatever
/// the renderer produced, in whatever palette, arrives here as plain colours to
/// be mapped.
pub fn degrade(buffer: &mut Buffer, depth: Depth) {
    match depth {
        Depth::Truecolor => {}
        Depth::Ansi256 => {
            for cell in &mut buffer.content {
                cell.fg = indexed(cell.fg);
                cell.bg = indexed(cell.bg);
            }
        }
        Depth::None => {
            for cell in &mut buffer.content {
                cell.fg = Color::Reset;
                cell.bg = Color::Reset;
            }
        }
    }
}

/// The 256-colour equivalent of one colour. Anything already expressed in
/// terms the terminal defines — an index, or its default — is left alone.
fn indexed(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Indexed(quantize(r, g, b)),
        other => other,
    }
}

/// The six values each channel of the xterm colour cube can take. Note the gap:
/// the step from 0 to 95 is far larger than the ones above it, which is why
/// dark colours are the ones the cube approximates worst and why the greyscale
/// ramp is worth consulting separately.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The nearest xterm-256 palette index to an RGB colour.
///
/// Two candidates are compared: the closest colour in the 6x6x6 cube
/// (indices 16-231) and the closest step on the 24-entry greyscale ramp
/// (232-255). Greys are why both are needed — the ramp is far finer than the
/// cube's diagonal, so `(120, 120, 120)` has a near-exact match on one and a
/// visible cast on the other.
pub fn quantize(r: u8, g: u8, b: u8) -> u8 {
    let level = |channel: u8| -> usize {
        CUBE_LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, &level)| (level as i32 - channel as i32).abs())
            .map(|(index, _)| index)
            .unwrap_or(0)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let cube_distance = distance(
        (r, g, b),
        (CUBE_LEVELS[ri], CUBE_LEVELS[gi], CUBE_LEVELS[bi]),
    );

    // The ramp runs 8, 18, ... 238, so the nearest step is found by rounding
    // the average channel rather than searching.
    let average = (r as u32 + g as u32 + b as u32) / 3;
    let step = (average.saturating_sub(3) / 10).min(23);
    let grey = (8 + 10 * step) as u8;
    let grey_distance = distance((r, g, b), (grey, grey, grey));

    if grey_distance < cube_distance {
        (232 + step) as u8
    } else {
        cube_index as u8
    }
}

/// Squared distance between two colours. Squared because only the comparison
/// matters and the square root would not change which side of it we land on.
fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let channel = |x: u8, y: u8| {
        let d = x as i32 - y as i32;
        (d * d) as u32
    };
    channel(a.0, b.0) + channel(a.1, b.1) + channel(a.2, b.2)
}

/// A style that reads as a selection whether or not colour survives.
///
/// `REVERSED` on top of an ordinary foreground/background pair is what a
/// coloured terminal was going to draw anyway — it swaps them back — while on a
/// terminal rendering no colour at all it is the one distinction still
/// available. Written the other way round (background set directly, no
/// modifier) the highlight bar simply disappears under [`Depth::None`], and
/// with it any way to tell where the cursor is.
pub fn highlight(color: Color) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(color)
        .bg(crate::theme::BASE)
        .add_modifier(Modifier::REVERSED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(colorterm: Option<&str>, term: Option<&str>) -> Env {
        Env {
            colorterm: colorterm.map(str::to_string),
            term: term.map(str::to_string),
            no_color: None,
        }
    }

    /// The RGB the terminal shows for a palette index, so the round trip below
    /// has something to compare against.
    fn palette_rgb(index: u8) -> (u8, u8, u8) {
        if index >= 232 {
            let grey = 8 + 10 * (index as u16 - 232);
            (grey as u8, grey as u8, grey as u8)
        } else {
            let n = index as usize - 16;
            (
                CUBE_LEVELS[n / 36],
                CUBE_LEVELS[(n / 6) % 6],
                CUBE_LEVELS[n % 6],
            )
        }
    }

    #[test]
    fn every_palette_colour_quantizes_to_itself() {
        for index in 16..=255u8 {
            let (r, g, b) = palette_rgb(index);
            assert_eq!(
                quantize(r, g, b),
                index,
                "index {index} ({r}, {g}, {b}) should be its own nearest match"
            );
        }
    }

    #[test]
    fn greys_prefer_the_ramp_over_the_cube_diagonal() {
        // 120 sits between the cube's 95 and 135 and almost exactly on a ramp
        // step, which is the case the ramp exists for.
        let index = quantize(120, 120, 120);
        assert!(index >= 232, "expected a greyscale index, got {index}");
        assert_eq!(palette_rgb(index), (118, 118, 118));
    }

    #[test]
    fn a_saturated_colour_takes_the_cube() {
        let index = quantize(255, 0, 0);
        assert_eq!(index, 196);
        assert_eq!(palette_rgb(index), (255, 0, 0));
    }

    #[test]
    fn colorterm_is_the_one_claim_of_truecolor_we_take_at_its_word() {
        assert_eq!(
            detect(&env(Some("truecolor"), Some("xterm"))),
            Depth::Truecolor
        );
        assert_eq!(detect(&env(Some("24bit"), Some("xterm"))), Depth::Truecolor);
        assert_eq!(detect(&env(Some("TrueColor"), None)), Depth::Truecolor);
    }

    #[test]
    fn a_256_colour_term_without_colorterm_gets_the_palette() {
        assert_eq!(detect(&env(None, Some("xterm-256color"))), Depth::Ansi256);
        assert_eq!(detect(&env(None, Some("screen-256color"))), Depth::Ansi256);
        // A COLORTERM saying something else is not a claim of 24-bit support.
        assert_eq!(
            detect(&env(Some("8bit"), Some("xterm-256color"))),
            Depth::Ansi256
        );
    }

    #[test]
    fn a_term_we_have_never_heard_of_gets_the_conservative_answer() {
        assert_eq!(detect(&env(None, Some("vt220"))), Depth::Ansi256);
        assert_eq!(detect(&env(None, Some("linux"))), Depth::Ansi256);
    }

    #[test]
    fn a_terminal_that_names_itself_truecolor_is_believed_too() {
        assert_eq!(
            detect(&env(None, Some("xterm-truecolor"))),
            Depth::Truecolor
        );
    }

    #[test]
    fn no_term_at_all_is_windows_and_keeps_its_colour() {
        assert_eq!(detect(&env(None, None)), Depth::Truecolor);
    }

    #[test]
    fn a_dumb_terminal_gets_no_colour() {
        assert_eq!(detect(&env(Some("truecolor"), Some("dumb"))), Depth::None);
    }

    #[test]
    fn no_color_wins_over_anything_the_terminal_advertises() {
        let mut env = env(Some("truecolor"), Some("xterm-256color"));
        env.no_color = Some("1".to_string());
        assert_eq!(detect(&env), Depth::None);
        // Per the convention, the value is irrelevant; only presence counts.
        env.no_color = Some("0".to_string());
        assert_eq!(detect(&env), Depth::None);
    }

    #[test]
    fn an_empty_variable_is_the_same_as_an_unset_one() {
        // `Env::from_process` filters these out, so detection never sees them;
        // this pins the behaviour that filter is there to produce.
        assert_eq!(detect(&Env::default()), Depth::Truecolor);
    }

    #[test]
    fn the_flag_outranks_both_the_terminal_and_no_color() {
        let mut hostile = env(None, Some("dumb"));
        hostile.no_color = Some("1".to_string());

        assert_eq!(resolve(Choice::Truecolor, &hostile), Depth::Truecolor);
        assert_eq!(resolve(Choice::Ansi256, &hostile), Depth::Ansi256);
        assert_eq!(
            resolve(Choice::Never, &env(Some("truecolor"), None)),
            Depth::None
        );
        assert_eq!(resolve(Choice::Auto, &hostile), Depth::None);
    }

    fn painted(color: Color) -> Buffer {
        let mut buffer = Buffer::empty(ratatui::layout::Rect::new(0, 0, 1, 1));
        buffer.content[0].fg = color;
        buffer.content[0].bg = color;
        buffer
    }

    #[test]
    fn truecolor_leaves_the_frame_exactly_as_rendered() {
        let mut buffer = painted(Color::Rgb(1, 2, 3));
        degrade(&mut buffer, Depth::Truecolor);
        assert_eq!(buffer.content[0].fg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn the_palette_pass_rewrites_both_halves_of_a_half_block_cell() {
        let mut buffer = painted(Color::Rgb(255, 0, 0));
        degrade(&mut buffer, Depth::Ansi256);
        assert_eq!(buffer.content[0].fg, Color::Indexed(196));
        assert_eq!(buffer.content[0].bg, Color::Indexed(196));
    }

    #[test]
    fn a_colour_the_terminal_already_defines_is_passed_through_untouched() {
        let mut buffer = painted(Color::Indexed(42));
        degrade(&mut buffer, Depth::Ansi256);
        assert_eq!(buffer.content[0].fg, Color::Indexed(42));

        let mut buffer = painted(Color::Reset);
        degrade(&mut buffer, Depth::Ansi256);
        assert_eq!(buffer.content[0].fg, Color::Reset);
    }

    #[test]
    fn no_colour_leaves_the_modifiers_that_carry_meaning_without_it() {
        let mut buffer = painted(Color::Rgb(255, 236, 39));
        buffer.content[0].modifier = Modifier::REVERSED;
        degrade(&mut buffer, Depth::None);

        assert_eq!(buffer.content[0].fg, Color::Reset);
        assert_eq!(buffer.content[0].bg, Color::Reset);
        assert_eq!(buffer.content[0].modifier, Modifier::REVERSED);
    }
}
