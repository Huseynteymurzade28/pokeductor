//! Parsing for the search box.
//!
//! The box takes plain text, and on top of that a couple of `key:value` terms
//! that narrow by something the name cannot express:
//!
//! ```text
//! char                 name contains "char"
//! 25                   dex number 25 — Pikachu — or a name containing "25"
//! dex:25               dex number 25, without the name fallback
//! dex:1-151            every dex number in that range — generation 1
//! type:water           every Water Pokemon
//! type:water type:fly  Water *and* Flying — Gyarados, Mantine, ...
//! ability:levitate     every Pokemon that can have Levitate
//! egg:dragon           every species in the Dragon breeding group
//! gen:1 gen:2          introduced in either generation
//! gen:1 type:ghost ga  all three, combined
//! ```
//!
//! Anything that is not a recognised term is treated as ordinary text, so a
//! stray colon degrades to a name search instead of an error.

use std::ops::RangeInclusive;

use crate::models::{PokemonEntry, RosterKind, RosterTerm};

/// A parsed search query. The default value matches everything.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    /// Free text, lowercased, matched as a substring of the species name.
    pub text: String,
    /// `text` read as a dex number, when it is one on its own.
    ///
    /// A bare number searches by number *as well as* by name rather than
    /// instead of it: `2` should still find `porygon2`, and only the user
    /// knows which of the two they meant.
    pub text_dex: Option<u32>,
    /// Membership terms — `type:`, `ability:`, `egg:` — each of which needs a
    /// roster the caller fetches separately.
    ///
    /// Combined with AND, across kinds as well as within one: a Pokemon must
    /// satisfy *every* term. That is the useful reading, because searching two
    /// types is how you look for a specific dual typing, and pairing a type
    /// with an ability is how you narrow rather than widen.
    pub rosters: Vec<RosterTerm>,
    /// Generations from `gen:` terms.
    ///
    /// Combined with OR — unlike types, since a species belongs to exactly one
    /// generation and requiring two at once could only ever match nothing.
    pub generations: Vec<u8>,
    /// Dex numbers from `dex:` terms, each one a single number or a range.
    ///
    /// Combined with OR, for the same reason generations are: a species has
    /// exactly one dex number.
    pub dex: Vec<RangeInclusive<u32>>,
}

impl Query {
    pub fn parse(raw: &str) -> Self {
        let mut query = Query::default();
        let mut words: Vec<&str> = Vec::new();

        for token in raw.split_whitespace() {
            match token.split_once(':') {
                // A roster term with nothing after the colon is one the user is
                // still typing, and is dropped for the same reason `gen:` is
                // below: filtering on the empty string, or falling back to
                // searching names for "type:", both answer a question nobody
                // asked.
                Some(("type" | "t", value)) => {
                    query.push_roster(RosterKind::Type, value);
                }
                Some(("ability" | "a", value)) => {
                    query.push_roster(RosterKind::Ability, value);
                }
                Some(("egg" | "e", value)) => {
                    query.push_roster(RosterKind::EggGroup, &egg_group_slug(value));
                }
                // `gen:` with nothing usable after it is still a filter the
                // user is in the middle of typing, not a name to search for —
                // dropping it keeps the list from flickering through unrelated
                // results on the way to `gen:4`.
                Some(("gen" | "g", value)) => {
                    if let Ok(gen) = value.parse::<u8>() {
                        query.generations.push(gen);
                    }
                }
                // Same reading for a half-typed `dex:1-`, which is a range on
                // its way to `dex:1-151`.
                Some(("dex" | "d", value)) => {
                    if let Some(range) = parse_dex_range(value) {
                        query.dex.push(range);
                    }
                }
                _ => words.push(token),
            }
        }

        query.text = words.join(" ").to_lowercase();
        query.text_dex = query.text.parse().ok();
        query
    }

    /// Records a roster term, ignoring an empty value and any repeat of a term
    /// already present — `type:fire type:fire` narrows no further than one.
    fn push_roster(&mut self, kind: RosterKind, value: &str) {
        if value.is_empty() {
            return;
        }
        let term = RosterTerm::new(kind, value.to_lowercase());
        if !self.rosters.contains(&term) {
            self.rosters.push(term);
        }
    }

    /// Whether `entry` satisfies everything except the roster terms, which need
    /// membership lists the caller has to supply separately.
    pub fn matches_entry(&self, entry: &PokemonEntry) -> bool {
        // An alternate form has no dex number and no generation to test, so
        // both filters exclude it rather than guessing which species it
        // belongs to. Only the name match still reaches it.
        let dex = entry.dex_number();

        if !self.text.is_empty() {
            let by_name = entry.name.to_lowercase().contains(&self.text);
            let by_number = self.text_dex.is_some_and(|number| dex == Some(number));
            if !by_name && !by_number {
                return false;
            }
        }
        if !self.dex.is_empty() {
            let Some(dex) = dex else {
                return false;
            };
            if !self.dex.iter().any(|range| range.contains(&dex)) {
                return false;
            }
        }
        if !self.generations.is_empty() {
            let Some(gen) = entry.generation() else {
                return false;
            };
            if !self.generations.contains(&gen) {
                return false;
            }
        }
        true
    }
}

/// Translates the breeding groups' in-game names into the slugs PokeAPI uses
/// for them, which are a layer of history older: the Grass group is `plant`
/// there, Field is `ground`, and so on. Anything without a translation is
/// passed through, so the API's own spelling keeps working.
fn egg_group_slug(value: &str) -> String {
    let lowered = value.to_lowercase();
    match lowered.as_str() {
        "grass" => "plant".to_string(),
        "field" => "ground".to_string(),
        "human-like" | "humanlike" | "human" => "humanshape".to_string(),
        "amorphous" => "indeterminate".to_string(),
        "water-1" => "water1".to_string(),
        "water-2" => "water2".to_string(),
        "water-3" => "water3".to_string(),
        _ => lowered,
    }
}

/// Reads a `dex:` value: a single number (`25`) or an inclusive range
/// (`1-151`).
///
/// A reversed range is read the same way round as a sorted one, since
/// `dex:151-1` can only have meant the first generation's span.
fn parse_dex_range(value: &str) -> Option<RangeInclusive<u32>> {
    match value.split_once('-') {
        Some((start, end)) => {
            let start = start.parse::<u32>().ok()?;
            let end = end.parse::<u32>().ok()?;
            Some(start.min(end)..=start.max(end))
        }
        None => {
            let number = value.parse::<u32>().ok()?;
            Some(number..=number)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `dex` field a single `dex:` term parses to.
    ///
    /// Spelled as a helper because a bare `[1..=151]` reads as a range to
    /// collect rather than an array holding one, and clippy rejects the
    /// ambiguity.
    fn one_range(range: RangeInclusive<u32>) -> Vec<RangeInclusive<u32>> {
        vec![range]
    }

    /// The roster term `type:ghost` parses to, spelled once because several
    /// tests need it.
    fn ghost() -> RosterTerm {
        RosterTerm::new(RosterKind::Type, "ghost")
    }

    /// The two fields a filter reads, and nothing else the list response
    /// carries.
    fn entry(name: &str, id: u32) -> PokemonEntry {
        PokemonEntry {
            name: name.to_string(),
            id,
        }
    }

    #[test]
    fn plain_text_is_lowercased_and_joined() {
        assert_eq!(Query::parse("  MR  Mime ").text, "mr mime");
    }

    #[test]
    fn terms_are_lifted_out_of_the_text() {
        let query = Query::parse("gen:1 type:Ghost ga");
        assert_eq!(query.text, "ga");
        assert_eq!(query.rosters, [ghost()]);
        assert_eq!(query.generations, [1]);
    }

    #[test]
    fn short_forms_are_accepted() {
        let query = Query::parse("t:fire g:2 d:25");
        assert_eq!(query.rosters, [RosterTerm::new(RosterKind::Type, "fire")]);
        assert_eq!(query.generations, [2]);
        assert_eq!(query.dex, one_range(25..=25));
    }

    #[test]
    fn repeated_terms_accumulate() {
        let query = Query::parse("type:water type:flying gen:1 gen:2");
        assert_eq!(
            query.rosters,
            [
                RosterTerm::new(RosterKind::Type, "water"),
                RosterTerm::new(RosterKind::Type, "flying"),
            ]
        );
        assert_eq!(query.generations, [1, 2]);
    }

    #[test]
    fn unrecognised_terms_stay_searchable_text() {
        let query = Query::parse("colour:red");
        assert_eq!(query.text, "colour:red");
        assert!(query.rosters.is_empty());
    }

    #[test]
    fn abilities_and_egg_groups_are_terms_of_their_own() {
        let query = Query::parse("ability:Levitate egg:dragon");
        assert_eq!(
            query.rosters,
            [
                RosterTerm::new(RosterKind::Ability, "levitate"),
                RosterTerm::new(RosterKind::EggGroup, "dragon"),
            ]
        );
        assert!(query.text.is_empty());
    }

    #[test]
    fn ability_and_egg_have_short_forms_too() {
        let query = Query::parse("a:levitate e:dragon");
        assert_eq!(
            query.rosters,
            [
                RosterTerm::new(RosterKind::Ability, "levitate"),
                RosterTerm::new(RosterKind::EggGroup, "dragon"),
            ]
        );
    }

    #[test]
    fn egg_groups_accept_their_in_game_names() {
        let slug = |raw: &str| Query::parse(raw).rosters[0].value.clone();
        assert_eq!(slug("egg:grass"), "plant");
        assert_eq!(slug("egg:Field"), "ground");
        assert_eq!(slug("egg:human-like"), "humanshape");
        assert_eq!(slug("egg:amorphous"), "indeterminate");
        assert_eq!(slug("egg:water-1"), "water1");
        // The API's own spelling still works.
        assert_eq!(slug("egg:monster"), "monster");
    }

    #[test]
    fn a_repeated_roster_term_is_recorded_once() {
        assert_eq!(Query::parse("type:ghost t:Ghost").rosters, [ghost()]);
    }

    #[test]
    fn a_half_typed_roster_term_filters_nothing_extra() {
        assert_eq!(Query::parse("type:"), Query::default());
        assert_eq!(Query::parse("ability:"), Query::default());
        assert_eq!(Query::parse("egg:"), Query::default());
    }

    #[test]
    fn a_half_typed_generation_filters_nothing_extra() {
        assert_eq!(Query::parse("gen:"), Query::default());
    }

    #[test]
    fn a_half_typed_dex_range_filters_nothing_extra() {
        assert_eq!(Query::parse("dex:"), Query::default());
        assert_eq!(Query::parse("dex:1-"), Query::default());
        assert_eq!(Query::parse("dex:eevee"), Query::default());
    }

    #[test]
    fn a_dex_range_is_read_in_either_direction() {
        assert_eq!(Query::parse("dex:1-151").dex, one_range(1..=151));
        assert_eq!(Query::parse("dex:151-1").dex, one_range(1..=151));
    }

    #[test]
    fn an_empty_query_matches_everything() {
        let query = Query::parse("   ");
        assert_eq!(query, Query::default());
        assert!(query.matches_entry(&entry("bulbasaur", 1)));
        assert!(query.matches_entry(&entry("raichu-alola", 10100)));
    }

    #[test]
    fn text_matches_anywhere_in_the_name() {
        let query = Query::parse("saur");
        assert!(query.matches_entry(&entry("bulbasaur", 1)));
        assert!(!query.matches_entry(&entry("pikachu", 25)));
    }

    #[test]
    fn generations_are_matched_as_alternatives() {
        let query = Query::parse("gen:1 gen:3");
        assert!(query.matches_entry(&entry("bulbasaur", 1)));
        assert!(query.matches_entry(&entry("treecko", 252)));
        assert!(!query.matches_entry(&entry("chikorita", 152)));
    }

    #[test]
    fn a_generation_filter_excludes_alternate_forms() {
        let query = Query::parse("gen:7");
        assert!(!query.matches_entry(&entry("raichu-alola", 10100)));
    }

    #[test]
    fn a_bare_number_matches_by_dex_number() {
        let query = Query::parse("25");
        assert!(query.matches_entry(&entry("pikachu", 25)));
        assert!(!query.matches_entry(&entry("bulbasaur", 1)));
    }

    #[test]
    fn a_bare_number_still_matches_a_name_that_contains_it() {
        let query = Query::parse("2");
        assert!(query.matches_entry(&entry("porygon2", 233)));
        assert!(query.matches_entry(&entry("ivysaur", 2)));
        assert!(!query.matches_entry(&entry("pikachu", 25)));
    }

    #[test]
    fn a_dex_term_matches_by_number_alone() {
        let query = Query::parse("dex:2");
        assert!(query.matches_entry(&entry("ivysaur", 2)));
        assert!(!query.matches_entry(&entry("porygon2", 233)));
    }

    #[test]
    fn a_dex_range_matches_its_ends() {
        let query = Query::parse("dex:1-151");
        assert!(query.matches_entry(&entry("bulbasaur", 1)));
        assert!(query.matches_entry(&entry("mew", 151)));
        assert!(!query.matches_entry(&entry("chikorita", 152)));
    }

    #[test]
    fn dex_terms_are_matched_as_alternatives() {
        let query = Query::parse("dex:1-3 dex:25");
        assert!(query.matches_entry(&entry("ivysaur", 2)));
        assert!(query.matches_entry(&entry("pikachu", 25)));
        assert!(!query.matches_entry(&entry("mew", 151)));
    }

    #[test]
    fn a_dex_filter_excludes_alternate_forms() {
        // Consistent with `gen:`: a form's id is not a dex number, so a filter
        // that reads dex numbers cannot speak for it.
        let query = Query::parse("dex:26");
        assert!(!query.matches_entry(&entry("raichu-alola", 10100)));
    }

    #[test]
    fn dex_and_name_terms_combine() {
        let query = Query::parse("dex:1-151 saur");
        assert!(query.matches_entry(&entry("bulbasaur", 1)));
        assert!(!query.matches_entry(&entry("pikachu", 25)));
    }
}
