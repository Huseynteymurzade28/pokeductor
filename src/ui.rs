//! All `ratatui` rendering. Pure functions of [`App`] state — given the same
//! state they always draw the same frame, which keeps the loop trivial.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::i18n::Strings;
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

    frame.render_widget(Paragraph::new(lines), info);
}

// --- Sprite rendering ----------------------------------------------------

/// Maximum cell width we'll ever give a sprite, so it stays a tasteful accent
/// rather than swallowing the panel on very wide terminals.
const MAX_SPRITE_COLS: u16 = 34;

/// Chooses the sprite column width: square-ish, bounded by ~40% of the panel
/// width, the available height (two pixels per cell row), and [`MAX_SPRITE_COLS`].
fn sprite_col_width(inner: Rect) -> u16 {
    let by_width = inner.width * 2 / 5;
    let by_height = inner.height.saturating_mul(2);
    let w = by_width.min(by_height).min(MAX_SPRITE_COLS);
    (w & !1).max(2) // keep it even so rows = cols / 2 divides cleanly
}

/// Draws `sprite` into `area` using upper-half-block characters: each cell packs
/// two vertical pixels (foreground = top, background = bottom), so one terminal
/// row shows two image rows. The image is nearest-neighbour downsampled to fit
/// and centred within `area`.
fn render_sprite(frame: &mut Frame, area: Rect, sprite: &Sprite) {
    let cols = area.width.min(area.height.saturating_mul(2)).min(MAX_SPRITE_COLS) & !1;
    let rows = cols / 2;
    if cols < 2 || rows < 1 || sprite.width == 0 || sprite.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for cy in 0..rows {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for cx in 0..cols {
            let sx = (cx as u32 * sprite.width) / cols as u32;
            let sy_top = (2 * cy as u32 * sprite.height) / (2 * rows as u32);
            let sy_bot = ((2 * cy as u32 + 1) * sprite.height) / (2 * rows as u32);
            let top = pixel_color(sprite.sample(sx, sy_top));
            let bottom = pixel_color(sprite.sample(sx, sy_bot));
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

/// Maps an RGBA pixel to a terminal colour, collapsing (mostly) transparent
/// pixels onto the panel background so the sprite blends into the UI.
fn pixel_color(rgba: [u8; 4]) -> ratatui::style::Color {
    if rgba[3] < 128 {
        theme::BASE
    } else {
        ratatui::style::Color::Rgb(rgba[0], rgba[1], rgba[2])
    }
}

fn render_evolution(frame: &mut Frame, app: &App, s: &Strings, area: Rect) {
    let block = panel_block(s.evolution_title, false);
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

    let highlight = app.selected_name.as_deref();
    let lines = evolution_lines(tree, highlight);
    frame.render_widget(Paragraph::new(lines), inner);
}

// --- Small rendering helpers ---------------------------------------------

fn panel_block(title: &'static str, focused: bool) -> Block<'static> {
    panel_block_owned(title.to_string(), focused)
}

fn panel_block_owned(title: String, focused: bool) -> Block<'static> {
    let (border, text) = if focused {
        (theme::MAUVE, theme::MAUVE)
    } else {
        (theme::OVERLAY, theme::SUBTEXT)
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
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
