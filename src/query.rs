//! Parsing for the search box.
//!
//! The box takes plain text, and on top of that a couple of `key:value` terms
//! that narrow by something the name cannot express:
//!
//! ```text
//! char                 name contains "char"
//! type:water           every Water Pokemon
//! type:water type:fly  Water *and* Flying — Gyarados, Mantine, ...
//! gen:1 gen:2          introduced in either generation
//! gen:1 type:ghost ga  all three, combined
//! ```
//!
//! Anything that is not a recognised term is treated as ordinary text, so a
//! stray colon degrades to a name search instead of an error.

/// A parsed search query. The default value matches everything.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    /// Free text, lowercased, matched as a substring of the species name.
    pub text: String,
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
                _ => words.push(token),
            }
        }

        query.text = words.join(" ").to_lowercase();
        query
    }

    /// Whether `name` and `generation` satisfy everything except the type
    /// terms, which need a roster the caller has to supply separately.
    pub fn matches_name_and_generation(&self, name: &str, generation: Option<u8>) -> bool {
        if !self.text.is_empty() && !name.to_lowercase().contains(&self.text) {
            return false;
        }
        if !self.generations.is_empty() {
            // An alternate form has no generation to test, so a `gen:` filter
            // excludes it rather than guessing which species it belongs to.
            let Some(gen) = generation else {
                return false;
            };
            if !self.generations.contains(&gen) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let query = Query::parse("t:fire g:2");
        assert_eq!(query.types, ["fire"]);
        assert_eq!(query.generations, [2]);
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
    fn an_empty_query_matches_everything() {
        let query = Query::parse("   ");
        assert_eq!(query, Query::default());
        assert!(query.matches_name_and_generation("bulbasaur", Some(1)));
        assert!(query.matches_name_and_generation("raichu-alola", None));
    }

    #[test]
    fn text_matches_anywhere_in_the_name() {
        let query = Query::parse("saur");
        assert!(query.matches_name_and_generation("bulbasaur", Some(1)));
        assert!(!query.matches_name_and_generation("pikachu", Some(1)));
    }

    #[test]
    fn generations_are_matched_as_alternatives() {
        let query = Query::parse("gen:1 gen:3");
        assert!(query.matches_name_and_generation("bulbasaur", Some(1)));
        assert!(query.matches_name_and_generation("treecko", Some(3)));
        assert!(!query.matches_name_and_generation("chikorita", Some(2)));
    }

    #[test]
    fn a_generation_filter_excludes_alternate_forms() {
        let query = Query::parse("gen:7");
        assert!(!query.matches_name_and_generation("raichu-alola", None));
    }
}
