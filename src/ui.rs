//! All `ratatui` rendering. Pure functions of [`App`] state — given the same
//! state they always draw the same frame, which keeps the loop trivial.

use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, SortKey};
use crate::color;
use crate::compare;
use crate::i18n::{EvoStrings, Language, Strings};
use crate::models::{title_case, EvolutionTree, LearnMethod, LearnedMove, PokemonDetail, Sprite};
use crate::team::{self, AbilityImmunity};
use crate::theme;
use crate::typechart;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Column width reserved for stat labels (longest is "Verteid."/"Sp. Def").
const STAT_LABEL_WIDTH: usize = 9;

/// Entry point called once per frame by the event loop.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let strings = app.language.strings();

    // Paint the whole background first so gaps share the pastel base color.
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BASE)),
        area,
    );

    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(0),    // body
        Constraint::Length(1), // footer / help
    ])
    .split(area);

    render_header(frame, app, &strings, rows[0]);
    render_footer(frame, &strings, rows[2]);

    let cols =
        Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(68)]).split(rows[1]);

    render_sidebar(frame, app, &strings, cols[0]);

    let right =
        Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)]).split(cols[1]);
    render_details(frame, app, &strings, right[0]);
    render_evolution(frame, app, &strings, right[1]);

    // The overlay cards float above everything when open. Only one can be open
    // at a time (input is modal), so the draw order is arbitrary.
    if app.matchups {
        render_matchups(frame, app, &strings, area);
    }
    if app.ability_card {
        render_abilities(frame, app, &strings, area);
    }
    if app.moves_card {
        render_moves(frame, app, &strings, area);
    }
    if app.team_card {
        render_team(frame, app, &strings, area);
    }
    if app.language_picker {
        render_language_picker(frame, app, &strings, area);
    }
    if app.evo_card {
        render_evolution_card(frame, app, &strings, area);
    }
    if app.compare_card {
        render_compare(frame, app, &strings, area);
    }
    // Drawn last: help must land on top of whatever it is explaining.
    if app.help_card {
        render_help(frame, &strings, area);
    }
}

fn render_header(frame: &mut Frame, app: &App, s: &Strings, area: Rect) {
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(12)]).split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        s.app_title,
        Style::default()
            .fg(theme::MAUVE)
            .add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(title, cols[0]);

    let tag = Paragraph::new(Line::from(vec![
        Span::styled("◐ ", Style::default().fg(theme::PEACH)),
        Span::styled(
            app.language.tag(),
            Style::default()
                .fg(theme::PEACH)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .alignment(Alignment::Right);
    frame.render_widget(tag, cols[1]);
}

fn render_footer(frame: &mut Frame, s: &Strings, area: Rect) {
    let footer = Paragraph::new(Line::from(Span::styled(
        s.help,
        Style::default().fg(theme::SUBTEXT),
    )))
    .style(Style::default().bg(theme::SURFACE))
    .alignment(Alignment::Center);
    frame.render_widget(footer, area);
}

fn render_sidebar(frame: &mut Frame, app: &mut App, s: &Strings, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);

    // --- Search box ---
    let search_focused = app.focus == Focus::Search;
    let search_block = panel_block(s.search_title, search_focused);
    let cursor = if search_focused { "▏" } else { "" };
    let query_line = if app.query.is_empty() && !search_focused {
        Line::from(Span::styled(
            s.search_hint,
            Style::default().fg(theme::OVERLAY),
        ))
    } else {
        Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(theme::SAPPHIRE)),
            Span::styled(app.query.clone(), Style::default().fg(theme::TEXT)),
            Span::styled(cursor, Style::default().fg(theme::MAUVE)),
        ])
    };
    frame.render_widget(Paragraph::new(query_line).block(search_block), rows[0]);

    // --- List ---
    let list_focused = app.focus == Focus::List;
    let sort_badge = match app.sort {
        SortKey::Dex => s.sort_dex,
        SortKey::Name => s.sort_name,
    };
    let title = format!(
        "{}({}) ⇅ {} ",
        s.sidebar_title,
        app.filtered.len(),
        sort_badge
    );
    let list_block = panel_block_owned(title, list_focused);
    let inner = list_block.inner(rows[1]);
    frame.render_widget(&list_block, rows[1]);

    if app.list_loading {
        render_centered_loading(frame, inner, s.loading_list, app.spinner);
        return;
    }
    // A `type:`, `ability:` or `egg:` filter cannot match anything until its
    // roster arrives, so say that rather than claiming the search found
    // nothing.
    if app.awaiting_roster() {
        render_centered_loading(frame, inner, s.loading_filter, app.spinner);
        return;
    }
    if app.filtered.is_empty() {
        render_centered_text(frame, inner, s.no_results, theme::OVERLAY);
        return;
    }

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .filter_map(|&idx| app.all_pokemon.get(idx))
        .map(|p| {
            // Alternate forms have no dex number; their column stays blank so
            // the names below still line up.
            let dex = match p.dex_number() {
                Some(number) => format!("{number:>4} "),
                None => " ".repeat(5),
            };
            // Two slots, each with a meaning of its own: the comparison pin on
            // the left, party membership on the right. A species can be both,
            // and each keeps its column whether or not the other is there, so
            // a marker always means the same thing in the same place.
            let pin = if app.is_pinned(&p.name) { "◆" } else { " " };
            let party = if app.is_in_team(&p.name) { "●" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(pin, Style::default().fg(theme::TEAL)),
                Span::styled(party, Style::default().fg(theme::GREEN)),
                Span::styled(dex, Style::default().fg(theme::OVERLAY)),
                Span::styled(title_case(&p.name), Style::default().fg(theme::TEXT)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_symbol("▶ ")
        .highlight_style(color::highlight(theme::MAUVE).add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, inner, &mut app.list_state);
}

fn render_details(frame: &mut Frame, app: &App, s: &Strings, area: Rect) {
    let block = panel_block(s.details_title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.detail_is_loading() {
        render_centered_loading(frame, inner, s.loading, app.spinner);
        return;
    }

    let Some(detail) = app.selected_detail() else {
        match &app.error {
            Some(err) => render_error(frame, inner, s, err),
            None => render_centered_text(frame, inner, s.no_selection, theme::OVERLAY),
        }
        return;
    };

    // Carve out a square column on the left for the sprite when the panel is
    // wide and tall enough to host one; otherwise the info text spans the full
    // width as before.
    let info = match app.selected_sprite() {
        Some(sprite) if inner.width >= 46 && inner.height >= 6 => {
            let sprite_w = sprite_col_width(inner);
            let cols = Layout::horizontal([
                Constraint::Length(sprite_w),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);
            render_sprite(frame, cols[0], sprite);
            cols[2]
        }
        _ => inner,
    };

    let mut lines: Vec<Line> = Vec::new();

    let mut title_spans = vec![
        Span::styled(
            title_case(&detail.name),
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   #{:04}", detail.dex_number),
            Style::default().fg(theme::OVERLAY),
        ),
    ];
    // Say so when the artwork is shiny: an unfamiliar palette otherwise reads
    // as a rendering bug rather than a deliberate choice.
    if app.sprite_variant.is_shiny() {
        title_spans.push(Span::styled(
            format!("  ✦ {}", s.shiny_label),
            Style::default()
                .fg(theme::YELLOW)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(title_spans));

    // Pokedex genus, e.g. "Seed Pokémon" — the headline of the info card, in the
    // active language where PokeAPI has it.
    let lang_code = app.language.flavor_code();
    if let Some(genus) = detail.genus_for(lang_code) {
        lines.push(Line::from(Span::styled(
            genus.to_string(),
            Style::default()
                .fg(theme::PEACH)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    // Special-category badges (Legendary / Mythical / Baby), as little chips.
    let mut badges: Vec<(&str, ratatui::style::Color)> = Vec::new();
    if detail.is_legendary {
        badges.push((s.legendary_label, theme::YELLOW));
    }
    if detail.is_mythical {
        badges.push((s.mythical_label, theme::PINK));
    }
    if detail.is_baby {
        badges.push((s.baby_label, theme::TEAL));
    }
    if !badges.is_empty() {
        let mut spans = Vec::new();
        for (label, color) in badges {
            spans.push(Span::styled(
                format!(" ✦ {label} "),
                Style::default()
                    .fg(theme::BASE)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }

    // Type chips.
    let mut type_spans = vec![Span::styled(
        format!("{}: ", s.types_label),
        Style::default().fg(theme::SUBTEXT),
    )];
    type_spans.extend(type_chips(&detail.types));
    lines.push(Line::from(type_spans));

    // Ability names. These come in the same payload as the types, so the row
    // costs nothing; the descriptions behind `A` are what need a request.
    //
    // Three abilities plus a "hidden" marker overrun a narrow panel, so the
    // row is wrapped onto continuation lines rather than clipped: a name cut
    // off halfway is worse than one on the next line.
    if !detail.abilities.is_empty() {
        let label = format!("{}: ", s.abilities_label);
        let entries: Vec<String> = detail
            .abilities
            .iter()
            .map(|ability| {
                let name = ability_display_name(app, &ability.name);
                match ability.is_hidden {
                    true => format!("{name} ({})", s.ability_hidden),
                    false => name,
                }
            })
            .collect();

        let indent = " ".repeat(label.chars().count());
        let budget = (info.width as usize).saturating_sub(label.chars().count());
        for (row, text) in wrap_plain(&entries.join(" · "), budget.max(8))
            .into_iter()
            .enumerate()
        {
            lines.push(Line::from(vec![
                Span::styled(
                    if row == 0 {
                        label.clone()
                    } else {
                        indent.clone()
                    },
                    Style::default().fg(theme::SUBTEXT),
                ),
                Span::styled(text, Style::default().fg(theme::TEXT)),
            ]));
        }
    }

    lines.push(Line::from(vec![
        Span::styled(
            format!("{}: ", s.height_label),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(
            format!("{:.1} m", detail.height as f32 / 10.0),
            Style::default().fg(theme::TEXT),
        ),
        Span::raw("    "),
        Span::styled(
            format!("{}: ", s.weight_label),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(
            format!("{:.1} kg", detail.weight as f32 / 10.0),
            Style::default().fg(theme::TEXT),
        ),
    ]));
    lines.push(Line::raw(""));

    // Stat bars sized to the available width.
    let bar_width = (info.width as usize).saturating_sub(STAT_LABEL_WIDTH + 6);
    for stat in &detail.stats {
        lines.push(stat_line(
            app.language.stat_label(stat.kind),
            stat.base,
            bar_width,
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!("{}: ", s.total_label),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(
            detail.stat_total().to_string(),
            Style::default()
                .fg(theme::LAVENDER)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // When there's a flavor blurb and room to show it, split a small card off
    // the bottom of the info column for it; otherwise the stats use all of it.
    // Prefer PokeAPI's native blurb, then a cached machine translation, then the
    // English original as a last resort.
    let flavor = detail
        .flavors
        .get(lang_code)
        .map(String::as_str)
        .or_else(|| app.translation_for(&detail.name, lang_code))
        .or_else(|| detail.flavors.get("en").map(String::as_str));

    let flavor_rows = 4;
    match flavor {
        Some(flavor) if info.height as usize > lines.len() + flavor_rows => {
            let split =
                Layout::vertical([Constraint::Min(0), Constraint::Length(flavor_rows as u16)])
                    .split(info);
            frame.render_widget(Paragraph::new(lines), split[0]);
            render_flavor_card(frame, split[1], flavor);
        }
        _ => frame.render_widget(Paragraph::new(lines), info),
    }
}

/// Renders the Pokedex flavor-text blurb as a quoted, word-wrapped little card.
fn render_flavor_card(frame: &mut Frame, area: Rect, flavor: &str) {
    let para = Paragraph::new(vec![Line::from(Span::styled(
        format!("“{flavor}”"),
        Style::default()
            .fg(theme::SUBTEXT)
            .add_modifier(Modifier::ITALIC),
    ))])
    .wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

// --- Sprite rendering ----------------------------------------------------

/// Maximum cell width we'll ever give a sprite, so it stays a tasteful accent
/// rather than swallowing the panel on very wide terminals.
const MAX_SPRITE_COLS: u16 = 40;

/// Chooses the sprite column width: square-ish, bounded by ~40% of the panel
/// width, the available height (two pixels per cell row), and [`MAX_SPRITE_COLS`].
fn sprite_col_width(inner: Rect) -> u16 {
    let by_width = inner.width * 2 / 5;
    let by_height = inner.height.saturating_mul(2);
    let w = by_width.min(by_height).min(MAX_SPRITE_COLS);
    (w & !1).max(2) // keep it even so rows = cols / 2 divides cleanly
}

/// Draws `sprite` into `area`, capped at [`MAX_SPRITE_COLS`] columns.
fn render_sprite(frame: &mut Frame, area: Rect, sprite: &Sprite) {
    render_sprite_capped(frame, area, sprite, MAX_SPRITE_COLS);
}

/// Draws `sprite` into `area` using upper-half-block characters: each cell packs
/// two vertical pixels (foreground = top, background = bottom), so one terminal
/// row shows two image rows.
///
/// The artwork is first cropped to its opaque bounding box (PokeAPI sprites have
/// a wide transparent margin), then scaled to the largest size that fits `area`
/// and `max_cols` *while preserving aspect ratio* — accounting for terminal
/// cells being roughly twice as tall as they are wide — and finally centred.
fn render_sprite_capped(frame: &mut Frame, area: Rect, sprite: &Sprite, max_cols: u16) {
    if area.width < 2 || area.height < 1 || sprite.width() == 0 || sprite.height() == 0 {
        return;
    }

    // Crop to the visible Pokemon so it fills the box instead of floating in
    // empty space.
    let (bx0, by0, bx1, by1) = sprite.content_bounds();
    let bw = (bx1 - bx0 + 1) as f32;
    let bh = (by1 - by0 + 1) as f32;

    // Fit the cropped box into the available pixel grid (width in cells, height
    // in half-cells) keeping its proportions.
    let max_w = area.width.min(max_cols) as f32;
    let max_h_px = (area.height as f32) * 2.0;
    let scale = (max_w / bw).min(max_h_px / bh);
    let cols = (((bw * scale) as u16).max(2)) & !1; // even, so columns map cleanly
    let rows = ((bh * scale) as u16).div_ceil(2).max(1);

    let bw = bw as u32;
    let bh = bh as u32;
    let cols_u = cols as u32;
    let sub_rows = 2 * rows as u32; // each cell row carries two vertical pixels

    // Source box covered by output column `cx` / sub-row `py`, in image pixels.
    let span_x = |cx: u32| {
        (
            bx0 + cx * bw / cols_u,
            bx0 + ((cx + 1) * bw / cols_u).saturating_sub(1),
        )
    };
    let span_y = |py: u32| {
        (
            by0 + py * bh / sub_rows,
            by0 + ((py + 1) * bh / sub_rows).saturating_sub(1),
        )
    };

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for cy in 0..rows {
        let (ty0, ty1) = span_y(2 * cy as u32);
        let (by_0, by_1) = span_y(2 * cy as u32 + 1);
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for cx in 0..cols {
            let (sx0, sx1) = span_x(cx as u32);
            let top = pixel_color(sprite.box_average(sx0, ty0, sx1, ty1));
            let bottom = pixel_color(sprite.box_average(sx0, by_0, sx1, by_1));
            spans.push(Span::styled("▀", Style::default().fg(top).bg(bottom)));
        }
        lines.push(Line::from(spans));
    }

    // Centre the block within the allotted area.
    let target = Rect {
        x: area.x + (area.width.saturating_sub(cols)) / 2,
        y: area.y + (area.height.saturating_sub(rows)) / 2,
        width: cols,
        height: rows,
    };
    frame.render_widget(Paragraph::new(lines), target);
}

/// Maps an averaged RGBA pixel to a terminal colour by alpha-compositing it over
/// the panel background. Blending (rather than a hard transparency threshold)
/// lets sprite edges fade cleanly into the UI instead of leaving a dark fringe.
fn pixel_color(rgba: [u8; 4]) -> Color {
    let a = rgba[3] as u16;
    if a == 0 {
        return theme::BASE;
    }
    let (br, bg, bb) = theme::BASE_RGB;
    let mix = |fg: u8, bg: u8| ((fg as u16 * a + bg as u16 * (255 - a)) / 255) as u8;
    Color::Rgb(mix(rgba[0], br), mix(rgba[1], bg), mix(rgba[2], bb))
}

fn render_evolution(frame: &mut Frame, app: &App, s: &Strings, area: Rect) {
    let focused = app.focus == Focus::Evolution;
    let block = if app.sprite_variant.is_shiny() {
        panel_block_owned(
            format!("{}✦ {} ", s.evolution_title, s.shiny_label),
            focused,
        )
    } else {
        panel_block(s.evolution_title, focused)
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.detail_is_loading() {
        render_centered_loading(frame, inner, s.loading, app.spinner);
        return;
    }

    let Some(tree) = app.selected_evolution() else {
        if app.selected_detail().is_some() {
            render_centered_text(frame, inner, s.no_evolution, theme::OVERLAY);
        } else {
            render_centered_text(frame, inner, s.no_selection, theme::OVERLAY);
        }
        return;
    };

    // Highlight the chain node matching the displayed species (forms like
    // "raichu-alola" map back to their base "raichu" node).
    let current = app
        .selected_detail()
        .map(|d| d.species.as_str())
        .or(app.selected_name.as_deref());
    // Only when focused does the cursor highlight a specific member.
    let cursor_name = if focused {
        app.chain_names().get(app.evo_cursor).cloned()
    } else {
        None
    };
    let cursor = cursor_name.as_deref();

    // Reserve the bottom row for a context hint.
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    draw_chain(frame, app, s, tree, current, cursor, rows[0]);

    let fallback = if focused {
        s.evo_nav_hint
    } else {
        s.expand_hint
    };
    frame.render_widget(
        Paragraph::new(chain_hint(tree, cursor, s, fallback)).alignment(Alignment::Center),
        rows[1],
    );
}

/// The full-screen evolution view: the same chain renderer, handed the whole
/// terminal rather than one panel.
///
/// Wide chains — Eevee's eight branches, Tyrogue, Wurmple, the regional-form
/// lines — need more rows than the evolution panel can ever offer, so there they
/// degrade to the compact text tree, which is exactly the case the sprite cards
/// would help most with. This view is how they get to ask for the space.
fn render_evolution_card(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let Some(tree) = app.selected_evolution() else {
        return; // nothing loaded to expand
    };
    if full.width < MIN_CARD_W + 2 || full.height < MIN_CARD_H + 3 {
        return; // too cramped to be readable; leave the main view alone
    }

    frame.render_widget(Clear, full);

    // The sprite pixels are composited over `theme::BASE`, so the card behind
    // them has to be that same colour or every sprite picks up a halo.
    let title = if app.sprite_variant.is_shiny() {
        format!("{}✦ {} ", s.evolution_title, s.shiny_label)
    } else {
        s.evolution_title.to_string()
    };
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BASE));
    let inner = block.inner(full);
    frame.render_widget(block, full);

    let current = app
        .selected_detail()
        .map(|d| d.species.as_str())
        .or(app.selected_name.as_deref());
    // The card is modal, so its cursor is always live — unlike the panel's,
    // which only lights up while the panel holds focus.
    let cursor_name = app.chain_names().get(app.evo_cursor).cloned();
    let cursor = cursor_name.as_deref();

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    draw_chain(frame, app, s, tree, current, cursor, rows[0]);
    frame.render_widget(
        Paragraph::new(chain_hint(tree, cursor, s, s.evo_card_hint)).alignment(Alignment::Center),
        rows[1],
    );
}

/// Draws a chain onto `canvas`: the sprite graph when every card has room,
/// otherwise the compact text tree so cramped terminals still show the
/// relationships.
fn draw_chain(
    frame: &mut Frame,
    app: &App,
    s: &Strings,
    tree: &EvolutionTree,
    current: Option<&str>,
    cursor: Option<&str>,
    canvas: Rect,
) {
    let depth = tree.depth() as u16;
    let leaves = tree.leaf_count() as u16;
    match card_grid(canvas, depth, leaves) {
        Some((col_w, lane_h)) => {
            // The grid rarely uses the canvas to the last row or column — lanes
            // divide it with a remainder, and wide canvases hit the card-width
            // cap — so centre what it does use rather than letting the leftover
            // pile up below and to the right of the chain.
            let canvas = centered_fixed(col_w * depth, lane_h * leaves, canvas);
            let mut lane = 0u16;
            place_node(
                frame, app, s, tree, current, cursor, canvas, col_w, lane_h, 0, &mut lane,
            );
        }
        None => {
            let lines = evolution_lines(tree, cursor.or(current), &s.evo, canvas.width);
            frame.render_widget(Paragraph::new(lines), canvas);
        }
    }
}

/// The sprite-card grid for a chain of `depth` stages and `leaves` branches on
/// `canvas`, or `None` when a card would come out smaller than
/// [`MIN_CARD_W`] × [`MIN_CARD_H`] and the text tree is the better rendering.
///
/// Every lane needs its own [`MIN_CARD_H`] rows, so the height a wide chain
/// asks for grows with its branches: Eevee's eight leaves want 32 rows, which
/// no panel in the right-hand column will ever have and a full screen usually
/// does. That difference is the whole point of the full-screen view.
fn card_grid(canvas: Rect, depth: u16, leaves: u16) -> Option<(u16, u16)> {
    let col_w = canvas.width.checked_div(depth)?.min(MAX_CARD_W + EVO_GAP);
    let lane_h = canvas.height.checked_div(leaves)?;
    (col_w >= MIN_CARD_W && lane_h >= MIN_CARD_H).then_some((col_w, lane_h))
}

/// The bottom row under a chain. While the cursor sits on a member it doubles
/// as a requirement readout, spelling out in full what it takes to get there —
/// the cards only have room for the headline condition; otherwise it carries
/// `fallback`, whatever the view wants to say about its own keys.
fn chain_hint(
    tree: &EvolutionTree,
    cursor: Option<&str>,
    s: &Strings,
    fallback: &'static str,
) -> Line<'static> {
    let requirement = cursor
        .and_then(|name| tree.find(name))
        .and_then(|node| node.condition.as_ref())
        .map(|condition| s.evo.summary(condition))
        .filter(|text| !text.is_empty());

    match requirement {
        Some(text) => Line::from(vec![
            Span::styled("✦ ", Style::default().fg(theme::PEACH)),
            Span::styled(text, Style::default().fg(theme::LAVENDER)),
        ]),
        None => Line::from(Span::styled(fallback, Style::default().fg(theme::OVERLAY))),
    }
}

// --- Small rendering helpers ---------------------------------------------

fn panel_block(title: &'static str, focused: bool) -> Block<'static> {
    panel_block_owned(title.to_string(), focused)
}

fn panel_block_owned(title: String, focused: bool) -> Block<'static> {
    // Focused panels glow warm yellow with a heavier double rule; resting panels
    // recede to a thin indigo frame — a retro DOS-panel feel.
    let (border, text, border_type) = if focused {
        (theme::MAUVE, theme::MAUVE, BorderType::Double)
    } else {
        (theme::OVERLAY, theme::SUBTEXT, BorderType::Plain)
    };
    Block::bordered()
        .border_type(border_type)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            title,
            Style::default().fg(text).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BASE))
}

fn stat_line(label: &str, base: u16, bar_width: usize) -> Line<'static> {
    let filled = if bar_width == 0 {
        0
    } else {
        ((base as usize * bar_width) / 255).min(bar_width)
    };
    Line::from(vec![
        Span::styled(
            format!("{label:<STAT_LABEL_WIDTH$}"),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(format!("{base:>3} "), Style::default().fg(theme::TEXT)),
        Span::styled(
            "█".repeat(filled),
            Style::default().fg(theme::stat_color(base)),
        ),
        Span::styled(
            "░".repeat(bar_width - filled),
            Style::default().fg(theme::SURFACE),
        ),
    ])
}

fn render_error(frame: &mut Frame, inner: Rect, s: &Strings, err: &str) {
    let para = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("⚠ {}", s.error_prefix),
            Style::default().fg(theme::RED).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            err.to_string(),
            Style::default().fg(theme::SUBTEXT),
        )),
    ])
    .wrap(ratatui::widgets::Wrap { trim: true });
    frame.render_widget(para, inner);
}

fn render_centered_text(frame: &mut Frame, inner: Rect, text: &str, color: ratatui::style::Color) {
    if inner.height == 0 {
        return;
    }
    let row = Rect {
        x: inner.x,
        y: inner.y + inner.height / 2,
        width: inner.width,
        height: 1,
    };
    let para = Paragraph::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(color),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(para, row);
}

fn render_centered_loading(frame: &mut Frame, inner: Rect, label: &str, spinner: usize) {
    if inner.height == 0 {
        return;
    }
    let frame_char = SPINNER[spinner % SPINNER.len()];
    let row = Rect {
        x: inner.x,
        y: inner.y + inner.height / 2,
        width: inner.width,
        height: 1,
    };
    let para = Paragraph::new(Line::from(vec![
        Span::styled(format!("{frame_char} "), Style::default().fg(theme::MAUVE)),
        Span::styled(format!("{label}…"), Style::default().fg(theme::SUBTEXT)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(para, row);
}

// --- Evolution tree rendering --------------------------------------------

/// Renders an [`EvolutionTree`] as a list of styled lines. Linear segments are
/// drawn horizontally (`A ──▶ B (Lv. 16) ──▶ C`); wherever a species branches,
/// the children are laid out vertically with `├──`/`└──` connectors. Each
/// member carries its evolution requirement in parentheses.
fn evolution_lines(
    tree: &EvolutionTree,
    highlight: Option<&str>,
    evo: &EvoStrings,
    width: u16,
) -> Vec<Line<'static>> {
    node_block(tree, highlight, evo, requirement_budget(width))
        .into_iter()
        .map(Line::from)
        .collect()
}

/// How many columns a requirement may take in the compact tree. Names and
/// connectors eat into the panel, so the budget grows with the panel but never
/// so far that a long location name pushes the tree off the right edge.
fn requirement_budget(width: u16) -> usize {
    (width as usize).saturating_sub(28).clamp(12, 40)
}

/// Returns the block of span-rows for `node` and its descendants, without any
/// outer indentation (the caller prepends connectors).
fn node_block(
    node: &EvolutionTree,
    highlight: Option<&str>,
    evo: &EvoStrings,
    budget: usize,
) -> Vec<Vec<Span<'static>>> {
    // Walk the linear run: follow single-child links onto one horizontal line.
    let mut run: Vec<&EvolutionTree> = vec![node];
    let mut cur = node;
    while cur.children.len() == 1 {
        cur = &cur.children[0];
        run.push(cur);
    }

    // Lay the run out left to right, tracking how wide it gets so any branch
    // connectors below can be indented under the last name.
    let mut first: Vec<Span<'static>> = Vec::new();
    let mut width = 0usize;
    let mut indent_width = 0usize;
    for (i, n) in run.iter().enumerate() {
        if i > 0 {
            first.push(Span::styled(" ──▶ ", Style::default().fg(theme::OVERLAY)));
            width += 5; // " ──▶ " is 5 columns
        }
        if i + 1 == run.len() {
            indent_width = width; // everything preceding the final name
        }
        first.push(name_span(&n.name, highlight));
        width += title_case(&n.name).chars().count();
        if let Some(label) = condition_label(n, evo, budget) {
            width += label.chars().count();
            first.push(Span::styled(label, Style::default().fg(theme::OVERLAY)));
        }
    }
    let mut lines = vec![first];

    // `cur` ends the run; if it branches, lay children out vertically beneath
    // the final name of the run.
    if cur.children.len() > 1 {
        let indent = " ".repeat(indent_width);

        let count = cur.children.len();
        for (i, child) in cur.children.iter().enumerate() {
            let is_last = i == count - 1;
            for (j, child_row) in node_block(child, highlight, evo, budget)
                .into_iter()
                .enumerate()
            {
                let connector = if j == 0 {
                    if is_last {
                        "└── "
                    } else {
                        "├── "
                    }
                } else if is_last {
                    "    "
                } else {
                    "│   "
                };
                let mut row = vec![Span::styled(
                    format!("{indent}{connector}"),
                    Style::default().fg(theme::OVERLAY),
                )];
                row.extend(child_row);
                lines.push(row);
            }
        }
    }

    lines
}

/// The parenthesised requirement suffix for a chain member in the compact text
/// tree, e.g. `" (Lv. 16)"`. `None` for a chain root, which nothing evolves into.
fn condition_label(node: &EvolutionTree, evo: &EvoStrings, budget: usize) -> Option<String> {
    let text = node.condition.as_ref().and_then(|c| evo.short(c))?;
    Some(format!(" ({})", truncate(&text, budget)))
}

/// Shortens `text` to `max` columns, marking the cut with an ellipsis.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    text.chars()
        .take(max - 1)
        .chain(std::iter::once('…'))
        .collect()
}

fn name_span(raw_name: &str, highlight: Option<&str>) -> Span<'static> {
    let style = if highlight == Some(raw_name) {
        Style::default()
            .fg(theme::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::GREEN)
    };
    Span::styled(title_case(raw_name), style)
}

// --- Evolution sprite graph ----------------------------------------------

/// Minimum cells a single sprite card needs to be worth drawing as art rather
/// than falling back to the compact text tree.
const MIN_CARD_W: u16 = 10;
const MIN_CARD_H: u16 = 4;
/// Columns reserved between generations for the connector arrows.
const EVO_GAP: u16 = 5;
/// Widest a card is allowed to get. Past this it is mostly whitespace: the
/// sprite is bounded by its lane height, and a name with its short requirement
/// rarely runs further. Capping it is what stops a full screen from spreading a
/// two-stage chain into two distant clusters with a connector stretched
/// between them.
const MAX_CARD_W: u16 = 30;

/// Recursively lays out `node` and its descendants. Each generation occupies a
/// fixed-width column; leaves are stacked into horizontal lanes. Returns the
/// vertical centre (absolute row) of this node's card so the caller can wire a
/// connector to it.
///
/// `current` is the species shown in the detail panel; `cursor` is the member
/// the navigation cursor sits on (only set while the panel is focused).
#[allow(clippy::too_many_arguments)]
fn place_node(
    frame: &mut Frame,
    app: &App,
    s: &Strings,
    node: &EvolutionTree,
    current: Option<&str>,
    cursor: Option<&str>,
    canvas: Rect,
    col_w: u16,
    lane_h: u16,
    depth_idx: u16,
    lane: &mut u16,
) -> u16 {
    let x = canvas.x + depth_idx * col_w;
    let card_w = col_w.saturating_sub(EVO_GAP);

    if node.children.is_empty() {
        let top = canvas.y + *lane * lane_h;
        *lane += 1;
        draw_card(frame, app, s, node, current, cursor, x, top, card_w, lane_h);
        return top + lane_h / 2;
    }

    // Place children first so we know where to anchor the connectors.
    let centers: Vec<u16> = node
        .children
        .iter()
        .map(|child| {
            place_node(
                frame,
                app,
                s,
                child,
                current,
                cursor,
                canvas,
                col_w,
                lane_h,
                depth_idx + 1,
                lane,
            )
        })
        .collect();

    let first = *centers.first().unwrap();
    let last = *centers.last().unwrap();
    let cy = (first + last) / 2;
    let top = cy.saturating_sub(lane_h / 2);
    draw_card(frame, app, s, node, current, cursor, x, top, card_w, lane_h);

    let child_x = canvas.x + (depth_idx + 1) * col_w;
    draw_connectors(frame, x + card_w, child_x, cy, &centers);
    cy
}

/// Draws one species card: its sprite (or a placeholder while loading) with the
/// name centred beneath it. The navigation cursor gets a highlighted name bar;
/// the currently displayed species is tinted but not boxed.
#[allow(clippy::too_many_arguments)]
fn draw_card(
    frame: &mut Frame,
    app: &App,
    s: &Strings,
    node: &EvolutionTree,
    current: Option<&str>,
    cursor: Option<&str>,
    x: u16,
    top: u16,
    w: u16,
    h: u16,
) {
    if w == 0 || h == 0 {
        return;
    }

    // How this stage is reached. A card one row taller than the minimum gets a
    // dedicated row for it; a shorter one tucks it in beside the name instead,
    // so the requirement survives even on a cramped three-way branch.
    let condition = node.condition.as_ref().and_then(|c| s.evo.short(c));
    let stacked = condition.is_some() && h > MIN_CARD_H;
    let text_rows = if stacked { 2 } else { 1 };

    let sprite_area = Rect {
        x,
        y: top,
        width: w,
        height: h.saturating_sub(text_rows),
    };
    match app.sprite_for(&node.name) {
        Some(sprite) => render_sprite_capped(frame, sprite_area, sprite, w),
        None => {
            let placeholder = if app.sprite_is_loading(&node.name) {
                s.sprite_loading
            } else {
                "…"
            };
            render_centered_text(frame, sprite_area, placeholder, theme::OVERLAY);
        }
    }

    let is_cursor = cursor == Some(node.name.as_str());
    let is_current = current == Some(node.name.as_str());
    let style = if is_cursor {
        color::highlight(theme::YELLOW).add_modifier(Modifier::BOLD)
    } else if is_current {
        Style::default()
            .fg(theme::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::GREEN)
    };
    let label = title_case(&node.name);
    let mut name_spans = vec![Span::styled(label.clone(), style)];

    // Inline requirement: only when there is no row of its own for it, and only
    // if enough columns are left over to say something meaningful.
    if let (Some(text), false) = (&condition, stacked) {
        let free = (w as usize).saturating_sub(label.chars().count());
        if free >= 6 {
            name_spans.push(Span::styled(
                truncate(&format!(" · {text}"), free),
                Style::default().fg(theme::PEACH),
            ));
        }
    }

    let name_y = top + h.saturating_sub(text_rows);
    let name = Paragraph::new(Line::from(name_spans)).alignment(Alignment::Center);
    frame.render_widget(
        name,
        Rect {
            x,
            y: name_y,
            width: w,
            height: 1,
        },
    );

    if let (Some(text), true) = (&condition, stacked) {
        let requirement = Paragraph::new(Line::from(Span::styled(
            truncate(text, w as usize),
            Style::default().fg(theme::PEACH),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(
            requirement,
            Rect {
                x,
                y: name_y + 1,
                width: w,
                height: 1,
            },
        );
    }
}

/// Wires a parent card's right edge to each child card's left edge with
/// box-drawing connectors and an arrowhead, branching where needed.
fn draw_connectors(frame: &mut Frame, x_from: u16, x_to: u16, parent_cy: u16, centers: &[u16]) {
    let color = theme::OVERLAY;
    if x_to <= x_from {
        return;
    }

    // Single child: a straight arrow reads cleaner than a trunk-and-branch.
    if centers.len() == 1 {
        let cy = centers[0];
        for x in x_from..x_to.saturating_sub(1) {
            put_cell(frame, x, cy, "─", color);
        }
        put_cell(frame, x_to.saturating_sub(1), cy, "▶", theme::MAUVE);
        return;
    }

    let trunk_x = x_from + (x_to - x_from) / 2;
    let min_c = *centers.iter().min().unwrap();
    let max_c = *centers.iter().max().unwrap();

    // Stub from the parent into the vertical trunk.
    for x in x_from..trunk_x {
        put_cell(frame, x, parent_cy, "─", color);
    }
    // The vertical trunk spanning all the children.
    for y in min_c..=max_c {
        put_cell(frame, trunk_x, y, "│", color);
    }
    // Junction where the parent's stub meets the trunk.
    let junction = if centers.contains(&parent_cy) {
        "┼"
    } else {
        "┤"
    };
    put_cell(frame, trunk_x, parent_cy, junction, color);

    // Branch off to each child and tip it with an arrowhead.
    for &cy in centers {
        let corner = if cy == min_c {
            "┌"
        } else if cy == max_c {
            "└"
        } else {
            "├"
        };
        if cy != parent_cy {
            put_cell(frame, trunk_x, cy, corner, color);
        }
        for x in (trunk_x + 1)..x_to.saturating_sub(1) {
            put_cell(frame, x, cy, "─", color);
        }
        put_cell(frame, x_to.saturating_sub(1), cy, "▶", theme::MAUVE);
    }
}

/// Writes a single glyph straight into the frame buffer (used for the connector
/// art, which doesn't map cleanly onto a widget).
fn put_cell(frame: &mut Frame, x: u16, y: u16, symbol: &str, color: Color) {
    let area = frame.area();
    if x < area.x || y < area.y || x >= area.right() || y >= area.bottom() {
        return;
    }
    if let Some(cell) = frame.buffer_mut().cell_mut(Position::new(x, y)) {
        cell.set_symbol(symbol).set_fg(color);
    }
}

// --- Type matchup card ----------------------------------------------------

/// Preferred width of the matchup card, clamped to the terminal.
const MATCHUP_CARD_W: u16 = 48;
/// The team card carries names *and* chips, so it needs a little more room.
const TEAM_CARD_W: u16 = 56;
/// The ability card holds wrapped prose, so it is wider still.
const ABILITY_CARD_W: u16 = 60;
/// The moves card is the widest of them: seven columns, and a description
/// underneath that wants the same room the ability card's prose does.
const MOVES_CARD_W: u16 = 66;
/// Columns each of the moves card's numeric fields is padded to.
const MOVE_NUM_W: usize = 5;
/// The comparison card holds two of everything side by side, so it is the
/// widest of the lot.
const COMPARE_CARD_W: u16 = 72;
/// Columns each side's number gets on a comparison row. Four rather than the
/// three a base stat needs, so the totals line — which can run past a thousand
/// — reads down the same columns as the rows above it.
const COMPARE_VAL_W: usize = 4;
/// Columns the margin gets at the end of a comparison row: an arrow pointing at
/// the winner, a space, and up to three digits.
const COMPARE_MARGIN_W: usize = 6;
/// Columns reserved for a multiplier label (`" ×4  "`), which also sets the
/// indent used when a group of chips wraps onto another row.
const MATCHUP_LABEL_W: usize = 5;

/// Draws the modal card summarising the selected Pokemon's type matchups: what
/// hits it hard, what it shrugs off, and what its own attacks are strong
/// against. Everything here is computed offline from [`typechart`].
fn render_matchups(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let Some(detail) = app.selected_detail() else {
        return; // nothing loaded to analyse
    };

    let width = MATCHUP_CARD_W.min(full.width);
    let text_w = width.saturating_sub(2) as usize; // usable columns inside the border
    if text_w < 16 || full.height < 8 {
        return; // too cramped to be readable; leave the main view alone
    }

    let mut lines: Vec<Line> = Vec::new();

    // Headline: who this card is about, and the types the analysis is based on.
    let mut head = vec![Span::styled(
        format!(" {}  ", title_case(&detail.name)),
        Style::default()
            .fg(theme::MAUVE)
            .add_modifier(Modifier::BOLD),
    )];
    head.extend(type_chips(&detail.types));
    lines.push(Line::from(head));
    lines.push(Line::raw(""));

    // Defensive view: incoming damage, worst multiplier first. Neutral matchups
    // are omitted by `defensive_groups`, so every row here is worth reading.
    //
    // Abilities are read first because a certain one rewrites the rows: a
    // species that cannot *not* have Levitate is simply not hit by Ground, and
    // the chart on its own would say otherwise. One it merely might have is
    // left out of the numbers and annotated below instead.
    let immunities = team::ability_immunities(detail);
    let certain: Vec<&str> = immunities
        .iter()
        .filter(|immunity| immunity.certain)
        .map(|immunity| immunity.immune_to)
        .collect();

    lines.push(section_heading(s.matchups_defense));
    for group in typechart::defensive_groups(&detail.types, &certain) {
        lines.extend(chip_rows(group.label, &group.types, text_w));
    }

    // Directly under the numbers, because it is the numbers this explains:
    // why a row moved, or what would move if the species turned out to carry
    // the other ability.
    if !immunities.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section_heading(s.immune_by_ability));
        for immunity in &immunities {
            lines.push(ability_immunity_row(app, s, immunity, "  "));
        }
    }

    // Offensive view: what its own same-type moves are strong against.
    lines.push(Line::raw(""));
    lines.push(section_heading(s.matchups_offense));
    let coverage = typechart::offensive_coverage(&detail.types);
    if coverage.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", s.matchups_none),
            Style::default().fg(theme::OVERLAY),
        )));
    } else {
        lines.extend(chip_rows("", &coverage, text_w));
    }

    // Two border rows plus the hint row at the foot.
    let height = (lines.len() as u16 + 3).min(full.height);
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.matchups_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines), rows[0]);

    let hint = Paragraph::new(Line::from(Span::styled(
        s.close_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[1]);
}

/// Renders a list of types as coloured chips, separated by a space.
fn type_chips(types: &[String]) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(types.len() * 2);
    for ty in types {
        spans.push(Span::styled(
            format!(" {} ", title_case(ty)),
            Style::default().fg(theme::BASE).bg(theme::type_color(ty)),
        ));
        spans.push(Span::raw(" "));
    }
    spans
}

fn section_heading(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {text}"),
        Style::default()
            .fg(theme::PEACH)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Lays `types` out as chips in a labelled row, wrapping onto further rows when
/// they overflow `max_width`. Continuation rows are indented under the chips so
/// the label column stays clean.
fn chip_rows(label: &str, types: &[&str], max_width: usize) -> Vec<Line<'static>> {
    let indent = " ".repeat(MATCHUP_LABEL_W);
    let mut rows: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = vec![Span::styled(
        format!(" {label:<pad$} ", pad = MATCHUP_LABEL_W - 2),
        Style::default()
            .fg(theme::SUBTEXT)
            .add_modifier(Modifier::BOLD),
    )];
    let mut used = MATCHUP_LABEL_W;

    for ty in types {
        let chip = format!(" {} ", title_case(ty));
        let chip_w = chip.chars().count() + 1; // chip plus its trailing space
        if used + chip_w > max_width && used > MATCHUP_LABEL_W {
            rows.push(Line::from(std::mem::take(&mut spans)));
            spans.push(Span::raw(indent.clone()));
            used = MATCHUP_LABEL_W;
        }
        spans.push(Span::styled(
            chip,
            Style::default().fg(theme::BASE).bg(theme::type_color(ty)),
        ));
        spans.push(Span::raw(" "));
        used += chip_w;
    }

    rows.push(Line::from(spans));
    rows
}

// --- Language picker ------------------------------------------------------

/// Draws the little modal card for switching interface language.
/// The party card: who is on the team, and the three things their combined
/// typings say about it.
/// The ability card: each of the species' abilities with what it actually does.
/// The overlay is two columns wide so the whole key map fits without scrolling
/// on a standard 24-row terminal.
const HELP_CARD_W: u16 = 86;

/// The help overlay: every binding in one place, grouped by where it applies.
///
/// The key names are language-neutral and live here; only the action labels
/// come from the translation table.
fn render_help(frame: &mut Frame, s: &Strings, full: Rect) {
    let h = &s.help_card;

    let left: Vec<(&str, &str)> = vec![
        ("", h.ctx_list),
        ("↑ ↓ · j k", h.act_move),
        ("PgUp PgDn", h.act_jump10),
        ("Enter", h.act_load),
        ("/ · Tab", h.act_search),
        ("E", h.act_evolutions),
        ("F", h.act_chain_expand),
        ("T", h.act_types),
        ("C", h.act_compare),
        ("A", h.act_abilities),
        ("M", h.act_moves),
        ("X", h.act_shiny),
        ("Space", h.act_party_toggle),
        ("P", h.act_party_card),
        ("S", h.act_sort),
        ("L", h.act_language),
        ("?", h.act_help),
        ("Q", h.act_quit),
    ];
    let right: Vec<(&str, &str)> = vec![
        ("", h.ctx_search),
        ("Enter", h.act_load_back),
        ("Esc · Tab", h.act_back),
        ("type:water", h.act_by_type),
        ("ability:levitate", h.act_by_ability),
        ("egg:dragon", h.act_by_egg),
        ("gen:1", h.act_by_generation),
        ("", ""),
        ("", h.ctx_evolution),
        ("← → ↑ ↓ · h j k l", h.act_chain_move),
        ("Enter", h.act_chain_jump),
        ("F", h.act_chain_expand),
        ("X", h.act_shiny),
        ("Esc · Tab", h.act_back),
        ("", ""),
        ("", h.ctx_cards),
        ("Esc", h.act_close),
        ("Ctrl-C", h.act_quit),
    ];

    let rows = left.len().max(right.len()) as u16;
    let width = HELP_CARD_W.min(full.width);
    let height = (rows + 4).min(full.height);
    if width < 40 || height < 8 {
        return; // too cramped to be readable; leave the main view alone
    }

    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            h.title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let body = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(body[0]);
    frame.render_widget(Paragraph::new(help_lines(&left)), cols[0]);
    frame.render_widget(Paragraph::new(help_lines(&right)), cols[1]);

    let hint = Paragraph::new(Line::from(Span::styled(
        h.close_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, body[1]);
}

/// Turns help rows into lines. A row with no keys is a section heading, and an
/// entirely empty one is a spacer.
///
/// The key column is sized from the column's own widest entry, so the side
/// carrying `← → ↑ ↓ · h j k l` does not force that much padding on the other
/// and squeeze its labels into truncation.
fn help_lines(rows: &[(&str, &str)]) -> Vec<Line<'static>> {
    let key_w = rows
        .iter()
        .map(|(keys, _)| keys.chars().count())
        .max()
        .unwrap_or(0)
        + 2;

    rows.iter()
        .map(|(keys, action)| {
            if keys.is_empty() {
                return match action.is_empty() {
                    true => Line::raw(""),
                    false => section_heading(action),
                };
            }
            Line::from(vec![
                Span::styled(
                    format!("  {keys:<key_w$}"),
                    Style::default()
                        .fg(theme::TEAL)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled((*action).to_string(), Style::default().fg(theme::SUBTEXT)),
            ])
        })
        .collect()
}

/// Draws the modal card listing the selected Pokemon's learnset: what it learns,
/// how, and — for whichever row the cursor is on — what the move actually does.
///
/// The rows come free with the species record. The per-move numbers do not, so
/// a row shows what it has and fills in the rest once
/// [`App::ensure_move_info`] has fetched it; scrolling past a row without
/// stopping costs one request that the next visit reads from the cache.
fn render_moves(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let Some(detail) = app.selected_detail() else {
        return;
    };
    let learnset = detail.moves.as_slice();

    let width = MOVES_CARD_W.min(full.width);
    // Prose is inset from the border on both sides; the table uses the full
    // inner width, since its own leading space is part of the format.
    let table_w = width.saturating_sub(2) as usize;
    let text_w = width.saturating_sub(4) as usize;
    if text_w < 40 || full.height < 12 {
        return; // too cramped for seven columns; leave the main view alone
    }

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.moves_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));

    // The card claims most of the height available, leaving a margin so the
    // list behind it stays visible — this is a card, not a second screen.
    let height = full
        .height
        .saturating_sub(4)
        .min(learnset.len() as u16 + 10);
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if learnset.is_empty() {
        render_centered_text(frame, inner, s.moves_empty, theme::OVERLAY);
        return;
    }

    // Header, the scrolling list, the highlighted move's description, and the
    // hint — in that order, with the list taking whatever is left over.
    let rows = Layout::vertical([
        Constraint::Length(2), // species + games, then column headings
        Constraint::Min(1),    // the learnset
        Constraint::Length(3), // what the highlighted move does
        Constraint::Length(1), // close hint
    ])
    .split(inner);

    let mut heading = vec![Span::styled(
        format!(" {}", title_case(&detail.name)),
        Style::default()
            .fg(theme::MAUVE)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(games) = &detail.learnset_games {
        heading.push(Span::styled(
            format!("  ·  {}", title_case(games)),
            Style::default().fg(theme::OVERLAY),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![Line::from(heading), {
            let (left, middle, right) = move_columns(
                s.col_learn,
                s.col_move,
                s.col_type,
                s.col_category,
                s.col_power,
                s.col_accuracy,
                s.col_pp,
                table_w,
            );
            Line::from(Span::styled(
                format!("{left}{middle}{right}"),
                Style::default().fg(theme::OVERLAY),
            ))
        }]),
        rows[0],
    );

    // Centre the cursor in the window where there is room on both sides, and
    // pin it to an end where there is not, so the last rows stay reachable.
    let window = rows[1].height as usize;
    let first = app
        .move_cursor
        .saturating_sub(window / 2)
        .min(learnset.len().saturating_sub(window));
    let lines: Vec<Line> = learnset
        .iter()
        .enumerate()
        .skip(first)
        .take(window)
        .map(|(idx, learned)| move_row(app, s, learned, idx == app.move_cursor, table_w))
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[1]);

    frame.render_widget(Paragraph::new(move_description(app, s, text_w)), rows[2]);

    let hint = Paragraph::new(Line::from(Span::styled(
        s.moves_close_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[3]);
}

/// Lays the seven columns out on one row, split either side of the type so the
/// caller can colour that column on its own. Spelled once so the headings and
/// the rows under them cannot drift apart.
#[allow(clippy::too_many_arguments)]
fn move_columns(
    learn: &str,
    name: &str,
    type_name: &str,
    category: &str,
    power: &str,
    accuracy: &str,
    pp: &str,
    width: usize,
) -> (String, String, String) {
    // Everything but the name is fixed-width — the leading space, the level
    // column and its separator, the type column and its separators, and the
    // four numeric columns — so the name absorbs whatever is left over.
    let name_w = width.saturating_sub(12 + 4 * MOVE_NUM_W + 8).max(8);
    (
        format!(" {learn:>7} {name:<name_w$} "),
        format!("{type_name:<9}"),
        format!(
            " {category:<MOVE_NUM_W$}{power:>MOVE_NUM_W$}{accuracy:>MOVE_NUM_W$}{pp:>MOVE_NUM_W$}"
        ),
    )
}

/// One row of the learnset. The type is the only part that carries colour: it
/// is what a reader scans the list for.
fn move_row<'a>(
    app: &'a App,
    s: &Strings,
    learned: &'a LearnedMove,
    highlighted: bool,
    width: usize,
) -> Line<'a> {
    let code = app.language.flavor_code();
    let info = app.moves.get(&learned.name);

    // Levels go in bare under the level heading, the way the games print them.
    // Level zero is how a move known from the start is recorded, and no game
    // ever calls that level zero.
    let learn = match learned.method {
        LearnMethod::LevelUp if learned.level == 0 => "—".to_string(),
        LearnMethod::LevelUp => learned.level.to_string(),
        LearnMethod::Machine => s.learn_machine.to_string(),
        LearnMethod::Egg => s.learn_egg.to_string(),
        LearnMethod::Tutor => s.learn_tutor.to_string(),
    };
    let name = match info {
        Some(info) => info.name_for(code),
        None => title_case(&learned.name),
    };
    // A row whose record has not landed shows the two fields the species
    // record already answered for, and blanks rather than zeros for the rest.
    let (type_name, category, power, accuracy, pp) = match info {
        Some(info) => (
            info.type_name.to_uppercase(),
            damage_class_label(s, &info.damage_class).to_string(),
            info.power
                .map_or_else(|| "—".to_string(), |p| p.to_string()),
            info.accuracy
                .map_or_else(|| "—".to_string(), |a| a.to_string()),
            info.pp.map_or_else(|| "—".to_string(), |p| p.to_string()),
        ),
        None => (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ),
    };

    let (left, middle, right) = move_columns(
        &learn, &name, &type_name, &category, &power, &accuracy, &pp, width,
    );

    // The highlighted row is painted in one piece: the selection bar is what
    // says where the cursor is, and a type colour showing through it would only
    // muddy that.
    if highlighted {
        let style = color::highlight(theme::MAUVE).add_modifier(Modifier::BOLD);
        return Line::from(Span::styled(format!("{left}{middle}{right}"), style));
    }

    let plain = Style::default().fg(theme::TEXT);
    Line::from(vec![
        Span::styled(left, plain),
        Span::styled(
            middle,
            Style::default().fg(theme::type_color(&learned_type(app, learned))),
        ),
        Span::styled(right, Style::default().fg(theme::SUBTEXT)),
    ])
}

/// The type slug of a move whose record has landed, or the empty string —
/// which no type answers to, so the column simply draws unstyled.
fn learned_type(app: &App, learned: &LearnedMove) -> String {
    app.moves
        .get(&learned.name)
        .map(|info| info.type_name.clone())
        .unwrap_or_default()
}

/// What the highlighted move does, wrapped to the card. Absent until its record
/// lands, where the loading placeholder stands in — the same shape the ability
/// card uses for the same reason.
fn move_description<'a>(app: &App, s: &Strings, width: usize) -> Vec<Line<'a>> {
    let code = app.language.flavor_code();
    let text = app
        .highlighted_move()
        .and_then(|learned| app.moves.get(&learned.name))
        .and_then(|info| info.flavor_for(code));

    match text {
        Some(text) => wrap_plain(text, width)
            .into_iter()
            .take(3)
            .map(|row| {
                Line::from(Span::styled(
                    format!(" {row}"),
                    Style::default().fg(theme::SUBTEXT),
                ))
            })
            .collect(),
        None => vec![Line::from(Span::styled(
            format!(" {}…", s.loading),
            Style::default().fg(theme::OVERLAY),
        ))],
    }
}

/// Localized label for a move's damage category. An unrecognised class shows
/// its API slug rather than being dropped, which is how a new one would
/// announce itself.
fn damage_class_label<'a>(s: &Strings, class: &'a str) -> &'a str
where
    'static: 'a,
{
    match class {
        "physical" => s.class_physical,
        "special" => s.class_special,
        "status" => s.class_status,
        other => other,
    }
}

fn render_abilities(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let Some(detail) = app.selected_detail() else {
        return;
    };

    let width = ABILITY_CARD_W.min(full.width);
    let text_w = width.saturating_sub(4) as usize;
    if text_w < 16 || full.height < 8 {
        return; // too cramped to be readable; leave the main view alone
    }

    let mut lines: Vec<Line> = Vec::new();
    let code = app.language.flavor_code();

    lines.push(Line::from(Span::styled(
        format!(" {}", title_case(&detail.name)),
        Style::default()
            .fg(theme::MAUVE)
            .add_modifier(Modifier::BOLD),
    )));

    for ability in &detail.abilities {
        lines.push(Line::raw(""));

        let mut head = vec![Span::styled(
            format!(" {}", ability_display_name(app, &ability.name)),
            Style::default()
                .fg(theme::PEACH)
                .add_modifier(Modifier::BOLD),
        )];
        if ability.is_hidden {
            head.push(Span::styled(
                format!("  ({})", s.ability_hidden),
                Style::default().fg(theme::OVERLAY),
            ));
        }
        lines.push(Line::from(head));

        // Until the text lands — or if it never does — the name above is still
        // the useful half, so the row degrades to a quiet placeholder.
        match app
            .abilities
            .get(&ability.name)
            .and_then(|info| info.flavor_for(code))
        {
            Some(text) => {
                for row in wrap_plain(text, text_w) {
                    lines.push(Line::from(Span::styled(
                        format!("  {row}"),
                        Style::default().fg(theme::SUBTEXT),
                    )));
                }
            }
            None => lines.push(Line::from(Span::styled(
                format!("  {}…", s.loading),
                Style::default().fg(theme::OVERLAY),
            ))),
        }
    }

    let height = (lines.len() as u16 + 3).min(full.height);
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.abilities_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines), rows[0]);

    let hint = Paragraph::new(Line::from(Span::styled(
        s.ability_close_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[1]);
}

/// An ability's name in the active language, falling back to its slug until
/// the localized text has been fetched. Callers add the hidden marker
/// themselves, since the two cards place it differently.
/// One `ability → type` row, drawn identically wherever an immunity is
/// reported. `lead` is whatever precedes the ability name: the party card
/// names the member it belongs to, the single-species card is already about
/// one Pokemon and has nothing to disambiguate.
///
/// An immunity the species might not have is marked rather than asserted. A
/// species carries one of its listed abilities, not all of them, and a card
/// that quietly dropped that distinction would promise a certainty the data
/// does not support.
fn ability_immunity_row(
    app: &App,
    s: &Strings,
    immunity: &AbilityImmunity,
    lead: &str,
) -> Line<'static> {
    let mut row = vec![
        Span::styled(lead.to_string(), Style::default().fg(theme::TEXT)),
        Span::styled(
            ability_display_name(app, &immunity.ability),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(" → ", Style::default().fg(theme::OVERLAY)),
        Span::styled(
            format!(" {} ", title_case(immunity.immune_to)),
            Style::default()
                .fg(theme::BASE)
                .bg(theme::type_color(immunity.immune_to)),
        ),
    ];
    if !immunity.certain {
        row.push(Span::styled(
            format!("  ({})", s.immunity_maybe),
            Style::default().fg(theme::OVERLAY),
        ));
    }
    Line::from(row)
}

fn ability_display_name(app: &App, slug: &str) -> String {
    match app.abilities.get(slug) {
        Some(info) => info.name_for(app.language.flavor_code()),
        None => title_case(slug),
    }
}

/// Greedy word wrap for a plain paragraph of text.
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            rows.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

fn render_team(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let width = TEAM_CARD_W.min(full.width);
    let text_w = width.saturating_sub(2) as usize;
    if text_w < 16 || full.height < 8 {
        return; // too cramped to be readable; leave the main view alone
    }

    let loaded = app.team_details();
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        format!(" {}/{}", app.team.len(), team::MAX_MEMBERS),
        Style::default()
            .fg(theme::MAUVE)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));

    if app.team.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {}", s.team_empty),
            Style::default().fg(theme::OVERLAY),
        )));
    }

    // Roster. A member whose record has not arrived yet is listed by name so
    // the party still reads as complete, but greyed out — the analysis below
    // genuinely does not account for it yet.
    for name in &app.team {
        let mut row = vec![Span::styled(
            format!("  {:<12} ", title_case(name)),
            Style::default().fg(theme::TEXT),
        )];
        match app.details.get(name) {
            Some(detail) => row.extend(type_chips(&detail.types)),
            None => row.push(Span::styled(
                s.loading.to_string(),
                Style::default().fg(theme::OVERLAY),
            )),
        }
        lines.push(Line::from(row));
    }

    if !loaded.is_empty() {
        let analysis = team::analyse(&loaded);

        // Shared weaknesses, grouped by how many members each type hits. The
        // `n/total` label counts members, not damage — an important distinction
        // next to the single-species card, where the label is a multiplier.
        lines.push(Line::raw(""));
        lines.push(section_heading(s.team_shared_weak));
        if analysis.shared_weaknesses.is_empty() {
            lines.push(all_clear(s));
        } else {
            let mut remaining = analysis.shared_weaknesses.as_slice();
            while let Some(first) = remaining.first() {
                let count = first.weak;
                let split = remaining.partition_point(|row| row.weak == count);
                let (group, rest) = remaining.split_at(split);
                let types: Vec<&str> = group.iter().map(|row| row.attacker).collect();
                let label = format!("{count}/{}", loaded.len());
                lines.extend(chip_rows(&label, &types, text_w));
                remaining = rest;
            }
        }

        lines.push(Line::raw(""));
        lines.push(section_heading(s.team_unresisted));
        push_chip_section(&mut lines, &analysis.unresisted, text_w, s);

        // Placed directly under "resisted by nobody", because that is exactly
        // the claim it qualifies: the chart cannot see these, so an unresisted
        // type may still have an answer sitting right here.
        if !analysis.ability_immunities.is_empty() {
            lines.push(Line::raw(""));
            lines.push(section_heading(s.immune_by_ability));
            for immunity in &analysis.ability_immunities {
                // Led by the member's name: this card lists six Pokemon, so an
                // unattributed row would not say whose immunity it is.
                let lead = format!("  {} · ", title_case(&immunity.pokemon));
                lines.push(ability_immunity_row(app, s, immunity, &lead));
            }
        }

        lines.push(Line::raw(""));
        lines.push(section_heading(s.team_offense_gaps));
        push_chip_section(&mut lines, &analysis.offense_gaps, text_w, s);
    }

    let height = (lines.len() as u16 + 3).min(full.height);
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.team_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    frame.render_widget(Paragraph::new(lines), rows[0]);

    let hint = Paragraph::new(Line::from(Span::styled(
        s.team_close_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[1]);
}

/// Renders one chip section, or the "nothing to report" line when it is empty.
/// On this card an empty section is good news, so it reads as reassurance
/// rather than as missing data.
fn push_chip_section(lines: &mut Vec<Line<'static>>, types: &[&str], width: usize, s: &Strings) {
    if types.is_empty() {
        lines.push(all_clear(s));
    } else {
        lines.extend(chip_rows("", types, width));
    }
}

fn all_clear(s: &Strings) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}", s.team_all_clear),
        Style::default().fg(theme::GREEN),
    ))
}

/// Draws the head-to-head card: the pinned species against the one on display,
/// stat by stat.
///
/// The arithmetic is [`compare`]'s; what this adds is the reading order. Every
/// row is a pair of bars growing outwards from the labels, so the answer to
/// "which of these two is bulkier" arrives before any of the numbers are read,
/// and the margin at the end of the row says by how much for the ones that are.
fn render_compare(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let Some((left, right)) = app.comparison() else {
        return; // nothing pinned, or nothing on display to pin it against
    };

    let width = COMPARE_CARD_W.min(full.width);
    // Two borders, and a column of breathing room inside each of them: the
    // header, the chips and the abilities all sit against the frame otherwise.
    let inner_w = width.saturating_sub(4) as usize;
    // Two bars, two values, a label and the margin. Below this there is no
    // room left for bars, and a card of bare numbers is what the reader could
    // already have got by flipping between the two species by hand.
    let bar_w =
        inner_w.saturating_sub(COMPARE_VAL_W * 2 + STAT_LABEL_WIDTH + COMPARE_MARGIN_W + 5) / 2;
    if bar_w < 6 || full.height < 18 {
        return; // too cramped to be readable; leave the main view alone
    }

    let rows = compare::stat_rows(left, right);
    let peak = compare::peak(&rows);

    let height = (rows.len() as u16 + 15).min(full.height);
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.compare_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };

    let body = Layout::vertical([
        Constraint::Length(2),                     // names, then type chips
        Constraint::Length(1),                     // spacer
        Constraint::Length(rows.len() as u16 + 2), // stats, spacer, totals
        Constraint::Length(1),                     // spacer
        Constraint::Length(1),                     // best-hit heading
        Constraint::Length(1),                     // best-hit row
        Constraint::Length(1),                     // spacer
        Constraint::Min(0),                        // measurements and abilities
        Constraint::Length(1),                     // close hint
    ])
    .split(inner);

    // Each side keeps to its own half throughout, and the right one is mirrored
    // — right-aligned against the edge it grows from — so the two read as two
    // columns rather than as one list of pairs.
    let head =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(body[0]);
    frame.render_widget(Paragraph::new(side_heading(left)), head[0]);
    frame.render_widget(
        Paragraph::new(side_heading(right)).alignment(Alignment::Right),
        head[1],
    );

    let mut stat_lines: Vec<Line> = rows
        .iter()
        .map(|row| {
            compare_row(
                app.language.stat_label(row.kind),
                row.left as u32,
                row.right as u32,
                Some((row.left, row.right, peak)),
                bar_w,
                s,
            )
        })
        .collect();
    stat_lines.push(Line::raw(""));
    // The totals are on a scale of their own — a species' six stats sum to
    // several hundred — so the row carries the numbers without bars rather than
    // drawing them against a ruler the rows above do not share.
    stat_lines.push(compare_row(
        s.total_label,
        left.stat_total(),
        right.stat_total(),
        None,
        bar_w,
        s,
    ));
    frame.render_widget(Paragraph::new(stat_lines), body[2]);

    frame.render_widget(Paragraph::new(section_heading(s.compare_best_hit)), body[4]);
    let hits =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(body[5]);
    frame.render_widget(Paragraph::new(best_hit_line(left, right)), hits[0]);
    frame.render_widget(
        Paragraph::new(best_hit_line(right, left)).alignment(Alignment::Right),
        hits[1],
    );

    let facts =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(body[7]);
    let fact_w = facts[0].width as usize;
    frame.render_widget(Paragraph::new(side_facts(app, left, fact_w)), facts[0]);
    frame.render_widget(
        Paragraph::new(side_facts(app, right, fact_w)).alignment(Alignment::Right),
        facts[1],
    );

    let hint = Paragraph::new(Line::from(Span::styled(
        s.compare_hint,
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, body[8]);
}

/// One side's name, dex number and typing, for the top of the comparison card.
fn side_heading(species: &PokemonDetail) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled(
                title_case(&species.name),
                Style::default()
                    .fg(theme::MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  #{:04}", species.dex_number),
                Style::default().fg(theme::OVERLAY),
            ),
        ]),
        Line::from(type_chips(&species.types)),
    ]
}

/// One comparison row: a pair of bars growing outwards from the label, the two
/// numbers beside it, and the margin at the end.
///
/// `bars` carries the values to draw and the number they scale against; the
/// totals line passes `None`, which lays the numbers out on the same columns
/// with the bar space left blank.
fn compare_row(
    label: &str,
    left: u32,
    right: u32,
    bars: Option<(u16, u16, u16)>,
    bar_w: usize,
    s: &Strings,
) -> Line<'static> {
    let winner = compare::side(left, right);
    let (left_color, right_color) = match winner {
        compare::Side::Left => (theme::GREEN, theme::OVERLAY),
        compare::Side::Right => (theme::OVERLAY, theme::GREEN),
        compare::Side::Tie => (theme::LAVENDER, theme::LAVENDER),
    };
    let emphasis = |side| match winner == side {
        true => Modifier::BOLD,
        false => Modifier::empty(),
    };

    let (left_fill, right_fill) = match bars {
        Some((l, r, peak)) => (fill(l, peak, bar_w), fill(r, peak, bar_w)),
        None => (0, 0),
    };

    Line::from(vec![
        Span::raw(" ".repeat(bar_w - left_fill)),
        Span::styled("█".repeat(left_fill), Style::default().fg(left_color)),
        Span::styled(
            format!(" {left:>COMPARE_VAL_W$} "),
            Style::default()
                .fg(left_color)
                .add_modifier(emphasis(compare::Side::Left)),
        ),
        Span::styled(
            format!("{label:^STAT_LABEL_WIDTH$}"),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(
            format!(" {right:<COMPARE_VAL_W$} "),
            Style::default()
                .fg(right_color)
                .add_modifier(emphasis(compare::Side::Right)),
        ),
        Span::styled("█".repeat(right_fill), Style::default().fg(right_color)),
        Span::raw(" ".repeat(bar_w - right_fill)),
        Span::styled(
            format!(" {:<COMPARE_MARGIN_W$}", margin_label(left, right, s)),
            Style::default().fg(match winner {
                compare::Side::Tie => theme::OVERLAY,
                _ => theme::GREEN,
            }),
        ),
    ])
}

/// How many cells of a `bar_w` bar a value fills, against the biggest number on
/// the card. A value that is not quite zero still gets a cell, so a row reads
/// as a very short bar rather than as a missing one.
fn fill(value: u16, peak: u16, bar_w: usize) -> usize {
    if value == 0 || peak == 0 {
        return 0;
    }
    ((value as usize * bar_w) / peak as usize).clamp(1, bar_w)
}

/// The end of a comparison row: an arrow pointing at the side that wins it and
/// by how much, or the word for a row they are level on.
fn margin_label(left: u32, right: u32, s: &Strings) -> String {
    match compare::side(left, right) {
        compare::Side::Left => format!("◀ {}", left - right),
        compare::Side::Right => format!("▶ {}", right - left),
        compare::Side::Tie => s.compare_tie.to_string(),
    }
}

/// The hardest same-type hit `attacker` has on `defender`, as a chip and a
/// multiplier.
fn best_hit_line(attacker: &PokemonDetail, defender: &PokemonDetail) -> Line<'static> {
    let Some(hit) = compare::best_hit(attacker, defender) else {
        return Line::raw("");
    };
    let label = typechart::multiplier_label(hit.multiplier);
    Line::from(vec![
        Span::styled(
            format!(" {} ", title_case(hit.attack_type)),
            Style::default()
                .fg(theme::BASE)
                .bg(theme::type_color(hit.attack_type))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {label}"),
            Style::default()
                .fg(match hit.multiplier > 1.0 {
                    true => theme::PEACH,
                    false => theme::SUBTEXT,
                })
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// One side's measurements and abilities, for the foot of the comparison card.
fn side_facts(app: &App, species: &PokemonDetail, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:.1} m · {:.1} kg",
            species.height as f32 / 10.0,
            species.weight as f32 / 10.0
        ),
        Style::default().fg(theme::SUBTEXT),
    ))];

    let abilities: Vec<String> = species
        .abilities
        .iter()
        .map(|ability| ability_display_name(app, &ability.name))
        .collect();
    if !abilities.is_empty() {
        // Two lines at most: a third would push the hint off a card sized for
        // the pair, and the ability card behind `A` has the full list anyway.
        lines.extend(
            wrap_plain(&abilities.join(" · "), width.max(8))
                .into_iter()
                .take(2)
                .map(|text| Line::from(Span::styled(text, Style::default().fg(theme::TEXT)))),
        );
    }
    lines
}

fn render_language_picker(frame: &mut Frame, app: &App, s: &Strings, full: Rect) {
    let width = 26u16;
    let height = Language::ALL.len() as u16 + 4; // borders + title pad + hint
    let area = centered_fixed(width, height, full);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::MAUVE))
        .title(Span::styled(
            s.language_title,
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);

    let mut lines: Vec<Line> = Vec::with_capacity(Language::ALL.len());
    for (i, lang) in Language::ALL.iter().enumerate() {
        let selected = i == app.lang_cursor;
        let active = *lang == app.language;
        let marker = if active { "●" } else { "○" };
        let label = format!(" {marker} {:<10} {} ", lang.label(), lang.tag());
        let style = if selected {
            color::highlight(theme::MAUVE).add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(theme::MAUVE)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(Span::styled(label, style)));
    }
    frame.render_widget(Paragraph::new(lines), rows[0]);

    let hint = Paragraph::new(Line::from(Span::styled(
        "↑/↓ · Enter · Esc",
        Style::default().fg(theme::OVERLAY),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[1]);
}

/// A fixed-size `Rect` centred within `area` (clamped to fit).
fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain shaped like `children`: one root, then a leaf per entry, nested
    /// `depth` deep along the first branch.
    fn chain(depth: usize, leaves: usize) -> EvolutionTree {
        let mut node = EvolutionTree {
            name: "leaf".to_string(),
            condition: None,
            children: Vec::new(),
        };
        for _ in 1..depth {
            node = EvolutionTree {
                name: "stage".to_string(),
                condition: None,
                children: vec![node],
            };
        }
        // Widen the last stage out to `leaves` branches.
        let deepest = (1..depth).fold(&mut node, |n, _| &mut n.children[0]);
        for _ in 1..leaves {
            deepest.children.push(EvolutionTree {
                name: "branch".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
        node
    }

    fn canvas(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    #[test]
    fn a_two_stage_chain_gets_cards_in_a_panel() {
        assert_eq!(card_grid(canvas(60, 16), 2, 2), Some((30, 8)));
    }

    #[test]
    fn a_column_never_grows_past_the_card_cap() {
        // Half of a 130-column screen would be a 65-wide card of mostly
        // whitespace; the graph is drawn tighter and centred instead.
        assert_eq!(
            card_grid(canvas(130, 16), 2, 2),
            Some((MAX_CARD_W + EVO_GAP, 8))
        );
    }

    #[test]
    fn eevees_eight_branches_do_not_fit_the_panel() {
        // The evolution panel realistically gets 15-20 rows; eight lanes need
        // MIN_CARD_H each, so the chain falls back to the text tree there...
        assert_eq!(card_grid(canvas(120, 18), 2, 8), None);
        // ...and gets its sprite cards once the full screen is handed over.
        assert!(card_grid(canvas(120, 40), 2, 8).is_some());
    }

    #[test]
    fn a_chain_too_wide_for_its_columns_falls_back() {
        // Nine stages across 80 columns leaves under MIN_CARD_W each, however
        // many rows are available.
        assert_eq!(card_grid(canvas(80, 60), 9, 1), None);
    }

    #[test]
    fn an_empty_canvas_is_not_divided_by_zero() {
        assert_eq!(card_grid(canvas(0, 0), 0, 0), None);
    }

    #[test]
    fn the_hint_row_prefers_the_cursors_requirement_over_the_key_map() {
        let s = Language::English.strings();
        let tree = chain(2, 1);
        // No cursor: the view's own hint.
        let plain = chain_hint(&tree, None, &s, "keys");
        assert_eq!(plain.spans.len(), 1);
        assert_eq!(plain.spans[0].content, "keys");
        // A cursor on a node nothing is known about keeps the same hint: an
        // empty requirement is not worth a row of its own.
        assert_eq!(
            chain_hint(&tree, Some("leaf"), &s, "keys").spans[0].content,
            "keys"
        );
    }
}
