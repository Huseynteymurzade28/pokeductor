//! Colour palettes.
//!
//! Every colour the interface draws comes from this module, which is what
//! makes a second palette a matter of filling in another set of values rather
//! than hunting through `ui.rs`. Sprites are composited over the palette's own
//! background rather than a hardcoded navy, and `color.rs` degrades whatever
//! comes out of here for terminals that cannot show truecolor, so a palette
//! has to be right in 24-bit and is then correct everywhere else by
//! construction.
//!
//! The slot names are PICO-8's, because that is the palette the interface was
//! designed in and the names are what the rendering code already reads. In
//! another palette they are slots rather than promises about hue: `pink()` on
//! a Game Boy is a shade of green, and the meaning that matters — "the colour
//! a Mythical badge is drawn in" — is the same in both.
//!
//! The palette in force is process-wide. It is chosen once, from the flag or
//! the restored session, before the first frame is drawn, and threading it
//! through every rendering function instead would obscure the very code this
//! module exists to keep readable.

use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::Color;

/// A palette the interface can be drawn in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    /// PICO-8's palette: warm yellows and matte blues. The default, and what
    /// every screenshot in the README was taken in.
    #[default]
    Pico8,
    /// The Game Boy DMG's greens. A Pokedex in DMG green is a joke worth
    /// making, and it needs no new rendering to make it — only other numbers.
    Dmg,
}

impl Theme {
    /// Every palette, in the order `--theme` lists them.
    pub const ALL: [Theme; 2] = [Theme::Pico8, Theme::Dmg];

    /// Stable name used by the flag and the session file, so that reordering
    /// this enum can never change what a stored session means.
    pub fn code(self) -> &'static str {
        match self {
            Theme::Pico8 => "pico8",
            Theme::Dmg => "dmg",
        }
    }

    /// The inverse of [`code`](Self::code). A name this build does not have —
    /// a session written by a later one — is `None`, and the caller keeps its
    /// default rather than failing.
    pub fn from_code(code: &str) -> Option<Self> {
        Theme::ALL.into_iter().find(|theme| theme.code() == code)
    }

    /// Name for humans, for `--help` and anywhere the choice is shown.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Pico8 => "PICO-8",
            Theme::Dmg => "Game Boy",
        }
    }

    /// The values this palette draws with.
    pub fn palette(self) -> &'static Palette {
        match self {
            Theme::Pico8 => &PICO8,
            Theme::Dmg => &DMG,
        }
    }
}

/// One complete set of colours, as raw components: the degradation pass in
/// `color.rs` and the sprite compositor both want the numbers, not a `Color`.
pub struct Palette {
    /// Panel background. Sprite pixels are alpha-blended onto it, so it is the
    /// one slot whose components are read directly.
    pub base: Rgb,
    /// A lifted background for bars, cards and other secondary surfaces.
    pub surface: Rgb,
    /// Dim borders and de-emphasised text.
    pub overlay: Rgb,
    pub text: Rgb,
    /// Muted labels.
    pub subtext: Rgb,
    /// Primary accent: titles, focused borders and the selection bar.
    pub mauve: Rgb,
    /// Water, and the search icon.
    pub blue: Rgb,
    pub sapphire: Rgb,
    /// The five below double as the stat ramp, weakest to strongest, so a
    /// palette has to keep them in that order for a stat bar to read as one.
    pub red: Rgb,
    pub peach: Rgb,
    pub yellow: Rgb,
    pub green: Rgb,
    pub teal: Rgb,
    pub pink: Rgb,
    pub lavender: Rgb,
    /// How the palette draws a colour that is not one of its own — a sprite
    /// pixel, after compositing. PICO-8 shows it as it is; a Game Boy has four
    /// shades and no choice about it.
    pub ink: fn(Rgb) -> Rgb,
    /// The accent for a type chip. Kept as a function because the two palettes
    /// answer it differently in kind, not only in value: one has eighteen
    /// hues, the other has none to spare.
    pub type_accent: fn(&str) -> Rgb,
}

/// Raw colour components, before they reach ratatui.
pub type Rgb = (u8, u8, u8);

// --- PICO-8 ---------------------------------------------------------------

/// The palette the interface was designed in: PICO-8's sixteen colours, warm
/// yellows on a matte navy.
static PICO8: Palette = Palette {
    base: (29, 43, 83),
    surface: (41, 54, 111),
    overlay: (131, 118, 156),
    text: (255, 241, 232),
    subtext: (194, 195, 199),
    mauve: (255, 236, 39),
    blue: (41, 173, 255),
    sapphire: (41, 173, 255),
    red: (255, 0, 77),
    peach: (255, 163, 0),
    yellow: (255, 236, 39),
    green: (0, 228, 54),
    teal: (43, 210, 200),
    pink: (255, 119, 168),
    lavender: (255, 204, 170),
    ink: |rgb| rgb,
    type_accent: pico8_type_accent,
};

/// Accent for a Pokemon type slug, in PICO-8's own colours.
fn pico8_type_accent(type_name: &str) -> Rgb {
    match type_name {
        "normal" => (194, 195, 199),
        "fire" => PICO8.peach,
        "water" => PICO8.blue,
        "electric" => PICO8.yellow,
        "grass" => PICO8.green,
        "ice" => (130, 220, 255),
        "fighting" => PICO8.red,
        "poison" => (199, 87, 197),
        "ground" => (171, 82, 54),
        "flying" => (160, 200, 255),
        "psychic" => PICO8.pink,
        "bug" => (140, 200, 60),
        "rock" => (171, 140, 100),
        "ghost" => (131, 118, 156),
        "dragon" => (120, 130, 240),
        "dark" => (130, 120, 140),
        "steel" => (160, 178, 196),
        "fairy" => PICO8.pink,
        _ => PICO8.subtext,
    }
}

// --- Game Boy DMG ---------------------------------------------------------

/// The DMG's four shades, darkest first, as the screen showed them.
const DMG_SHADES: [Rgb; 4] = [(15, 56, 15), (48, 98, 48), (139, 172, 15), (155, 188, 15)];

/// The Game Boy palette.
///
/// Four shades is what the hardware had, and what sprites are quantised to.
/// The interface needs a couple of steps the hardware never had to draw — a
/// border dim enough to recede but not so dim it vanishes into the background,
/// and a stat ramp with more than four rungs — so two slots are mixed from the
/// four rather than being one of them. The alternative is a border that is
/// either invisible or indistinguishable from the text.
static DMG: Palette = Palette {
    base: DMG_SHADES[0],
    surface: (34, 74, 26),
    overlay: (90, 130, 40),
    text: DMG_SHADES[3],
    subtext: DMG_SHADES[2],
    mauve: DMG_SHADES[3],
    blue: DMG_SHADES[2],
    sapphire: DMG_SHADES[2],
    // Weakest to strongest: a stat bar climbs out of the background towards
    // the brightest green the screen had.
    red: (70, 110, 35),
    peach: (110, 150, 30),
    yellow: (130, 165, 22),
    green: DMG_SHADES[2],
    teal: DMG_SHADES[3],
    pink: DMG_SHADES[2],
    lavender: DMG_SHADES[3],
    ink: dmg_quantise,
    type_accent: |_| DMG_SHADES[2],
};

/// Snaps a colour to the nearest of the DMG's four shades by brightness.
///
/// This is what a Game Boy did to everything it was ever shown, and it is the
/// whole reason a sprite drawn in this palette looks like one: keeping the
/// artwork in full colour over a green interface would only look broken.
fn dmg_quantise((r, g, b): Rgb) -> Rgb {
    // Rec. 601 luma, in integers: the coefficients are the usual 0.299 /
    // 0.587 / 0.114 scaled by 1000, which keeps the whole thing in `u32`.
    let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
    DMG_SHADES[(luma as usize * DMG_SHADES.len() / 256).min(DMG_SHADES.len() - 1)]
}

// --- The palette in force -------------------------------------------------

/// Index into [`Theme::ALL`] of the palette being drawn in.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Draws everything from here on in `theme`. Called once, before the first
/// frame, with whatever the flag and the restored session settled on.
pub fn use_theme(theme: Theme) {
    let index = Theme::ALL.iter().position(|&t| t == theme).unwrap_or(0);
    CURRENT.store(index, Ordering::Relaxed);
}

/// The palette in force.
pub fn current() -> Theme {
    Theme::ALL[CURRENT.load(Ordering::Relaxed).min(Theme::ALL.len() - 1)]
}

fn palette() -> &'static Palette {
    current().palette()
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

/// Raw components of [`base`], for alpha-blending sprite pixels onto the panel
/// background.
pub fn base_rgb() -> Rgb {
    palette().base
}

pub fn base() -> Color {
    color(palette().base)
}
pub fn surface() -> Color {
    color(palette().surface)
}
pub fn overlay() -> Color {
    color(palette().overlay)
}
pub fn text() -> Color {
    color(palette().text)
}
pub fn subtext() -> Color {
    color(palette().subtext)
}
pub fn mauve() -> Color {
    color(palette().mauve)
}
pub fn sapphire() -> Color {
    color(palette().sapphire)
}
pub fn red() -> Color {
    color(palette().red)
}
pub fn peach() -> Color {
    color(palette().peach)
}
pub fn yellow() -> Color {
    color(palette().yellow)
}
pub fn green() -> Color {
    color(palette().green)
}
pub fn teal() -> Color {
    color(palette().teal)
}
pub fn pink() -> Color {
    color(palette().pink)
}
pub fn lavender() -> Color {
    color(palette().lavender)
}

/// A sprite pixel in the palette's own ink, after it has been composited over
/// the background.
pub fn ink(rgb: Rgb) -> Color {
    color((palette().ink)(rgb))
}

/// Colour used to draw a stat bar, scaled by how high the value is so that
/// weak stats read "warm/alarming" and strong stats read "cool/calm" — or, in
/// a palette with one hue, dim and bright.
pub fn stat_color(base: u16) -> Color {
    let p = palette();
    color(match base {
        0..=49 => p.red,
        50..=89 => p.peach,
        90..=119 => p.yellow,
        120..=149 => p.green,
        _ => p.teal,
    })
}

/// Accent color for a Pokemon type slug (e.g. `"fire"`).
///
/// Chips are drawn as this colour behind [`base`], so every palette has to
/// answer with something the background text stays legible against. Eighteen
/// distinguishable hues is more than a Game Boy has, which is why the type's
/// name is written in the chip rather than left to the colour.
pub fn type_color(type_name: &str) -> Color {
    color((palette().type_accent)(type_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance, for asserting that one colour reads as lighter
    /// than another without pinning either to a value.
    fn luma((r, g, b): Rgb) -> u32 {
        299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)
    }

    #[test]
    fn every_theme_reads_back_out_of_its_code() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_code(theme.code()), Some(theme));
        }
        assert_eq!(Theme::from_code("gameboy"), None);
        assert_eq!(Theme::from_code(""), None);
    }

    #[test]
    fn the_default_palette_is_the_one_the_interface_was_designed_in() {
        assert_eq!(Theme::default(), Theme::Pico8);
        assert_eq!(current(), Theme::Pico8, "and nothing has switched it");
        assert_eq!(base(), Color::Rgb(29, 43, 83));
    }

    /// The five slots a stat bar is drawn from, weakest rung first.
    fn ramp(theme: Theme) -> [Rgb; 5] {
        let p = theme.palette();
        [p.red, p.peach, p.yellow, p.green, p.teal]
    }

    #[test]
    fn no_rung_of_a_stat_bar_disappears_into_the_panel() {
        for theme in Theme::ALL {
            for rung in ramp(theme) {
                assert!(
                    luma(rung) > luma(theme.palette().base) + 20_000,
                    "{} loses {rung:?} against its background",
                    theme.label()
                );
            }
        }
    }

    #[test]
    fn the_game_boys_stat_bar_carries_the_scale_in_brightness() {
        // PICO-8 says "weak" and "strong" in hue — red through orange to
        // teal — and its yellow is brighter than its green, deliberately. A
        // palette with one hue has only brightness left to say it with, so
        // there the five rungs have to climb, and a swapped pair would turn
        // the bar back into decoration.
        for pair in ramp(Theme::Dmg).windows(2) {
            assert!(
                luma(pair[0]) < luma(pair[1]),
                "the ramp goes backwards at {pair:?}"
            );
        }
    }

    #[test]
    fn every_palette_keeps_its_text_off_its_background() {
        for theme in Theme::ALL {
            let p = theme.palette();
            for (name, slot) in [("text", p.text), ("subtext", p.subtext), ("mauve", p.mauve)] {
                assert!(
                    luma(slot) > luma(p.base) + 40_000,
                    "{} draws {name} too close to its background",
                    theme.label()
                );
            }
            // A border has to recede without disappearing: dimmer than the
            // text, brighter than what it is drawn on.
            assert!(luma(p.base) < luma(p.overlay) && luma(p.overlay) < luma(p.text));
        }
    }

    #[test]
    fn a_type_chip_is_legible_in_every_palette() {
        // Chips are the type's accent behind the background colour, so the
        // two have to stay apart in every palette — including for the types
        // that fall through to the default.
        for theme in Theme::ALL {
            let p = theme.palette();
            for slug in [
                "normal", "fire", "water", "electric", "grass", "ice", "fighting", "poison",
                "ground", "flying", "psychic", "bug", "rock", "ghost", "dragon", "dark", "steel",
                "fairy", "stellar",
            ] {
                let chip = (p.type_accent)(slug);
                assert!(
                    luma(chip) > luma(p.base) + 40_000,
                    "{} draws a {slug} chip you cannot read",
                    theme.label()
                );
            }
        }
    }

    #[test]
    fn the_game_boy_has_only_its_four_shades_to_draw_a_sprite_with() {
        let quantised: Vec<Rgb> = [
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 77),
            (41, 173, 255),
            (130, 130, 130),
        ]
        .into_iter()
        .map(dmg_quantise)
        .collect();
        for shade in &quantised {
            assert!(
                DMG_SHADES.contains(shade),
                "{shade:?} is not a Game Boy green"
            );
        }
        // Black and white land at opposite ends rather than in the middle.
        assert_eq!(quantised[0], DMG_SHADES[0]);
        assert_eq!(quantised[1], DMG_SHADES[3]);
        // PICO-8 leaves a sprite exactly as it found it.
        assert_eq!((PICO8.ink)((255, 0, 77)), (255, 0, 77));
    }
}
