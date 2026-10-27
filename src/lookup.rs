//! The lookup card: a search over every ability and every item, beside the
//! species search the sidebar already is.
//!
//! What lives here is the part with no terminal or network in it — which
//! entries a query matches and in what order, and where the cursor goes. The
//! lists themselves and the records behind each entry are fetched by `app.rs`.

use crate::i18n;
use crate::models::LookupKind;

/// The card's state between keypresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    pub kind: LookupKind,
    /// What has been typed into the card's own search line.
    pub query: String,
    /// Index into the *filtered* list, not the full one.
    pub cursor: usize,
}

impl Default for Lookup {
    fn default() -> Self {
        Self {
            kind: LookupKind::Ability,
            query: String::new(),
            cursor: 0,
        }
    }
}

impl Lookup {
    /// Switches between abilities and items. The query is kept: `poison` is a
    /// fair thing to look for on both sides, and retyping it is the cost of
    /// clearing it.
    pub fn switch_kind(&mut self) {
        self.kind = match self.kind {
            LookupKind::Ability => LookupKind::Item,
            LookupKind::Item => LookupKind::Ability,
        };
        self.cursor = 0;
    }

    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.cursor = 0;
    }

    pub fn pop(&mut self) {
        self.query.pop();
        self.cursor = 0;
    }

    /// Moves the cursor by `delta`, clamping at both ends of a list `len`
    /// long, as the moves card does: two thousand items is one long list, and
    /// wrapping off the end of it loses the reader's place.
    pub fn move_cursor(&mut self, delta: i32, len: usize) {
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let next = self.cursor as i64 + i64::from(delta);
        self.cursor = next.clamp(0, len as i64 - 1) as usize;
    }

    /// The search-box term that narrows the species list to the Pokemon that
    /// go with `slug`.
    pub fn species_query(&self, slug: &str) -> String {
        match self.kind {
            LookupKind::Ability => format!("ability:{slug}"),
            LookupKind::Item => format!("held:{slug}"),
        }
    }
}

/// Indices into `names` of the entries `query` matches, best first.
///
/// An entry matches when every word of the query appears in its name, so
/// `ball gr` finds Great Ball. Ranking puts an exact name first, then names
/// that start with the query, then the rest, each group in PokeAPI's order —
/// which for abilities is the order they were introduced in, and is the
/// nearest thing to an order either list has. Without the ranking `ball`
/// would open on Master Ball at the top and Ball Fetch three hundred rows
/// further down, which is right, but `leftovers` would have to compete with
/// everything else containing `left`.
///
/// Names are matched in English, the slugs PokeAPI files them under: the
/// translated names only arrive one record at a time, and a search that
/// found an entry only after it had once been opened would be worse than one
/// that is consistently English.
pub fn matches(names: &[String], query: &str) -> Vec<usize> {
    let query = normalize(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return (0..names.len()).collect();
    }

    let mut ranked: Vec<(u8, usize)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let name = normalize(name);
            if !words.iter().all(|word| name.contains(word)) {
                return None;
            }
            let rank = if name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else {
                2
            };
            Some((rank, i))
        })
        .collect();
    // Stable, so each rank keeps PokeAPI's order.
    ranked.sort_by_key(|&(rank, _)| rank);
    ranked.into_iter().map(|(_, i)| i).collect()
}

/// Reduces a slug or a typed query to lowercase words: hyphens become spaces,
/// accents and punctuation go, so `King's Rock`, `kings-rock` and `kings rock`
/// all read the same.
fn normalize(text: &str) -> String {
    let folded: String = i18n::fold(text)
        .chars()
        .filter_map(|c| match c {
            '-' | '_' => Some(' '),
            c if c.is_alphanumeric() || c.is_whitespace() => Some(c),
            _ => None,
        })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    fn found<'a>(list: &'a [String], query: &str) -> Vec<&'a str> {
        matches(list, query)
            .into_iter()
            .map(|i| list[i].as_str())
            .collect()
    }

    #[test]
    fn an_empty_query_matches_everything_in_order() {
        let list = names(&["stench", "drizzle", "speed-boost"]);
        assert_eq!(found(&list, ""), ["stench", "drizzle", "speed-boost"]);
        assert_eq!(found(&list, "   "), ["stench", "drizzle", "speed-boost"]);
    }

    #[test]
    fn every_word_has_to_appear_somewhere_in_the_name() {
        let list = names(&["master-ball", "ultra-ball", "great-ball", "great-twin-bolt"]);
        assert_eq!(found(&list, "ball gr"), ["great-ball"]);
        assert_eq!(found(&list, "great"), ["great-ball", "great-twin-bolt"]);
    }

    #[test]
    fn an_exact_name_then_a_prefix_lead_the_rest() {
        let list = names(&["choice-band", "band-of-old", "band"]);
        assert_eq!(found(&list, "band"), ["band", "band-of-old", "choice-band"]);
    }

    #[test]
    fn hyphens_spaces_case_and_punctuation_are_all_the_same() {
        let list = names(&["kings-rock", "rocky-helmet"]);
        for query in ["King's Rock", "kings rock", "KINGS-ROCK"] {
            assert_eq!(found(&list, query), ["kings-rock"], "{query}");
        }
    }

    #[test]
    fn accents_are_optional() {
        let list = names(&["poke-ball"]);
        assert_eq!(found(&list, "Poké Ball"), ["poke-ball"]);
    }

    #[test]
    fn the_cursor_clamps_and_an_empty_list_holds_it_at_zero() {
        let mut lookup = Lookup::default();
        lookup.move_cursor(-1, 5);
        assert_eq!(lookup.cursor, 0);
        lookup.move_cursor(10, 5);
        assert_eq!(lookup.cursor, 4);
        lookup.move_cursor(1, 0);
        assert_eq!(lookup.cursor, 0);
    }

    #[test]
    fn typing_and_switching_go_back_to_the_top_but_keep_the_query() {
        let mut lookup = Lookup::default();
        lookup.push('p');
        lookup.cursor = 3;
        lookup.switch_kind();
        assert_eq!(lookup.kind, LookupKind::Item);
        assert_eq!((lookup.query.as_str(), lookup.cursor), ("p", 0));
        lookup.cursor = 3;
        lookup.pop();
        assert_eq!((lookup.query.as_str(), lookup.cursor), ("", 0));
    }

    #[test]
    fn each_kind_narrows_the_species_list_by_its_own_term() {
        let mut lookup = Lookup::default();
        assert_eq!(lookup.species_query("levitate"), "ability:levitate");
        lookup.switch_kind();
        assert_eq!(lookup.species_query("leftovers"), "held:leftovers");
    }
}
