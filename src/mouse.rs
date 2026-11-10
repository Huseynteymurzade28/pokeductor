//! Where things were drawn, so a click can be told what it landed on.
//!
//! The keyboard needs to know nothing about the screen: a key means the same
//! thing wherever the panels happen to sit. A click is only a column and a
//! row, and what is under it depends on the terminal's size, how far the list
//! is scrolled and whether a chain fitted as cards. Working that out a second
//! time in the event handler would mean two copies of the layout drifting
//! apart, so the draw pass records what it drew into a [`Hits`] and the
//! handler reads it back. A click is always against the frame it was aimed
//! at, which is the last one drawn.
//!
//! Nothing here knows what a click *does*. That is [`crate::app::App`]'s
//! half, expressed through the same methods the keys call.

use std::io::Write;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::layout::{Position, Rect};

/// Rows the list moves per notch of the wheel. One is precise and slow over
/// thirteen hundred entries; three is what terminals and editors scroll by.
pub const WHEEL_ROWS: i32 = 3;

/// What the last frame drew, as far as the mouse is concerned.
///
/// Rebuilt from empty on every frame, so something that was not drawn this
/// time cannot be clicked where it used to be.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Hits {
    /// The search box, border included: the whole box is the target.
    pub search: Rect,
    /// The rows of the species list, inside its border.
    pub list: Rect,
    /// Which entry of the filtered list is on the list's first row.
    pub list_offset: usize,
    /// The evolution panel, border included.
    pub evolution: Rect,
    /// The chain's sprite cards, each with its index in the depth-first order
    /// the evolution cursor counts in. Empty when the chain was drawn as a
    /// text tree, which has no cards to aim at.
    pub stages: Vec<(Rect, usize)>,
    /// The overlay card on top, if one is open and had room to draw.
    pub card: Option<Rect>,
}

/// What a click on the main view landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Search,
    /// A row of the list, as an index into the filtered list. May be past
    /// its end: the rows under a short list are still inside the panel.
    ListRow(usize),
    /// A chain member, by its index in depth-first order.
    Stage(usize),
    /// The evolution panel, but none of its cards.
    Evolution,
    Nothing,
}

impl Hits {
    /// What sits under `position` on the main view.
    ///
    /// Stages are tested before the panel they are drawn in, since a card is
    /// the more specific answer of the two.
    pub fn target(&self, position: Position) -> Target {
        if let Some(stage) = self.stage_at(position) {
            return Target::Stage(stage);
        }
        if self.list.contains(position) {
            let row = usize::from(position.y - self.list.y);
            return Target::ListRow(self.list_offset + row);
        }
        if self.search.contains(position) {
            return Target::Search;
        }
        if self.evolution.contains(position) {
            return Target::Evolution;
        }
        Target::Nothing
    }

    /// The chain member whose card is under `position`, if any.
    pub fn stage_at(&self, position: Position) -> Option<usize> {
        self.stages
            .iter()
            .find(|(area, _)| area.contains(position))
            .map(|&(_, index)| index)
    }

    /// Whether `position` is on the open overlay card. A card that is open
    /// but had no room to draw has no inside, so every click is outside it.
    pub fn on_card(&self, position: Position) -> bool {
        self.card.is_some_and(|card| card.contains(position))
    }
}

/// Asks the terminal to report the mouse, or to stop.
///
/// Best-effort, like the clipboard: a terminal that does not understand the
/// request ignores it, and then no mouse events arrive and nothing else
/// changes.
pub fn capture(on: bool) {
    let mut out = std::io::stdout();
    let _ = match on {
        true => crossterm::execute!(out, EnableMouseCapture),
        false => crossterm::execute!(out, DisableMouseCapture),
    };
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: u16, y: u16) -> Position {
        Position { x, y }
    }

    /// A screen with the search box over the list on the left and the
    /// evolution panel, holding two cards, at the bottom right.
    fn hits() -> Hits {
        Hits {
            search: Rect::new(0, 1, 30, 3),
            list: Rect::new(1, 5, 28, 20),
            list_offset: 40,
            evolution: Rect::new(30, 15, 50, 12),
            stages: vec![(Rect::new(32, 17, 12, 8), 0), (Rect::new(50, 17, 12, 8), 1)],
            card: None,
        }
    }

    #[test]
    fn a_list_row_counts_from_the_row_the_list_is_scrolled_to() {
        assert_eq!(hits().target(at(10, 5)), Target::ListRow(40));
        assert_eq!(hits().target(at(10, 8)), Target::ListRow(43));
    }

    #[test]
    fn the_list_border_is_not_a_row() {
        assert_eq!(hits().target(at(0, 8)), Target::Nothing);
        assert_eq!(hits().target(at(10, 4)), Target::Nothing);
    }

    #[test]
    fn the_whole_search_box_takes_the_click() {
        assert_eq!(hits().target(at(0, 1)), Target::Search);
        assert_eq!(hits().target(at(15, 2)), Target::Search);
    }

    #[test]
    fn a_card_answers_before_the_panel_it_is_drawn_in() {
        assert_eq!(hits().target(at(55, 20)), Target::Stage(1));
        assert_eq!(hits().target(at(46, 20)), Target::Evolution);
    }

    #[test]
    fn a_card_with_no_room_to_draw_has_no_inside() {
        let mut hits = hits();
        assert!(!hits.on_card(at(40, 10)));
        hits.card = Some(Rect::new(20, 5, 40, 10));
        assert!(hits.on_card(at(40, 10)));
        assert!(!hits.on_card(at(10, 10)));
    }
}
