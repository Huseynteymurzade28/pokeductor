//! `pokeductor --json NAME`: one species, as JSON, for scripts. And
//! `--json-list QUERY`: every species a search matches, one per line, in the
//! same shape.
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

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::IsTerminal;

use anyhow::Context;
use futures::StreamExt;
use serde::Serialize;

use crate::api::{self, ApiError};
use crate::browser::Browser;
use crate::cache;
use crate::cli;
use crate::models::{
    EvolutionCondition, EvolutionTree, EvolutionTrigger, PokemonDetail, PokemonEntry, RosterKind,
    RosterTerm, StatKind,
};
use crate::query::Query;
use crate::session;

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

/// Writes the output through [`cli::print`], which ends quietly on a closed
/// pipe.
fn print(species: &Species) -> anyhow::Result<()> {
    let mut out = serde_json::to_vec_pretty(species)?;
    out.push(b'\n');
    cli::print(&out)?;
    Ok(())
}

/// How many species records `--json-list` has in flight at once. The client
/// caps requests process-wide on its own; this only keeps enough records
/// queued that the cap is the limit, without starting a thousand futures for
/// an unfiltered list.
const LIST_AHEAD: usize = 8;

/// Prints every species `raw` matches as JSON Lines, in Pokedex order.
///
/// Each line is written as soon as its record is in and every record before
/// it has been written, so a slow cold-cache run shows progress on stdout
/// itself, and a reader that stops early (`| head -5`) stops the fetching
/// too, which makes a `--limit` flag unnecessary.
pub async fn run_list(raw: &str) -> anyhow::Result<()> {
    let client = api::build_client()?;
    let entries = list(&client).await?;
    let query = Query::parse(raw);

    let mut rosters = HashMap::new();
    for term in &query.rosters {
        rosters.insert(term.clone(), roster(&client, term).await?);
    }
    let favourites = match query.favourites {
        true => session::load().await.favourites.into_iter().collect(),
        false => BTreeSet::new(),
    };
    let matches = matching(&entries, raw, rosters, favourites);

    let mut progress = Progress::new(matches.len());
    let mut records = futures::stream::iter(&matches)
        .map(|entry| async {
            record(&client, &entry.name)
                .await
                .with_context(|| format!("could not fetch {}", entry.name))
        })
        .buffered(LIST_AHEAD);
    while let Some(record) = records.next().await {
        let (detail, evolution) = record?;
        progress.advance();
        let mut line = serde_json::to_vec(&Species::new(&detail, &evolution))?;
        line.push(b'\n');
        if !cli::print(&line)? {
            break;
        }
    }
    Ok(())
}

/// A roster, cache first, the way the sidebar resolves one.
///
/// A name PokeAPI has no roster for (`type:plasma`) is an empty roster, so the
/// query matches nothing and the answer is an empty list. Any other failure is
/// an error: unlike the sidebar, which can show a term as unanswered, a script
/// reading an empty output would take it as the answer.
async fn roster(client: &reqwest::Client, term: &RosterTerm) -> anyhow::Result<HashSet<String>> {
    if let Some(members) = cache::load_roster(term).await {
        return Ok(members.into_iter().collect());
    }
    match api::fetch_roster(client, term).await {
        Ok(members) => {
            cache::store_roster(term, &members).await;
            Ok(members.into_iter().collect())
        }
        Err(ApiError::NotFound(_)) => Ok(HashSet::new()),
        Err(err) => {
            let kind = match term.kind {
                RosterKind::Type => "type",
                RosterKind::Ability => "ability",
                RosterKind::EggGroup => "egg",
            };
            Err(err).with_context(|| format!("could not resolve {kind}:{}", term.value))
        }
    }
}

/// The entries `raw` matches, in Pokedex order, with alternate forms after.
///
/// This is the sidebar's own filter, fed the rosters and favourites it would
/// have, so the command line and the search box cannot disagree about what a
/// query means.
pub fn matching(
    entries: &[PokemonEntry],
    raw: &str,
    rosters: HashMap<RosterTerm, HashSet<String>>,
    favourites: BTreeSet<String>,
) -> Vec<PokemonEntry> {
    let mut browser = Browser {
        all: entries.to_vec(),
        query: raw.to_string(),
        rosters,
        favourites,
        ..Browser::default()
    };
    browser.recompute();
    browser
        .filtered
        .iter()
        .map(|&idx| browser.all[idx].clone())
        .collect()
}

/// A `12/151` counter on stderr while records are fetched.
///
/// Only drawn when stderr is a terminal and stdout is not: with output piped
/// somewhere, the counter is the only sign of life on a cold cache, and with
/// both on one terminal it would be drawn over the lines it is counting. The
/// line is cleared again when the counter is dropped, error or not, so the
/// terminal is left with nothing but what the command printed.
struct Progress {
    done: usize,
    total: usize,
    shown: bool,
}

impl Progress {
    fn new(total: usize) -> Self {
        Progress {
            done: 0,
            total,
            shown: total > 0 && std::io::stderr().is_terminal() && !std::io::stdout().is_terminal(),
        }
    }

    fn advance(&mut self) {
        self.done += 1;
        if self.shown {
            eprint!("\r{}/{}", self.done, self.total);
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if self.shown {
            eprint!("\r\x1b[2K");
        }
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
         --json-list prints every species a search matches"
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

/// One stage of an evolution chain.
#[derive(Debug, Serialize)]
pub struct Stage {
    pub name: String,
    /// What the previous stage takes to become this one. `null` at the root,
    /// which nothing evolves into.
    pub requires: Option<Requires>,
    pub evolves_to: Vec<Stage>,
}

/// What one evolution step takes.
///
/// Not [`EvolutionCondition`] serialized as-is: that is eighteen fields named
/// for the code that reads them, nearly all of them empty on any given step,
/// with PokeAPI's numeric codes for gender and Tyrogue's stat comparison.
/// Here an unset condition is left out rather than written as `null`, so a
/// step reads as what it takes, and the codes are spelled out as words.
#[derive(Debug, Default, Serialize)]
pub struct Requires {
    /// PokeAPI's trigger slug: `level-up`, `use-item`, `trade`, `shed`, or one
    /// of the rarer ones verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_level: Option<u32>,
    /// An item used on it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// An item it holds while the trigger happens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_move: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_move_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_happiness: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_affection: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_beauty: Option<u32>,
    /// `day`, `night` or `dusk`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// `male` or `female`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<&'static str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overworld_rain: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub upside_down: bool,
    /// The species it has to be traded for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_species: Option<String>,
    /// A species that has to be in the party.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party_species: Option<String>,
    /// A type some party member has to have.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party_type: Option<String>,
    /// Attack against Defense, for Tyrogue: `greater`, `equal` or `less`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_vs_defense: Option<&'static str>,
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
            requires: tree.condition.as_ref().map(Requires::new),
            evolves_to: tree.children.iter().map(Stage::new).collect(),
        }
    }
}

impl Requires {
    fn new(condition: &EvolutionCondition) -> Self {
        let c = condition.clone();
        Requires {
            trigger: c.trigger.map(|trigger| match trigger {
                EvolutionTrigger::LevelUp => "level-up".to_string(),
                EvolutionTrigger::Trade => "trade".to_string(),
                EvolutionTrigger::UseItem => "use-item".to_string(),
                EvolutionTrigger::Shed => "shed".to_string(),
                EvolutionTrigger::Other(slug) => slug,
            }),
            min_level: c.min_level,
            item: c.item,
            held_item: c.held_item,
            known_move: c.known_move,
            known_move_type: c.known_move_type,
            min_happiness: c.min_happiness,
            min_affection: c.min_affection,
            min_beauty: c.min_beauty,
            time_of_day: c.time_of_day,
            location: c.location,
            // PokeAPI's gender ids.
            gender: match c.gender {
                Some(1) => Some("female"),
                Some(2) => Some("male"),
                _ => None,
            },
            overworld_rain: c.needs_overworld_rain,
            upside_down: c.turn_upside_down,
            trade_species: c.trade_species,
            party_species: c.party_species,
            party_type: c.party_type,
            attack_vs_defense: c.relative_physical_stats.map(|cmp| match cmp {
                1 => "greater",
                -1 => "less",
                _ => "equal",
            }),
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
        let chain = EvolutionTree {
            name: "gastly".into(),
            condition: None,
            children: vec![step(
                "haunter",
                EvolutionCondition {
                    trigger: Some(EvolutionTrigger::LevelUp),
                    min_level: Some(25),
                    ..EvolutionCondition::default()
                },
                vec![step(
                    "gengar",
                    EvolutionCondition {
                        trigger: Some(EvolutionTrigger::Trade),
                        ..EvolutionCondition::default()
                    },
                    vec![],
                )],
            )],
        };
        (detail, chain)
    }

    fn step(
        name: &str,
        condition: EvolutionCondition,
        children: Vec<EvolutionTree>,
    ) -> EvolutionTree {
        EvolutionTree {
            name: name.into(),
            condition: Some(condition),
            children,
        }
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
                    "requires": null,
                    "evolves_to": [{
                        "name": "haunter",
                        "requires": { "trigger": "level-up", "min_level": 25 },
                        "evolves_to": [{
                            "name": "gengar",
                            "requires": { "trigger": "trade" },
                            "evolves_to": []
                        }]
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

    #[test]
    fn each_branch_of_a_branching_chain_keeps_its_own_requirements() {
        let eevee = EvolutionTree {
            name: "eevee".into(),
            condition: None,
            children: vec![
                step(
                    "vaporeon",
                    EvolutionCondition {
                        trigger: Some(EvolutionTrigger::UseItem),
                        item: Some("water-stone".into()),
                        ..EvolutionCondition::default()
                    },
                    vec![],
                ),
                step(
                    "umbreon",
                    EvolutionCondition {
                        trigger: Some(EvolutionTrigger::LevelUp),
                        min_happiness: Some(160),
                        time_of_day: Some("night".into()),
                        ..EvolutionCondition::default()
                    },
                    vec![],
                ),
            ],
        };
        let value = serde_json::to_value(Stage::new(&eevee)).unwrap();
        assert_eq!(
            value["evolves_to"],
            json!([
                {
                    "name": "vaporeon",
                    "requires": { "trigger": "use-item", "item": "water-stone" },
                    "evolves_to": []
                },
                {
                    "name": "umbreon",
                    "requires": {
                        "trigger": "level-up",
                        "min_happiness": 160,
                        "time_of_day": "night"
                    },
                    "evolves_to": []
                }
            ])
        );
    }

    #[test]
    fn numeric_codes_are_written_out_as_words() {
        let requires = |condition| serde_json::to_value(Requires::new(&condition)).unwrap();
        let hitmonlee = EvolutionCondition {
            trigger: Some(EvolutionTrigger::LevelUp),
            min_level: Some(20),
            relative_physical_stats: Some(1),
            ..EvolutionCondition::default()
        };
        assert_eq!(requires(hitmonlee)["attack_vs_defense"], "greater");
        let froslass = EvolutionCondition {
            trigger: Some(EvolutionTrigger::UseItem),
            item: Some("dawn-stone".into()),
            gender: Some(1),
            ..EvolutionCondition::default()
        };
        assert_eq!(requires(froslass)["gender"], "female");
    }

    #[test]
    fn flags_that_are_off_are_left_out_and_ones_that_are_on_are_true() {
        let sliggoo = EvolutionCondition {
            trigger: Some(EvolutionTrigger::LevelUp),
            min_level: Some(50),
            needs_overworld_rain: true,
            ..EvolutionCondition::default()
        };
        assert_eq!(
            serde_json::to_value(Requires::new(&sliggoo)).unwrap(),
            json!({ "trigger": "level-up", "min_level": 50, "overworld_rain": true })
        );
    }

    #[test]
    fn a_rare_trigger_is_carried_through_as_its_slug() {
        let condition = EvolutionCondition {
            trigger: Some(EvolutionTrigger::Other("three-critical-hits".into())),
            ..EvolutionCondition::default()
        };
        assert_eq!(
            serde_json::to_value(Requires::new(&condition)).unwrap(),
            json!({ "trigger": "three-critical-hits" })
        );
    }

    /// A slice of the master list with forms mixed in out of order, the way
    /// PokeAPI serves them after the numbered species.
    fn kanto_and_friends() -> Vec<PokemonEntry> {
        vec![
            entry("bulbasaur", 1),
            entry("gastly", 92),
            entry("haunter", 93),
            entry("gengar", 94),
            entry("gengar-mega", 10038),
            entry("misdreavus", 200),
            entry("dragonite", 149),
        ]
    }

    fn ghosts() -> (RosterTerm, HashSet<String>) {
        let members = ["gengar-mega", "misdreavus", "gengar", "gastly", "haunter"];
        (
            RosterTerm::new(RosterKind::Type, "ghost"),
            members.iter().map(|m| m.to_string()).collect(),
        )
    }

    fn names(entries: &[PokemonEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn a_roster_term_lists_its_members_in_dex_order_with_forms_last() {
        let found = matching(
            &kanto_and_friends(),
            "type:ghost",
            HashMap::from([ghosts()]),
            BTreeSet::new(),
        );
        assert_eq!(
            names(&found),
            ["gastly", "haunter", "gengar", "misdreavus", "gengar-mega"]
        );
    }

    #[test]
    fn terms_combine_the_way_they_do_in_the_search_box() {
        let found = matching(
            &kanto_and_friends(),
            "type:ghost gen:1 ga",
            HashMap::from([ghosts()]),
            BTreeSet::new(),
        );
        assert_eq!(names(&found), ["gastly", "gengar"]);
        let found = matching(
            &kanto_and_friends(),
            "dex:1-93",
            HashMap::new(),
            BTreeSet::new(),
        );
        assert_eq!(names(&found), ["bulbasaur", "gastly", "haunter"]);
    }

    #[test]
    fn a_type_named_in_another_language_reaches_the_same_roster() {
        let found = matching(
            &kanto_and_friends(),
            "type:geist gen:2",
            HashMap::from([ghosts()]),
            BTreeSet::new(),
        );
        assert_eq!(names(&found), ["misdreavus"]);
    }

    #[test]
    fn fav_lists_the_favourites_it_is_given() {
        let favourites = BTreeSet::from(["dragonite".to_string(), "gastly".to_string()]);
        let found = matching(&kanto_and_friends(), "fav:", HashMap::new(), favourites);
        assert_eq!(names(&found), ["gastly", "dragonite"]);
    }

    #[test]
    fn a_query_that_matches_nothing_is_an_empty_list() {
        let empty = (RosterTerm::new(RosterKind::Type, "plasma"), HashSet::new());
        assert!(matching(
            &kanto_and_friends(),
            "type:plasma",
            HashMap::from([empty]),
            BTreeSet::new()
        )
        .is_empty());
        assert!(matching(
            &kanto_and_friends(),
            "agumon",
            HashMap::new(),
            BTreeSet::new()
        )
        .is_empty());
    }

    /// JSON Lines needs every object on exactly one line, which the flavor
    /// text could break if it were not escaped.
    #[test]
    fn a_list_line_is_one_line_that_parses_on_its_own() {
        let (mut detail, chain) = gengar();
        detail
            .flavors
            .insert("en".into(), "Line one.\nLine two.".into());
        let line = serde_json::to_string(&Species::new(&detail, &chain)).unwrap();
        assert!(!line.contains('\n'));
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["name"], "gengar");
    }
}
