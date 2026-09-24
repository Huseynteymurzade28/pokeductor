//! `pokeductor --json NAME`: one species, as JSON, for scripts.
//!
//! The output is its own set of types rather than [`PokemonDetail`]
//! serialized as-is. That struct is shaped by the code that reads it — sprite
//! URLs, a full learnset, genus and flavor text for every language, stats as a
//! list keyed by an enum — and it changes whenever the interface needs it to.
//! A shape other programs parse has to change only on purpose, so it is
//! written down here, pinned by a test, and carries a `schema` number that
//! goes up whenever a field is renamed or removed. Adding a field does not
//! bump it; a consumer is expected to ignore keys it does not know.
//!
//! Everything in the output is English. The interface's language is a display
//! preference, and a script comparing `genus` across machines should not get a
//! different answer from each.

use std::io::Write;

use serde::Serialize;

use crate::api;
use crate::cache;
use crate::models::{EvolutionTree, PokemonDetail, PokemonEntry, StatKind};

/// Goes up when a field is renamed, removed or changes type. See the module
/// docs for what does not count.
pub const SCHEMA: u32 = 1;

/// Resolves `name` to one species, prints it and returns. Fetches whatever
/// the cache cannot answer, and caches it, exactly as the TUI would.
pub async fn run(name: &str) -> anyhow::Result<()> {
    let client = api::build_client()?;
    let entries = list(&client).await?;
    let entry = pick(&entries, name)?;
    let (detail, evolution) = record(&client, &entry.name).await?;
    let species = Species::new(&detail, &evolution);
    print(&species)
}

/// Writes the output, treating a reader that stopped early — `| head` — as
/// done rather than as a failure. `println!` panics there, and a script that
/// only wanted the first lines should not see a panic message for it.
fn print(species: &Species) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    let written = serde_json::to_writer_pretty(&mut out, species)
        .map_err(std::io::Error::from)
        .and_then(|()| writeln!(out))
        .and_then(|()| out.flush());
    match written {
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

/// The master list, cache first. A stale list is refreshed, and still used if
/// the refresh fails: a list a month old answers every species but the newest.
async fn list(client: &reqwest::Client) -> anyhow::Result<Vec<PokemonEntry>> {
    let cached = cache::load_list().await;
    if let Some(cached) = &cached {
        if cached.fresh {
            return Ok(cached.entries.clone());
        }
    }
    match api::fetch_pokemon_list(client).await {
        Ok(list) => {
            cache::store_list(&list).await;
            Ok(list)
        }
        Err(err) => match cached {
            Some(cached) => Ok(cached.entries),
            None => Err(err.into()),
        },
    }
}

/// A species' record, cache first, under the same key the TUI files it by.
async fn record(
    client: &reqwest::Client,
    name: &str,
) -> anyhow::Result<(PokemonDetail, EvolutionTree)> {
    if let Some(bundle) = cache::load_bundle(name).await {
        return Ok((bundle.detail, bundle.evolution));
    }
    let (detail, evolution) = api::fetch_record(client, name).await?;
    cache::store_bundle(name, &detail, &evolution).await;
    Ok((detail, evolution))
}

/// Why `NAME` did not come down to one species.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Miss {
    #[error("no species matches {0:?}")]
    Nothing(String),
    #[error("{query:?} matches {count} species ({shown}); name one of them exactly")]
    Several {
        query: String,
        count: usize,
        shown: String,
    },
    #[error(
        "--json takes a species name or Pokedex number, not a search term like {0:?}; \
         filters are for the list in the interface"
    )]
    SearchTerm(String),
}

/// How many candidates an ambiguous name lists before trailing off.
const SHOWN_CANDIDATES: usize = 5;

/// Brings `raw` down to exactly one entry of the master list.
///
/// The TUI puts the name in the search box, where a list of matches is a fine
/// answer. A script needs one species or a clear no, so the rule here is the
/// box's narrowed to that:
///
/// 1. An exact name wins, so `mew` is Mew and not Mewtwo.
/// 2. A bare number is a Pokedex number, and nothing else.
/// 3. Otherwise the name is a substring search, and it has to match exactly
///    one entry. More than one is an error that lists them, rather than a
///    guess the caller cannot see was made.
///
/// Search terms (`type:ghost`) describe a list, not a species, and are
/// refused rather than read as a name that happens to contain a colon.
pub fn pick<'a>(entries: &'a [PokemonEntry], raw: &str) -> Result<&'a PokemonEntry, Miss> {
    let query = raw.trim().to_lowercase();
    if query.contains(':') {
        return Err(Miss::SearchTerm(raw.trim().to_string()));
    }
    if query.is_empty() {
        return Err(Miss::Nothing(raw.to_string()));
    }

    if let Some(exact) = entries.iter().find(|e| e.name == query) {
        return Ok(exact);
    }
    if let Ok(number) = query.parse::<u32>() {
        return entries
            .iter()
            .find(|e| e.dex_number() == Some(number))
            .ok_or(Miss::Nothing(query));
    }

    let matches: Vec<&PokemonEntry> = entries.iter().filter(|e| e.name.contains(&query)).collect();
    match matches.as_slice() {
        [] => Err(Miss::Nothing(query)),
        [only] => Ok(only),
        several => {
            let mut shown: Vec<&str> = several
                .iter()
                .take(SHOWN_CANDIDATES)
                .map(|e| e.name.as_str())
                .collect();
            if several.len() > SHOWN_CANDIDATES {
                shown.push("...");
            }
            Err(Miss::Several {
                query,
                count: several.len(),
                shown: shown.join(", "),
            })
        }
    }
}

/// One species, as `--json` prints it. Field names are the public interface;
/// the README documents them.
#[derive(Debug, Serialize)]
pub struct Species {
    pub schema: u32,
    /// The name it is filed under, which is a form's own name for a form
    /// (`raichu-alola`).
    pub name: String,
    /// The species it belongs to (`raichu`).
    pub species: String,
    pub dex: u32,
    pub genus: Option<String>,
    pub flavor: Option<String>,
    /// In slot order, so the first is the primary type.
    pub types: Vec<String>,
    pub abilities: Vec<Ability>,
    pub stats: Stats,
    pub height_m: f64,
    pub weight_kg: f64,
    pub legendary: bool,
    pub mythical: bool,
    pub baby: bool,
    pub breeding: Breeding,
    /// Every variety of the species, this one included.
    pub forms: Vec<String>,
    /// The whole chain from its root, not just this species' place in it.
    pub evolution: Stage,
}

#[derive(Debug, Serialize)]
pub struct Ability {
    pub name: String,
    pub hidden: bool,
}

/// Base stats by name rather than as a list, so `jq .stats.speed` works.
#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub hp: u16,
    pub attack: u16,
    pub defense: u16,
    pub special_attack: u16,
    pub special_defense: u16,
    pub speed: u16,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct Breeding {
    pub egg_groups: Vec<String>,
    /// Percentages, or `null` for a genderless species.
    pub gender: Option<Gender>,
    pub capture_rate: u8,
    pub base_happiness: Option<u8>,
    pub growth_rate: Option<String>,
    pub habitat: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Gender {
    pub male: f32,
    pub female: f32,
}

/// One stage of an evolution chain. What it takes to evolve is not in schema
/// 1: the conditions are a wide struct of mostly-empty fields, and settling a
/// shape for them is its own piece of work.
#[derive(Debug, Serialize)]
pub struct Stage {
    pub name: String,
    pub evolves_to: Vec<Stage>,
}

impl Species {
    pub fn new(detail: &PokemonDetail, evolution: &EvolutionTree) -> Self {
        let mut stats = Stats {
            total: detail.stat_total(),
            ..Stats::default()
        };
        for stat in &detail.stats {
            let slot = match stat.kind {
                StatKind::Hp => &mut stats.hp,
                StatKind::Attack => &mut stats.attack,
                StatKind::Defense => &mut stats.defense,
                StatKind::SpecialAttack => &mut stats.special_attack,
                StatKind::SpecialDefense => &mut stats.special_defense,
                StatKind::Speed => &mut stats.speed,
            };
            *slot = stat.base;
        }

        let field = &detail.field;
        Species {
            schema: SCHEMA,
            name: detail.name.clone(),
            species: detail.species.clone(),
            dex: detail.dex_number,
            genus: detail.genera.get("en").cloned(),
            flavor: detail.flavors.get("en").cloned(),
            types: detail.types.clone(),
            abilities: detail
                .abilities
                .iter()
                .map(|a| Ability {
                    name: a.name.clone(),
                    hidden: a.is_hidden,
                })
                .collect(),
            stats,
            // PokeAPI counts in decimetres and hectograms.
            height_m: f64::from(detail.height) / 10.0,
            weight_kg: f64::from(detail.weight) / 10.0,
            legendary: detail.is_legendary,
            mythical: detail.is_mythical,
            baby: detail.is_baby,
            breeding: Breeding {
                egg_groups: field.egg_groups.clone(),
                gender: field
                    .gender_split()
                    .map(|(male, female)| Gender { male, female }),
                capture_rate: field.capture_rate,
                base_happiness: field.base_happiness,
                growth_rate: field.growth_rate.clone(),
                habitat: field.habitat.clone(),
            },
            forms: detail.forms.clone(),
            evolution: Stage::new(evolution),
        }
    }
}

impl Stage {
    fn new(tree: &EvolutionTree) -> Self {
        Stage {
            name: tree.name.clone(),
            evolves_to: tree.children.iter().map(Stage::new).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::models::{Ability as DetailAbility, FieldData, Stat};

    fn entry(name: &str, id: u32) -> PokemonEntry {
        PokemonEntry {
            name: name.to_string(),
            id,
        }
    }

    fn roster() -> Vec<PokemonEntry> {
        vec![
            entry("gengar", 94),
            entry("mewtwo", 150),
            entry("mew", 151),
            entry("porygon2", 233),
            entry("raichu-alola", 10100),
        ]
    }

    #[test]
    fn an_exact_name_wins_over_a_longer_one_that_contains_it() {
        assert_eq!(pick(&roster(), "mew").unwrap().name, "mew");
        assert_eq!(pick(&roster(), "  Gengar ").unwrap().name, "gengar");
    }

    #[test]
    fn a_bare_number_is_a_pokedex_number_and_not_a_substring() {
        assert_eq!(pick(&roster(), "94").unwrap().name, "gengar");
        // `2` is in `porygon2`, but as a number it names Ivysaur, which this
        // roster does not have.
        assert_eq!(pick(&roster(), "2").unwrap_err(), Miss::Nothing("2".into()));
        // Alternate forms have no dex number, so their id does not reach them.
        assert!(pick(&roster(), "10100").is_err());
    }

    #[test]
    fn a_fragment_that_names_one_species_resolves_to_it() {
        assert_eq!(pick(&roster(), "alola").unwrap().name, "raichu-alola");
    }

    #[test]
    fn a_fragment_that_names_several_is_an_error_listing_them() {
        let miss = pick(&roster(), "me").unwrap_err();
        assert_eq!(
            miss,
            Miss::Several {
                query: "me".into(),
                count: 2,
                shown: "mewtwo, mew".into(),
            }
        );
    }

    #[test]
    fn a_long_list_of_candidates_trails_off() {
        let many: Vec<PokemonEntry> = (1..=8).map(|n| entry(&format!("pika{n}"), n)).collect();
        let Err(Miss::Several { count, shown, .. }) = pick(&many, "pika") else {
            panic!("eight matches should be ambiguous");
        };
        assert_eq!(count, 8);
        assert_eq!(shown, "pika1, pika2, pika3, pika4, pika5, ...");
    }

    #[test]
    fn a_name_that_matches_nothing_is_a_miss() {
        assert_eq!(
            pick(&roster(), "agumon").unwrap_err(),
            Miss::Nothing("agumon".into())
        );
        assert!(pick(&roster(), "   ").is_err());
    }

    #[test]
    fn a_search_term_is_refused_rather_than_read_as_a_name() {
        assert!(matches!(
            pick(&roster(), "type:ghost"),
            Err(Miss::SearchTerm(_))
        ));
    }

    fn gengar() -> (PokemonDetail, EvolutionTree) {
        let stat = |kind, base| Stat { kind, base };
        let detail = PokemonDetail {
            name: "gengar".into(),
            species: "gengar".into(),
            forms: vec!["gengar".into(), "gengar-mega".into()],
            dex_number: 94,
            is_legendary: false,
            is_mythical: false,
            is_baby: false,
            types: vec!["ghost".into(), "poison".into()],
            abilities: vec![DetailAbility {
                name: "cursed-body".into(),
                is_hidden: false,
            }],
            stats: vec![
                stat(StatKind::Hp, 60),
                stat(StatKind::Attack, 65),
                stat(StatKind::Defense, 60),
                stat(StatKind::SpecialAttack, 130),
                stat(StatKind::SpecialDefense, 75),
                stat(StatKind::Speed, 110),
            ],
            height: 15,
            weight: 405,
            sprite_url: Some("https://example.invalid/94.png".into()),
            shiny_sprite_url: None,
            genera: HashMap::from([
                ("en".into(), "Shadow Pokémon".into()),
                ("de".into(), "Schatten-Pokémon".into()),
            ]),
            flavors: HashMap::from([("en".into(), "It hides in shadows.".into())]),
            moves: Vec::new(),
            learnset_games: Some("scarlet-violet".into()),
            field: FieldData {
                egg_groups: vec!["indeterminate".into()],
                capture_rate: 45,
                base_happiness: Some(50),
                growth_rate: Some("medium-slow".into()),
                gender_rate: 4,
                habitat: Some("cave".into()),
            },
        };
        let stage = |name: &str, children| EvolutionTree {
            name: name.into(),
            condition: None,
            children,
        };
        let chain = stage(
            "gastly",
            vec![stage("haunter", vec![stage("gengar", vec![])])],
        );
        (detail, chain)
    }

    /// The shape is the contract, so it is written out in full rather than
    /// checked a field at a time: a field added, renamed or dropped fails here
    /// and has to be decided on, not discovered by someone's script.
    #[test]
    fn the_output_shape_is_exactly_the_documented_one() {
        let (detail, chain) = gengar();
        let value = serde_json::to_value(Species::new(&detail, &chain)).unwrap();
        assert_eq!(
            value,
            json!({
                "schema": 1,
                "name": "gengar",
                "species": "gengar",
                "dex": 94,
                "genus": "Shadow Pokémon",
                "flavor": "It hides in shadows.",
                "types": ["ghost", "poison"],
                "abilities": [{ "name": "cursed-body", "hidden": false }],
                "stats": {
                    "hp": 60,
                    "attack": 65,
                    "defense": 60,
                    "special_attack": 130,
                    "special_defense": 75,
                    "speed": 110,
                    "total": 500
                },
                "height_m": 1.5,
                "weight_kg": 40.5,
                "legendary": false,
                "mythical": false,
                "baby": false,
                "breeding": {
                    "egg_groups": ["indeterminate"],
                    "gender": { "male": 50.0, "female": 50.0 },
                    "capture_rate": 45,
                    "base_happiness": 50,
                    "growth_rate": "medium-slow",
                    "habitat": "cave"
                },
                "forms": ["gengar", "gengar-mega"],
                "evolution": {
                    "name": "gastly",
                    "evolves_to": [{
                        "name": "haunter",
                        "evolves_to": [{ "name": "gengar", "evolves_to": [] }]
                    }]
                }
            })
        );
    }

    #[test]
    fn a_genderless_species_has_null_gender_rather_than_zeros() {
        let (mut detail, chain) = gengar();
        detail.field.gender_rate = -1;
        let value = serde_json::to_value(Species::new(&detail, &chain)).unwrap();
        assert_eq!(value["breeding"]["gender"], serde_json::Value::Null);
    }
}
