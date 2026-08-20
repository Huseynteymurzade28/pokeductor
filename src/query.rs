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
//! gen:1 gen:2          introduced in either generation
//! gen:1 type:ghost ga  all three, combined
//! ```
//!
//! Anything that is not a recognised term is treated as ordinary text, so a
//! stray colon degrades to a name search instead of an error.

use std::ops::RangeInclusive;

use crate::models::PokemonEntry;

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
    /// Types from `type:` terms, lowercased.
    ///
    /// Combined with AND: a Pokemon must carry *every* one of them. That is
    /// the useful reading, because searching two types is how you look for a
    /// specific dual typing.
    pub types: Vec<String>,
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
                Some(("type" | "t", value)) if !value.is_empty() => {
                    query.types.push(value.to_lowercase());
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

    /// Whether `entry` satisfies everything except the type terms, which need
    /// a roster the caller has to supply separately.
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
        assert_eq!(query.types, ["ghost"]);
        assert_eq!(query.generations, [1]);
    }

    #[test]
    fn short_forms_are_accepted() {
        let query = Query::parse("t:fire g:2 d:25");
        assert_eq!(query.types, ["fire"]);
        assert_eq!(query.generations, [2]);
        assert_eq!(query.dex, one_range(25..=25));
    }

    #[test]
    fn repeated_terms_accumulate() {
        let query = Query::parse("type:water type:flying gen:1 gen:2");
        assert_eq!(query.types, ["water", "flying"]);
        assert_eq!(query.generations, [1, 2]);
    }

    #[test]
    fn unrecognised_terms_stay_searchable_text() {
        let query = Query::parse("colour:red");
        assert_eq!(query.text, "colour:red");
        assert!(query.types.is_empty());
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
