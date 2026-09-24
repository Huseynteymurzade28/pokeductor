//! The sidebar's own state: the master list, what the search box narrowed it
//! to, how it is ordered, and where the cursor sits.
//!
//! Split out of [`crate::app::App`] because it is the one part of the running
//! app that is pure. There is no client here, no channel and no task: entries
//! and rosters go in, a visible list and a cursor come out. Everything the
//! sidebar does that is easy to break by accident — the highlight surviving a
//! narrowing search, a filter waiting on a roster matching nothing rather than
//! everything — is decided here, and can be tested without standing up the
//! rest of the app.
//!
//! What stays behind in `App` is the half that talks to the network: which
//! rosters are in flight, and the fetch a new term kicks off. This module only
//! ever reads the rosters that have already landed.

use std::collections::{BTreeSet, HashMap, HashSet};

use ratatui::widgets::ListState;

use crate::models::{PokemonEntry, RosterTerm};
use crate::query::Query;

/// How the sidebar orders whatever survived the filter.
///
/// Both keys are derived from data the list response already carries, so
/// sorting never costs a request. Ordering by base-stat total would: it needs
/// every species' stats, which is 1300 fetches for one keypress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortKey {
    /// National Pokedex order — PokeAPI's own, and the default.
    #[default]
    Dex,
    /// Alphabetical by name.
    Name,
}

impl SortKey {
    /// The next key in the cycle, for the sort hotkey.
    pub fn next(self) -> Self {
        match self {
            SortKey::Dex => SortKey::Name,
            SortKey::Name => SortKey::Dex,
        }
    }

    /// Stable name used to record the ordering in a session file, so that
    /// reordering this enum can never change what a stored session means.
    pub fn code(self) -> &'static str {
        match self {
            SortKey::Dex => "dex",
            SortKey::Name => "name",
        }
    }

    /// The inverse of [`code`](Self::code). An unrecognised value is `None`,
    /// and the caller keeps the default ordering.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "dex" => Some(SortKey::Dex),
            "name" => Some(SortKey::Name),
            _ => None,
        }
    }
}

/// The master list, the search box, and the cursor over the result.
#[derive(Default)]
pub struct Browser {
    /// Every entry PokeAPI serves, in the order it served them.
    pub all: Vec<PokemonEntry>,
    /// Indices into [`all`](Self::all) that match the current query, in the
    /// order the sort key puts them.
    pub filtered: Vec<usize>,
    pub list_state: ListState,
    /// Raw contents of the search box, exactly as typed.
    pub query: String,
    /// `query` after parsing, kept so the renderer can describe the active
    /// filter without re-parsing on every frame.
    pub parsed: Query,
    pub sort: SortKey,
    /// Membership lists for the filter terms that have been resolved so far.
    /// An entry that is present but empty means "we asked and got nothing
    /// back", which is a different thing from not having asked.
    pub rosters: HashMap<RosterTerm, HashSet<String>>,
    /// Species marked as favourites, by raw API name. What `fav:` narrows to.
    /// Ordered so the session file lists them the same way every run.
    pub favourites: BTreeSet<String>,
}

impl Browser {
    /// Rebuilds the visible list from the search box and the sort key.
    ///
    /// Called after anything that can change either, and cheap enough to run
    /// on every keystroke: the work is one pass over ~1300 entries plus a
    /// sort.
    pub fn recompute(&mut self) {
        let query = Query::parse(&self.query);

        // Remember what was highlighted so the same Pokemon stays under the
        // cursor when the list is merely re-sorted, or when it survives a
        // narrowing search. Losing the highlight on every keystroke is the
        // main thing that makes a filtered list annoying to use.
        let anchor = self.current_name();

        let mut filtered: Vec<usize> = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                query.matches_entry(p)
                    && self.in_every_roster(&query, &p.name)
                    && (!query.favourites || self.favourites.contains(&p.name))
            })
            .map(|(idx, _)| idx)
            .collect();

        match self.sort {
            SortKey::Dex => filtered.sort_unstable_by_key(|&idx| self.all[idx].id),
            SortKey::Name => {
                filtered.sort_unstable_by(|&a, &b| self.all[a].name.cmp(&self.all[b].name));
            }
        }

        self.filtered = filtered;
        self.parsed = query;
        self.restore_highlight(anchor);
    }

    /// Whether `name` is in the roster of every filter term the query asks
    /// for. A roster we do not have yet matches nothing, which leaves the list
    /// empty until it lands — the sidebar says as much while that is true.
    fn in_every_roster(&self, query: &Query, name: &str) -> bool {
        query.rosters.iter().all(|term| {
            self.rosters
                .get(term)
                .is_some_and(|members| members.contains(name))
        })
    }

    /// Puts the cursor back on `anchor` if it is still visible, and on the
    /// first row otherwise.
    fn restore_highlight(&mut self, anchor: Option<String>) {
        if self.filtered.is_empty() {
            self.list_state.select(None);
            return;
        }
        let restored = anchor
            .and_then(|name| self.index_of(&name))
            .and_then(|abs| self.filtered.iter().position(|&idx| idx == abs));
        self.list_state.select(Some(restored.unwrap_or(0)));
    }

    /// True while a filter term is still waiting on its roster, so the sidebar
    /// can say "loading" rather than "no results".
    pub fn awaiting_roster(&self) -> bool {
        self.parsed
            .rosters
            .iter()
            .any(|term| !self.rosters.contains_key(term))
    }

    /// Marks the highlighted species as a favourite, or unmarks it.
    ///
    /// Under a `fav:` filter an unmarked species no longer belongs in the list,
    /// so the list is rebuilt; the cursor then lands where
    /// [`recompute`](Self::recompute) always puts it when its species goes.
    pub fn toggle_favourite(&mut self) {
        let Some(name) = self.current_name() else {
            return;
        };
        if !self.favourites.remove(&name) {
            self.favourites.insert(name);
        }
        if self.parsed.favourites {
            self.recompute();
        }
    }

    pub fn is_favourite(&self, name: &str) -> bool {
        self.favourites.contains(name)
    }

    /// Cycles the sort order, keeping the highlighted species under the cursor
    /// across the re-ordering.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.recompute();
    }

    /// Moves the cursor by `delta` rows, wrapping at both ends. An empty list
    /// has nowhere to move to.
    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len() as i32;
        let current = self.list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len);
        self.list_state.select(Some(next as usize));
    }

    /// Raw API name of the highlighted entry, if any.
    pub fn current_name(&self) -> Option<String> {
        let selected = self.list_state.selected()?;
        let idx = *self.filtered.get(selected)?;
        self.all.get(idx).map(|p| p.name.clone())
    }

    /// Where `name` sits in the master list, whether or not the filter is
    /// currently showing it.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.all.iter().position(|p| p.name == name)
    }

    /// Where `name` sits in the list as currently filtered, if it does.
    pub fn position_of(&self, name: &str) -> Option<usize> {
        self.filtered
            .iter()
            .position(|&idx| self.all[idx].name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RosterKind, RosterTerm};

    /// A browser over `entries`, filtered and with the cursor parked, which is
    /// the state every keypress starts from.
    fn browsing(entries: &[(u32, &str)]) -> Browser {
        let mut browser = Browser {
            all: entries
                .iter()
                .map(|&(id, name)| PokemonEntry {
                    id,
                    name: name.to_string(),
                })
                .collect(),
            ..Browser::default()
        };
        browser.recompute();
        browser
    }

    fn kanto_ghosts() -> Browser {
        browsing(&[
            (92, "gastly"),
            (93, "haunter"),
            (94, "gengar"),
            (25, "pikachu"),
        ])
    }

    /// Types a query into the box and re-filters, as a keystroke does.
    fn search(browser: &mut Browser, query: &str) {
        browser.query = query.to_string();
        browser.recompute();
    }

    /// The names the sidebar would draw, top to bottom.
    fn visible(browser: &Browser) -> Vec<&str> {
        browser
            .filtered
            .iter()
            .map(|&idx| browser.all[idx].name.as_str())
            .collect()
    }

    #[test]
    fn every_sort_key_reads_back_out_of_its_code() {
        for sort in [SortKey::Dex, SortKey::Name] {
            assert_eq!(SortKey::from_code(sort.code()), Some(sort));
        }
    }

    #[test]
    fn an_unknown_sort_code_is_not_guessed_at() {
        assert_eq!(SortKey::from_code("stat-total"), None);
        assert_eq!(SortKey::from_code(""), None);
    }

    #[test]
    fn a_narrowing_search_keeps_the_highlighted_species_under_the_cursor() {
        // The contract that makes a filtered list usable: typing more of a
        // name you can already see must not move what you were looking at.
        let mut browser = kanto_ghosts();
        browser.list_state.select(Some(3)); // gengar, last in dex order
        assert_eq!(browser.current_name().as_deref(), Some("gengar"));

        search(&mut browser, "ga");
        assert_eq!(visible(&browser), ["gastly", "gengar"]);
        assert_eq!(browser.current_name().as_deref(), Some("gengar"));
    }

    #[test]
    fn a_species_the_search_filters_out_drops_the_cursor_to_the_first_row() {
        let mut browser = kanto_ghosts();
        browser.list_state.select(Some(0)); // pikachu, first in dex order
        search(&mut browser, "ga");
        assert_eq!(browser.current_name().as_deref(), Some("gastly"));

        // And a search matching nothing leaves nothing selected, rather than
        // a cursor pointing at a row that is not there.
        search(&mut browser, "nothing matches this");
        assert!(browser.filtered.is_empty());
        assert_eq!(browser.list_state.selected(), None);
        assert_eq!(browser.current_name(), None);
    }

    #[test]
    fn sorting_reorders_the_list_without_moving_the_highlight() {
        let mut browser = kanto_ghosts();
        browser.list_state.select(Some(0)); // pikachu, first in dex order
        assert_eq!(
            visible(&browser),
            ["pikachu", "gastly", "haunter", "gengar"]
        );

        browser.cycle_sort();
        assert_eq!(browser.sort, SortKey::Name);
        assert_eq!(
            visible(&browser),
            ["gastly", "gengar", "haunter", "pikachu"]
        );
        assert_eq!(
            browser.current_name().as_deref(),
            Some("pikachu"),
            "the species stays under the cursor; only its row number changed"
        );

        browser.cycle_sort();
        assert_eq!(browser.sort, SortKey::Dex);
        assert_eq!(browser.current_name().as_deref(), Some("pikachu"));
    }

    #[test]
    fn the_cursor_wraps_at_both_ends_and_an_empty_list_has_nowhere_to_go() {
        let mut browser = kanto_ghosts();
        browser.list_state.select(Some(0));

        browser.move_selection(-1);
        assert_eq!(
            browser.list_state.selected(),
            Some(3),
            "off the top, round to the bottom"
        );
        browser.move_selection(1);
        assert_eq!(browser.list_state.selected(), Some(0));

        // The ten-row jump wraps by the same rule rather than clamping.
        browser.move_selection(10);
        assert_eq!(browser.list_state.selected(), Some(2));

        let mut empty = browsing(&[]);
        empty.move_selection(1);
        assert_eq!(empty.list_state.selected(), None);
    }

    #[test]
    fn a_filter_whose_roster_has_not_arrived_matches_nothing_and_says_so() {
        // A roster we have not fetched cannot be treated as "everything" —
        // that would show the whole list under a filter that has not been
        // applied yet. It matches nothing, and the sidebar reports the wait.
        let mut browser = kanto_ghosts();
        search(&mut browser, "type:ghost");
        assert!(browser.filtered.is_empty());
        assert!(browser.awaiting_roster());

        // Once it lands, the filter applies and the wait is over.
        browser.rosters.insert(
            RosterTerm {
                kind: RosterKind::Type,
                value: "ghost".to_string(),
            },
            ["gastly", "haunter", "gengar"]
                .iter()
                .map(|n| n.to_string())
                .collect(),
        );
        browser.recompute();
        assert_eq!(visible(&browser), ["gastly", "haunter", "gengar"]);
        assert!(!browser.awaiting_roster());

        // An empty answer is an answer: the list is empty and nothing is
        // still being waited on.
        search(&mut browser, "type:nonsense");
        browser.rosters.insert(
            RosterTerm {
                kind: RosterKind::Type,
                value: "nonsense".to_string(),
            },
            HashSet::new(),
        );
        browser.recompute();
        assert!(browser.filtered.is_empty());
        assert!(!browser.awaiting_roster());
    }

    #[test]
    fn a_name_is_found_in_the_master_list_and_in_the_filtered_one() {
        let mut browser = kanto_ghosts();
        search(&mut browser, "ga");
        // Third in the master list as PokeAPI served it, second on screen.
        assert_eq!(browser.index_of("gengar"), Some(2));
        assert_eq!(browser.position_of("gengar"), Some(1));
        // Filtered out: still in the master list, nowhere on screen.
        assert_eq!(browser.index_of("pikachu"), Some(3));
        assert_eq!(browser.position_of("pikachu"), None);
        assert_eq!(browser.index_of("missingno"), None);
    }

    /// Marks each of `names` from the list, the way the key does.
    fn mark(browser: &mut Browser, names: &[&str]) {
        for name in names {
            let row = browser.position_of(name).expect("visible to be marked");
            browser.list_state.select(Some(row));
            browser.toggle_favourite();
        }
    }

    #[test]
    fn marking_twice_leaves_a_species_as_it_was() {
        let mut browser = kanto_ghosts();
        mark(&mut browser, &["gengar"]);
        assert!(browser.is_favourite("gengar"));
        mark(&mut browser, &["gengar"]);
        assert!(!browser.is_favourite("gengar"));
    }

    #[test]
    fn fav_narrows_to_favourites_and_needs_nothing_fetched() {
        let mut browser = kanto_ghosts();
        mark(&mut browser, &["pikachu", "gastly"]);
        search(&mut browser, "fav:");
        assert_eq!(visible(&browser), ["pikachu", "gastly"]);
        assert!(!browser.awaiting_roster());
    }

    #[test]
    fn fav_combines_with_the_other_terms() {
        let mut browser = kanto_ghosts();
        mark(&mut browser, &["pikachu", "gastly", "gengar"]);
        search(&mut browser, "fav: g");
        assert_eq!(visible(&browser), ["gastly", "gengar"]);
        search(&mut browser, "fav: dex:90-93");
        assert_eq!(visible(&browser), ["gastly"]);
    }

    #[test]
    fn unmarking_under_fav_drops_the_species_from_the_list() {
        let mut browser = kanto_ghosts();
        mark(&mut browser, &["gastly", "gengar"]);
        search(&mut browser, "fav:");
        mark(&mut browser, &["gastly"]);
        assert_eq!(visible(&browser), ["gengar"]);
        assert_eq!(browser.current_name().as_deref(), Some("gengar"));
    }

    #[test]
    fn with_no_favourites_fav_is_an_empty_list() {
        let mut browser = kanto_ghosts();
        search(&mut browser, "fav:");
        assert!(browser.filtered.is_empty());
        assert!(!browser.awaiting_roster(), "empty, not loading");
    }
}
