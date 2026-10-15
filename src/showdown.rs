//! The party as a Pokemon Showdown team, and back.
//!
//! Showdown's plain-text team format is what every team builder, damage
//! calculator and battle simulator reads and writes, so it is the one way a
//! party built here can leave the app, and a team built elsewhere can be
//! analysed here. A set in that format looks like this:
//!
//! ```text
//! Sparky (Raichu-Alola) (M) @ Life Orb
//! Ability: Surge Surfer
//! Tera Type: Electric
//! EVs: 252 SpA / 4 SpD / 252 Spe
//! Timid Nature
//! - Thunderbolt
//! ```
//!
//! The party only knows species, so only the first line of each set matters.
//! Everything below it is read past rather than rejected: a team pasted from a
//! builder should import as it is, not after someone strips it by hand.
//!
//! The other half of the work is names. Showdown spells species the way the
//! games print them (`Mr. Mime`, `Farfetch’d`, `Type: Null`), PokeAPI as URL
//! slugs (`mr-mime`, `farfetchd`, `type-null`), and the two differ on default
//! forms too: Showdown's `Deoxys` is PokeAPI's `deoxys-normal`. Both
//! directions live here, beside the tests that pin them.

use crate::models::{PokemonDetail, PokemonEntry};
use crate::team;

/// Species whose Showdown spelling carries punctuation, an accent or a space
/// that a slug cannot, keyed by the slug. Everything not listed is the slug
/// title-cased hyphen by hyphen, which is right for the rest — `ho-oh` is
/// `Ho-Oh`, `porygon-z` is `Porygon-Z`, `chi-yu` is `Chi-Yu`.
///
/// A form of one of these keeps the spelling and adds its suffix as usual, so
/// `mr-mime-galar` is `Mr. Mime-Galar`.
const SPELLINGS: &[(&str, &str)] = &[
    ("mr-mime", "Mr. Mime"),
    ("mime-jr", "Mime Jr."),
    ("mr-rime", "Mr. Rime"),
    ("farfetchd", "Farfetch’d"),
    ("sirfetchd", "Sirfetch’d"),
    ("type-null", "Type: Null"),
    ("flabebe", "Flabébé"),
    // Title-casing would give `Jangmo-O`; the games keep the second half low.
    ("jangmo-o", "Jangmo-o"),
    ("hakamo-o", "Hakamo-o"),
    ("kommo-o", "Kommo-o"),
    ("tapu-koko", "Tapu Koko"),
    ("tapu-lele", "Tapu Lele"),
    ("tapu-bulu", "Tapu Bulu"),
    ("tapu-fini", "Tapu Fini"),
    ("great-tusk", "Great Tusk"),
    ("scream-tail", "Scream Tail"),
    ("brute-bonnet", "Brute Bonnet"),
    ("flutter-mane", "Flutter Mane"),
    ("slither-wing", "Slither Wing"),
    ("sandy-shocks", "Sandy Shocks"),
    ("roaring-moon", "Roaring Moon"),
    ("walking-wake", "Walking Wake"),
    ("gouging-fire", "Gouging Fire"),
    ("raging-bolt", "Raging Bolt"),
    ("iron-treads", "Iron Treads"),
    ("iron-bundle", "Iron Bundle"),
    ("iron-hands", "Iron Hands"),
    ("iron-jugulis", "Iron Jugulis"),
    ("iron-moth", "Iron Moth"),
    ("iron-thorns", "Iron Thorns"),
    ("iron-valiant", "Iron Valiant"),
    ("iron-leaves", "Iron Leaves"),
    ("iron-boulder", "Iron Boulder"),
    ("iron-crown", "Iron Crown"),
];

/// The name a party member is exported under, as a slug: the species for a
/// default form, the form's own name otherwise.
///
/// PokeAPI files some default forms under a suffixed name (`deoxys-normal`,
/// `giratina-altered`, `mimikyu-disguised`) where Showdown names the species
/// alone, and would not recognise the suffix. A default form is the first of
/// its species' varieties, so it is told apart by that rather than by a list
/// of suffixes that would need a new entry every generation.
pub fn export_slug(detail: &PokemonDetail) -> &str {
    match detail.forms.first() {
        Some(first) if *first == detail.name => &detail.species,
        _ => &detail.name,
    }
}

/// A slug as Showdown spells it: `raichu-alola` as `Raichu-Alola`,
/// `mr-mime` as `Mr. Mime`, `indeedee-female` as `Indeedee-F`.
pub fn showdown_name(slug: &str) -> String {
    let (head, rest) = SPELLINGS
        .iter()
        .find_map(|(key, spelled)| match slug.strip_prefix(key) {
            Some("") => Some((spelled.to_string(), "")),
            Some(rest) if rest.starts_with('-') => Some((spelled.to_string(), rest)),
            _ => None,
        })
        .unwrap_or_else(|| (String::new(), slug));
    // Showdown shortens the gendered forms to a letter, as it does Nidoran's.
    let rest = match rest.strip_suffix("-female") {
        Some(stem) => format!("{stem}-f"),
        None => rest.to_string(),
    };
    let tail = rest
        .split('-')
        .filter(|part| !part.is_empty())
        .map(capitalise)
        .collect::<Vec<_>>()
        .join("-");
    match (head.is_empty(), tail.is_empty()) {
        (true, _) => tail,
        (false, true) => head,
        (false, false) => format!("{head}-{tail}"),
    }
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The party as a Showdown team: one set per member, each only its species
/// line, separated by blank lines as Showdown writes them. A set with nothing
/// but a species is a valid one; the builder fills in the rest.
pub fn export(slugs: &[&str]) -> String {
    slugs
        .iter()
        .map(|slug| format!("{}\n", showdown_name(slug)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Showdown species name reduced to the shape PokeAPI slugs take: lower
/// case, hyphens for spaces, and the punctuation the games print dropped.
/// `Mr. Mime` → `mr-mime`, `Type: Null` → `type-null`, `Zygarde-10%` →
/// `zygarde-10`, `Nidoran♀` → `nidoran-f`.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars().flat_map(char::to_lowercase) {
        match ch {
            'é' => out.push('e'),
            '♀' => out.push_str("-f"),
            '♂' => out.push_str("-m"),
            '.' | '’' | '\'' | ':' | '%' => {}
            ch if ch.is_whitespace() || ch == '-' => {
                if !out.is_empty() && !out.ends_with('-') {
                    out.push('-');
                }
            }
            ch => out.push(ch),
        }
    }
    out.trim_end_matches('-').to_string()
}

/// One set's species line, as written, with where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    /// 1-based, for error messages that point into the file.
    pub line: usize,
    pub species: String,
}

/// Finds the species line of every set in a Showdown team.
///
/// A set starts at the first line after a blank one (or at the top), and the
/// lines under it — ability, item, EVs, moves — belong to it and are passed
/// over. `=== [gen9ou] Name ===` headers, which the teambuilder's backup
/// format puts between teams, are skipped the same way.
pub fn parse(text: &str) -> Vec<Set> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut sets = Vec::new();
    let mut at_start = true;
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("===") {
            at_start = true;
            continue;
        }
        if at_start {
            sets.push(Set {
                line: idx + 1,
                species: species_of(line).to_string(),
            });
            at_start = false;
        }
    }
    sets
}

/// The species out of a set's first line, which can be any of:
///
/// ```text
/// Pikachu
/// Pikachu @ Light Ball
/// Pikachu (F) @ Light Ball
/// Sparky (Pikachu) (F) @ Light Ball
/// ```
///
/// The item goes first, then a gender marker, and what is left is either the
/// species or a nickname with the species in brackets after it.
fn species_of(line: &str) -> &str {
    let line = line
        .split_once('@')
        .map_or(line, |(before, _)| before)
        .trim();
    let line = line
        .strip_suffix("(M)")
        .or_else(|| line.strip_suffix("(F)"))
        .map_or(line, str::trim_end);
    match (line.rfind('('), line.strip_suffix(')')) {
        (Some(open), Some(inner)) if open > 0 => inner[open + 1..].trim(),
        _ => line,
    }
}

/// Why a team could not become the party. Each points at the line to fix.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("no Pokemon found; expected a team in Showdown's text format")]
    Empty,
    #[error("line {line}: no species is called {name:?}")]
    Unknown { line: usize, name: String },
    #[error("line {line}: {name} is already in the party")]
    Repeat { line: usize, name: String },
    #[error("line {line}: a party holds at most {} Pokemon", team::MAX_MEMBERS)]
    TooMany { line: usize },
}

/// The party a team describes, as names from the master list, in team order.
///
/// Every set has to resolve: dropping one quietly would leave a party that
/// looks imported and is not the team that was pasted. A repeat is refused for
/// the same reason — the party holds each species once, and silently keeping
/// five of six is worse than saying which line is the problem.
pub fn resolve(sets: &[Set], entries: &[PokemonEntry]) -> Result<Vec<String>, ImportError> {
    if sets.is_empty() {
        return Err(ImportError::Empty);
    }
    let mut party: Vec<String> = Vec::with_capacity(sets.len());
    for set in sets {
        if party.len() == team::MAX_MEMBERS {
            return Err(ImportError::TooMany { line: set.line });
        }
        let name = lookup(&slug(&set.species), entries).ok_or_else(|| ImportError::Unknown {
            line: set.line,
            name: set.species.clone(),
        })?;
        if party.contains(&name) {
            return Err(ImportError::Repeat {
                line: set.line,
                name: set.species.clone(),
            });
        }
        party.push(name);
    }
    Ok(party)
}

/// The master-list name a slug stands for, if any.
///
/// An exact name wins. Then the letter Showdown shortens a gendered form to
/// (`indeedee-f`) is spelled out as PokeAPI has it. Last, a bare species
/// (`deoxys`) is matched to the one default form filed under a suffix
/// (`deoxys-normal`); "one" because `nidoran` is the start of two species and
/// names neither.
fn lookup(slug: &str, entries: &[PokemonEntry]) -> Option<String> {
    let exact = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.name.clone())
    };
    if slug.is_empty() {
        return None;
    }
    if let Some(found) = exact(slug) {
        return Some(found);
    }
    let gendered = match (slug.strip_suffix("-f"), slug.strip_suffix("-m")) {
        (Some(stem), _) => exact(&format!("{stem}-female")),
        (_, Some(stem)) => exact(&format!("{stem}-male")),
        _ => None,
    };
    if gendered.is_some() {
        return gendered;
    }
    let prefix = format!("{slug}-");
    let mut defaults = entries
        .iter()
        .filter(|e| e.dex_number().is_some() && e.name.starts_with(&prefix));
    match (defaults.next(), defaults.next()) {
        (Some(only), None) => Some(only.name.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, id: u32) -> PokemonEntry {
        PokemonEntry {
            name: name.to_string(),
            id,
        }
    }

    /// A slice of the real master list, ids included, covering every naming
    /// rule above.
    fn entries() -> Vec<PokemonEntry> {
        [
            ("nidoran-f", 29),
            ("nidoran-m", 32),
            ("raichu", 26),
            ("mr-mime", 122),
            ("farfetchd", 83),
            ("porygon-z", 474),
            ("porygon", 137),
            ("deoxys-normal", 386),
            ("deoxys-attack", 10001),
            ("indeedee-male", 876),
            ("indeedee-female", 10186),
            ("type-null", 772),
            ("tapu-koko", 785),
            ("flabebe", 669),
            ("zygarde-50", 718),
            ("zygarde-10", 10181),
            ("iron-hands", 992),
            ("gengar", 94),
            ("garchomp", 445),
            ("raichu-alola", 10100),
            ("mr-mime-galar", 10168),
            ("kommo-o", 784),
            ("ho-oh", 250),
        ]
        .into_iter()
        .map(|(name, id)| entry(name, id))
        .collect()
    }

    fn import(text: &str) -> Result<Vec<String>, ImportError> {
        resolve(&parse(text), &entries())
    }

    #[test]
    fn plain_slugs_are_title_cased_hyphen_by_hyphen() {
        assert_eq!(showdown_name("gengar"), "Gengar");
        assert_eq!(showdown_name("raichu-alola"), "Raichu-Alola");
        assert_eq!(showdown_name("ho-oh"), "Ho-Oh");
        assert_eq!(showdown_name("porygon-z"), "Porygon-Z");
        assert_eq!(showdown_name("nidoran-f"), "Nidoran-F");
        assert_eq!(showdown_name("charizard-mega-x"), "Charizard-Mega-X");
    }

    #[test]
    fn names_the_games_punctuate_are_spelled_as_showdown_spells_them() {
        assert_eq!(showdown_name("mr-mime"), "Mr. Mime");
        assert_eq!(showdown_name("farfetchd"), "Farfetch’d");
        assert_eq!(showdown_name("type-null"), "Type: Null");
        assert_eq!(showdown_name("flabebe"), "Flabébé");
        assert_eq!(showdown_name("tapu-koko"), "Tapu Koko");
        assert_eq!(showdown_name("kommo-o"), "Kommo-o");
        assert_eq!(showdown_name("iron-hands"), "Iron Hands");
    }

    #[test]
    fn a_form_of_a_punctuated_name_keeps_the_spelling_and_adds_its_suffix() {
        assert_eq!(showdown_name("mr-mime-galar"), "Mr. Mime-Galar");
        // A key is only a prefix at a hyphen: `mr-mimes` is not Mr. Mime.
        assert_eq!(showdown_name("mr-mimes"), "Mr-Mimes");
    }

    #[test]
    fn gendered_forms_are_shortened_to_a_letter() {
        assert_eq!(showdown_name("indeedee-female"), "Indeedee-F");
    }

    #[test]
    fn showdown_names_reduce_to_slugs() {
        assert_eq!(slug("Mr. Mime"), "mr-mime");
        assert_eq!(slug("Mime Jr."), "mime-jr");
        assert_eq!(slug("Farfetch’d"), "farfetchd");
        assert_eq!(slug("Farfetch'd"), "farfetchd");
        assert_eq!(slug("Type: Null"), "type-null");
        assert_eq!(slug("Flabébé"), "flabebe");
        assert_eq!(slug("Zygarde-10%"), "zygarde-10");
        assert_eq!(slug("Nidoran♀"), "nidoran-f");
        assert_eq!(slug("  Tapu   Koko "), "tapu-koko");
    }

    #[test]
    fn a_set_line_gives_up_its_species_under_any_decoration() {
        assert_eq!(species_of("Pikachu"), "Pikachu");
        assert_eq!(species_of("Pikachu @ Light Ball"), "Pikachu");
        assert_eq!(species_of("Pikachu (F) @ Light Ball"), "Pikachu");
        assert_eq!(species_of("Pikachu (M)"), "Pikachu");
        assert_eq!(species_of("Sparky (Pikachu) (F) @ Light Ball"), "Pikachu");
        assert_eq!(species_of("Sparky (Raichu-Alola)"), "Raichu-Alola");
        assert_eq!(species_of("Mr. Mime @ Focus Sash"), "Mr. Mime");
    }

    #[test]
    fn everything_under_a_species_line_is_read_past() {
        let team = "\
Sparky (Raichu-Alola) (M) @ Life Orb
Ability: Surge Surfer
Tera Type: Electric
EVs: 252 SpA / 4 SpD / 252 Spe
Timid Nature
IVs: 0 Atk
- Thunderbolt
- Psychic

Garchomp @ Choice Scarf
Ability: Rough Skin
- Earthquake
";
        let sets = parse(team);
        assert_eq!(
            sets,
            [
                Set {
                    line: 1,
                    species: "Raichu-Alola".into()
                },
                Set {
                    line: 10,
                    species: "Garchomp".into()
                },
            ]
        );
        assert_eq!(
            import(team).unwrap(),
            ["raichu-alola".to_string(), "garchomp".into()]
        );
    }

    #[test]
    fn extra_blank_lines_headers_and_windows_line_endings_are_tolerated() {
        let team =
            "\u{feff}=== [gen9ou] Rain ===\r\n\r\n\r\nGengar\r\n- Shadow Ball\r\n\r\n\r\nHo-Oh\r\n";
        assert_eq!(import(team).unwrap(), ["gengar", "ho-oh"]);
    }

    #[test]
    fn default_forms_filed_under_a_suffix_are_found_by_their_species() {
        assert_eq!(import("Deoxys").unwrap(), ["deoxys-normal"]);
        assert_eq!(import("Deoxys-Attack").unwrap(), ["deoxys-attack"]);
        assert_eq!(import("Zygarde").unwrap(), ["zygarde-50"]);
        assert_eq!(import("Zygarde-10%").unwrap(), ["zygarde-10"]);
    }

    #[test]
    fn a_prefix_that_starts_two_species_names_neither() {
        assert!(matches!(
            import("Nidoran"),
            Err(ImportError::Unknown { line: 1, .. })
        ));
    }

    #[test]
    fn a_shortened_gender_finds_the_spelled_out_form() {
        assert_eq!(import("Indeedee-F").unwrap(), ["indeedee-female"]);
        assert_eq!(import("Indeedee-M").unwrap(), ["indeedee-male"]);
        // A species that is itself spelled with the letter is found as it is.
        assert_eq!(import("Nidoran-F").unwrap(), ["nidoran-f"]);
    }

    #[test]
    fn an_unknown_species_is_an_error_naming_its_line() {
        let err = import("Gengar\n\nMissingno\n").unwrap_err();
        assert_eq!(
            err,
            ImportError::Unknown {
                line: 3,
                name: "Missingno".into()
            }
        );
        assert_eq!(
            err.to_string(),
            "line 3: no species is called \"Missingno\""
        );
    }

    #[test]
    fn a_seventh_set_is_an_error_naming_its_line() {
        let team = [
            "Gengar",
            "Garchomp",
            "Raichu",
            "Porygon",
            "Ho-Oh",
            "Mr. Mime",
            "Farfetch’d",
        ]
        .join("\n\n");
        assert_eq!(import(&team), Err(ImportError::TooMany { line: 13 }));
    }

    #[test]
    fn a_repeat_is_an_error_rather_than_a_shorter_party() {
        assert_eq!(
            import("Gengar\n\nGarchomp\n\nGengar @ Focus Sash"),
            Err(ImportError::Repeat {
                line: 5,
                name: "Gengar".into()
            })
        );
    }

    #[test]
    fn a_file_with_no_sets_is_an_error_rather_than_an_empty_party() {
        assert_eq!(import(""), Err(ImportError::Empty));
        assert_eq!(
            import("\n\n=== [gen9] Empty ===\n"),
            Err(ImportError::Empty)
        );
    }

    #[test]
    fn an_exported_party_imports_as_the_same_party() {
        // As `export_slug` hands them over: species for default forms, the
        // form's name otherwise.
        let party = [
            ("mr-mime", "mr-mime"),
            ("deoxys-normal", "deoxys"),
            ("indeedee-female", "indeedee-female"),
            ("raichu-alola", "raichu-alola"),
            ("type-null", "type-null"),
            ("iron-hands", "iron-hands"),
        ];
        let slugs: Vec<&str> = party.iter().map(|(_, slug)| *slug).collect();
        let text = export(&slugs);
        let expected: Vec<&str> = party.iter().map(|(name, _)| *name).collect();
        assert_eq!(import(&text).unwrap(), expected);
    }

    #[test]
    fn an_export_is_one_species_line_per_set_separated_by_blank_lines() {
        assert_eq!(export(&["gengar", "mr-mime"]), "Gengar\n\nMr. Mime\n");
        assert_eq!(export(&[]), "");
    }
}
