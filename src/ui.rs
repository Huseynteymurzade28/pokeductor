//! All `ratatui` rendering. Pure functions of [`App`] state — given the same
//! state they always draw the same frame, which keeps the loop trivial.

use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::i18n::{Language, Strings};
use crate::models::{title_case, EvolutionTree, Sprite};
use crate::theme;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Column width reserved for stat labels (longest is "Verteid."/"Sp. Def").
const STAT_LABEL_WIDTH: usize = 9;

/// Entry point called once per frame by the event loop.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let strings = app.language.strings();

    // Paint the whole background first so gaps share the pastel base color.
    frame.render_widget(Block::default().style(Style::default().bg(theme::BASE)), area);

    let rows = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(0),    // body
        Constraint::Length(1), // footer / help
    ])
    .split(area);

    render_header(frame, app, &strings, rows[0]);
    render_footer(frame, &strings, rows[2]);

    let cols = Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(rows[1]);

    render_sidebar(frame, app, &strings, cols[0]);

    let right = Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(cols[1]);
    render_details(frame, app, &strings, right[0]);
    render_evolution(frame, app, &strings, right[1]);

    // The language picker floats above everything when open.
    if app.language_picker {
        render_language_picker(frame, app, &strings, area);
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
        Line::from(Span::styled("type to filter…", Style::default().fg(theme::OVERLAY)))
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
    let title = format!("{}({})", s.sidebar_title, app.filtered.len());
    let list_block = panel_block_owned(title, list_focused);
    let inner = list_block.inner(rows[1]);
    frame.render_widget(&list_block, rows[1]);

    if app.list_loading {
        render_centered_loading(frame, inner, s.loading_list, app.spinner);
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
        .map(|p| ListItem::new(Line::from(Span::styled(
            title_case(&p.name),
            Style::default().fg(theme::TEXT),
        ))))
        .collect();

    let list = List::new(items)
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(theme::BASE)
                .bg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        );
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

    lines.push(Line::from(vec![
        Span::styled(
            title_case(&detail.name),
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("   #{:04}", detail.id), Style::default().fg(theme::OVERLAY)),
    ]));

    // Pokedex genus, e.g. "Seed Pokémon" — the headline of the info card.
    if let Some(genus) = &detail.genus {
        lines.push(Line::from(Span::styled(
            genus.clone(),
            Style::default().fg(theme::PEACH).add_modifier(Modifier::ITALIC),
        )));
    }

    // Type chips.
    let mut type_spans = vec![Span::styled(
        format!("{}: ", s.types_label),
        Style::default().fg(theme::SUBTEXT),
    )];
    for ty in &detail.types {
        type_spans.push(Span::styled(
            format!(" {} ", title_case(ty)),
            Style::default().fg(theme::BASE).bg(theme::type_color(ty)),
        ));
        type_spans.push(Span::raw(" "));
    }
    lines.push(Line::from(type_spans));

    lines.push(Line::from(vec![
        Span::styled(format!("{}: ", s.height_label), Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            format!("{:.1} m", detail.height as f32 / 10.0),
            Style::default().fg(theme::TEXT),
        ),
        Span::raw("    "),
        Span::styled(format!("{}: ", s.weight_label), Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            format!("{:.1} kg", detail.weight as f32 / 10.0),
            Style::default().fg(theme::TEXT),
        ),
    ]));
    lines.push(Line::raw(""));

    // Stat bars sized to the available width.
    let bar_width = (info.width as usize).saturating_sub(STAT_LABEL_WIDTH + 6);
    for stat in &detail.stats {
        lines.push(stat_line(app.language.stat_label(stat.kind), stat.base, bar_width));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("{}: ", s.total_label), Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            detail.stat_total().to_string(),
            Style::default()
                .fg(theme::LAVENDER)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // When there's a flavor blurb and room to show it, split a small card off
    // the bottom of the info column for it; otherwise the stats use all of it.
    let flavor_rows = 4;
    match &detail.flavor {
        Some(flavor) if info.height as usize > lines.len() + flavor_rows => {
            let split = Layout::vertical([Constraint::Min(0), Constraint::Length(flavor_rows as u16)])
                .split(info);
            frame.render_widget(Paragraph::new(lines), split[0]);
            render_flavor_card(frame, split[1], flavor);
        }
        _ => frame.render_widget(Paragraph::new(lines), info),
    }
}

/// Renders the Pokedex flavor-text blurb as a quoted, word-wrapped little card.
fn render_flavor_card(frame: &mut Frame, area: Rect, flavor: &str) {
    let para = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("“{flavor}”"),
            Style::default().fg(theme::SUBTEXT).add_modifier(Modifier::ITALIC),
        )),
    ])
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
    if area.width < 2 || area.height < 1 || sprite.width == 0 || sprite.height == 0 {
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
    let span_x = |cx: u32| (bx0 + cx * bw / cols_u, bx0 + ((cx + 1) * bw / cols_u).saturating_sub(1));
    let span_y = |py: u32| (by0 + py * bh / sub_rows, by0 + ((py + 1) * bh / sub_rows).saturating_sub(1));

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
    let block = panel_block(s.evolution_title, focused);
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
    let canvas = rows[0];

    let depth = tree.depth() as u16;
    let leaves = tree.leaf_count() as u16;
    let col_w = canvas.width.checked_div(depth).unwrap_or(0);
    let lane_h = canvas.height.checked_div(leaves).unwrap_or(0);

    // Draw the sprite graph when every card has room; otherwise fall back to the
    // compact text tree so cramped terminals still show the relationships.
    if col_w >= MIN_CARD_W && lane_h >= MIN_CARD_H {
        let mut lane = 0u16;
        place_node(frame, app, s, tree, current, cursor, canvas, col_w, lane_h, 0, &mut lane);
    } else {
        frame.render_widget(Paragraph::new(evolution_lines(tree, cursor.or(current))), canvas);
    }

    let hint = if focused { s.evo_nav_hint } else { s.expand_hint };
    let hint = Paragraph::new(Line::from(Span::styled(hint, Style::default().fg(theme::OVERLAY))))
        .alignment(Alignment::Center);
    frame.render_widget(hint, rows[1]);
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
        Span::styled("█".repeat(filled), Style::default().fg(theme::stat_color(base))),
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
        Line::from(Span::styled(err.to_string(), Style::default().fg(theme::SUBTEXT))),
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
    let para = Paragraph::new(Line::from(Span::styled(text.to_string(), Style::default().fg(color))))
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
/// drawn horizontally (`A ──▶ B ──▶ C`); wherever a species branches, the
/// children are laid out vertically with `├──`/`└──` connectors.
fn evolution_lines(tree: &EvolutionTree, highlight: Option<&str>) -> Vec<Line<'static>> {
    node_block(tree, highlight)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// Returns the block of span-rows for `node` and its descendants, without any
/// outer indentation (the caller prepends connectors).
fn node_block(node: &EvolutionTree, highlight: Option<&str>) -> Vec<Vec<Span<'static>>> {
    // Walk the linear run: follow single-child links onto one horizontal line.
    let mut run: Vec<&EvolutionTree> = vec![node];
    let mut cur = node;
    while cur.children.len() == 1 {
        cur = &cur.children[0];
        run.push(cur);
    }

    let mut first: Vec<Span<'static>> = Vec::new();
    for (i, n) in run.iter().enumerate() {
        if i > 0 {
            first.push(Span::styled(" ──▶ ", Style::default().fg(theme::OVERLAY)));
        }
        first.push(name_span(&n.name, highlight));
    }
    let mut lines = vec![first];

    // `cur` ends the run; if it branches, lay children out vertically beneath
    // the final name of the run.
    if cur.children.len() > 1 {
        let mut indent_width = 0usize;
        for n in &run[..run.len() - 1] {
            indent_width += title_case(&n.name).chars().count();
        }
        indent_width += (run.len() - 1) * 5; // each " ──▶ " is 5 columns
        let indent = " ".repeat(indent_width);

        let count = cur.children.len();
        for (i, child) in cur.children.iter().enumerate() {
            let is_last = i == count - 1;
            for (j, child_row) in node_block(child, highlight).into_iter().enumerate() {
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
                frame, app, s, child, current, cursor, canvas, col_w, lane_h, depth_idx + 1, lane,
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
    let sprite_area = Rect { x, y: top, width: w, height: h.saturating_sub(1) };
    match app.sprites.get(&node.name) {
        Some(sprite) => render_sprite_capped(frame, sprite_area, sprite, w),
        None => {
            let placeholder = if app.sprite_loading.contains(&node.name) {
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
        Style::default()
            .fg(theme::BASE)
            .bg(theme::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else if is_current {
        Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::GREEN)
    };
    let name = Paragraph::new(Line::from(Span::styled(title_case(&node.name), style)))
        .alignment(Alignment::Center);
    frame.render_widget(name, Rect { x, y: top + h.saturating_sub(1), width: w, height: 1 });
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
    let junction = if centers.contains(&parent_cy) { "┼" } else { "┤" };
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

// --- Language picker ------------------------------------------------------

/// Draws the little modal card for switching interface language.
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
            Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD),
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
            Style::default()
                .fg(theme::BASE)
                .bg(theme::MAUVE)
                .add_modifier(Modifier::BOLD)
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
