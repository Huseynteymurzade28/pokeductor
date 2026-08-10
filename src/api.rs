//! PokeAPI client.
//!
//! All network access lives here. Functions take a shared [`reqwest::Client`]
//! and return clean domain types from `models.rs`. Raw wire-format structs are
//! kept private so the JSON shape never leaks out of this module.

use std::collections::HashMap;

use crate::models::{
    EvolutionCondition, EvolutionTree, EvolutionTrigger, PokemonDetail, PokemonEntry, Sprite, Stat,
    StatKind,
};

const BASE_URL: &str = "https://pokeapi.co/api/v2";
/// How many Pokemon to load into the sidebar. Covers all current species.
const LIST_LIMIT: u32 = 1302;

/// Errors that can occur while talking to PokeAPI.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("could not locate the evolution chain for this Pokemon")]
    MissingEvolutionChain,
    #[error("could not decode sprite image: {0}")]
    Image(#[from] image::ImageError),
}

/// Builds the shared HTTP client used for every request in the session.
pub fn build_client() -> Result<reqwest::Client, ApiError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("pokeductor/", env!("CARGO_PKG_VERSION")))
        .build()?;
    Ok(client)
}

/// Fetches the master list of Pokemon names for the sidebar.
pub async fn fetch_pokemon_list(
    client: &reqwest::Client,
) -> Result<Vec<PokemonEntry>, ApiError> {
    let url = format!("{BASE_URL}/pokemon?limit={LIST_LIMIT}&offset=0");
    let raw: NamedList = client.get(url).send().await?.error_for_status()?.json().await?;
    let entries = raw
        .results
        .into_iter()
        .map(|r| PokemonEntry { name: r.name })
        .collect();
    Ok(entries)
}

/// Fetches everything needed to display a Pokemon: its details and its parsed
/// evolution tree. Performed as one logical unit so the UI receives a complete
/// payload in a single message.
pub async fn fetch_pokemon_bundle(
    client: &reqwest::Client,
    name: &str,
) -> Result<(PokemonDetail, EvolutionTree, Option<Sprite>), ApiError> {
    let mut detail = fetch_detail(client, name).await?;

    // The species record carries both the evolution chain *and* the Pokedex
    // blurb shown on the info card, so we fetch it once and read both out. We
    // key it on the *base species* (not `detail.name`) so alternate forms like
    // `raichu-alola` resolve instead of 404-ing.
    let species = fetch_species(client, &detail.species).await?;
    detail.dex_number = species.dex_number;
    detail.is_legendary = species.is_legendary;
    detail.is_mythical = species.is_mythical;
    detail.is_baby = species.is_baby;
    detail.genera = species.genera;
    detail.flavors = species.flavors;
    let evolution = fetch_chain(client, &species.chain_url).await?;

    // The sprite is a nice-to-have: a missing or undecodable image must not
    // sink the whole bundle, so failures here degrade to "no sprite".
    let sprite = match &detail.sprite_url {
        Some(url) => fetch_sprite(client, url).await.ok(),
        None => None,
    };

    Ok((detail, evolution, sprite))
}

/// Translates `text` from one language to another via MyMemory's free,
/// key-less endpoint. Used to fill in flavor text for UI languages PokeAPI has
/// no native entry for (e.g. Turkish). Best-effort: callers fall back to the
/// English original if this errors or the service is rate-limited.
pub async fn translate_text(
    client: &reqwest::Client,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, ApiError> {
    // MyMemory caps anonymous requests at ~500 characters; flavor blurbs are
    // well under that, but clamp defensively just in case.
    let clamped: String = text.chars().take(500).collect();
    let pair = format!("{from}|{to}");
    let resp: MyMemoryResponse = client
        .get("https://api.mymemory.translated.net/get")
        .query(&[("q", clamped.as_str()), ("langpair", pair.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(resp.response_data.translated_text)
}

/// Fetches and decodes just the sprite for a named species. Used to lazily
/// populate the evolution overlay, where every member of the chain needs art.
pub async fn fetch_named_sprite(
    client: &reqwest::Client,
    name: &str,
) -> Result<Option<Sprite>, ApiError> {
    let detail = fetch_detail(client, name).await?;
    match detail.sprite_url {
        Some(url) => Ok(Some(fetch_sprite(client, &url).await?)),
        None => Ok(None),
    }
}

/// Downloads a sprite PNG and decodes it into raw RGBA pixels.
async fn fetch_sprite(client: &reqwest::Client, url: &str) -> Result<Sprite, ApiError> {
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    let image = image::load_from_memory(&bytes)?.to_rgba8();
    let (width, height) = image.dimensions();
    let pixels = image.pixels().map(|p| p.0).collect();
    Ok(Sprite { width, height, pixels })
}

async fn fetch_detail(client: &reqwest::Client, name: &str) -> Result<PokemonDetail, ApiError> {
    let url = format!("{BASE_URL}/pokemon/{name}");
    let raw: RawPokemon = client.get(url).send().await?.error_for_status()?.json().await?;

    let mut types: Vec<(u8, String)> = raw
        .types
        .into_iter()
        .map(|t| (t.slot, t.type_.name))
        .collect();
    types.sort_by_key(|(slot, _)| *slot);

    let mut stats: Vec<Stat> = raw
        .stats
        .into_iter()
        .filter_map(|s| {
            StatKind::from_api(&s.stat.name).map(|kind| Stat {
                kind,
                base: s.base_stat,
            })
        })
        .collect();
    stats.sort_by_key(|s| s.kind.order());

    Ok(PokemonDetail {
        name: raw.name,
        species: raw.species.name,
        // Sensible fallback for the default form; overwritten with the true
        // national number once the species record loads.
        dex_number: raw.id,
        is_legendary: false,
        is_mythical: false,
        is_baby: false,
        types: types.into_iter().map(|(_, name)| name).collect(),
        stats,
        height: raw.height,
        weight: raw.weight,
        sprite_url: raw.sprites.front_default,
        genera: HashMap::new(),
        flavors: HashMap::new(),
    })
}

/// Languages we keep info-card text for (matching the UI's selectable set, plus
/// English as the universal fallback). PokeAPI has no Turkish text, so `tr` is
/// intentionally absent and Turkish falls back to English at render time.
const CARD_LANGS: [&str; 5] = ["en", "de", "fr", "es", "it"];

/// The slice of species data the rest of the app cares about.
struct SpeciesInfo {
    chain_url: String,
    dex_number: u32,
    is_legendary: bool,
    is_mythical: bool,
    is_baby: bool,
    genera: HashMap<String, String>,
    flavors: HashMap<String, String>,
}

/// Fetches a species record, pulling out the evolution-chain URL plus the genus
/// and flavor text in every language we care about for the info card.
async fn fetch_species(client: &reqwest::Client, name: &str) -> Result<SpeciesInfo, ApiError> {
    let url = format!("{BASE_URL}/pokemon-species/{name}");
    let species: RawSpecies = client.get(url).send().await?.error_for_status()?.json().await?;

    let chain_url = species
        .evolution_chain
        .map(|c| c.url)
        .ok_or(ApiError::MissingEvolutionChain)?;

    let mut genera = HashMap::new();
    for g in &species.genera {
        if CARD_LANGS.contains(&g.language.name.as_str()) {
            genera.entry(g.language.name.clone()).or_insert_with(|| g.genus.clone());
        }
    }

    // A species lists one flavor entry per game version per language; the first
    // we see for each language is a fine representative.
    let mut flavors = HashMap::new();
    for e in &species.flavor_text_entries {
        if CARD_LANGS.contains(&e.language.name.as_str()) {
            flavors
                .entry(e.language.name.clone())
                .or_insert_with(|| clean_flavor(&e.flavor_text));
        }
    }

    Ok(SpeciesInfo {
        chain_url,
        dex_number: species.id,
        is_legendary: species.is_legendary,
        is_mythical: species.is_mythical,
        is_baby: species.is_baby,
        genera,
        flavors,
    })
}

/// Fetches and parses an evolution chain from its API URL.
async fn fetch_chain(client: &reqwest::Client, url: &str) -> Result<EvolutionTree, ApiError> {
    let chain: RawEvolutionChain = client.get(url).send().await?.error_for_status()?.json().await?;
    Ok(parse_chain(&chain.chain))
}

/// PokeAPI flavor text is wrapped to a fixed width with hard newlines and stray
/// form-feed characters; collapse all whitespace runs into single spaces.
fn clean_flavor(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Recursively converts PokeAPI's nested `ChainLink` into our [`EvolutionTree`].
fn parse_chain(link: &RawChainLink) -> EvolutionTree {
    EvolutionTree {
        name: link.species.name.clone(),
        condition: link.evolution_details.first().map(parse_condition),
        children: link.evolves_to.iter().map(parse_chain).collect(),
    }
}

/// Converts one raw `evolution_details` entry into an [`EvolutionCondition`].
///
/// A species can list several entries when the games offer more than one route
/// to the same evolution (different items across generations, for instance).
/// [`parse_chain`] surfaces only the first, which is the route the current
/// games use; showing all of them would swamp the panel.
fn parse_condition(raw: &RawEvolutionDetail) -> EvolutionCondition {
    let name_of = |r: &Option<NamedResource>| r.as_ref().map(|n| n.name.clone());
    EvolutionCondition {
        trigger: raw
            .trigger
            .as_ref()
            .map(|t| EvolutionTrigger::from_api(&t.name)),
        min_level: raw.min_level,
        item: name_of(&raw.item),
        held_item: name_of(&raw.held_item),
        known_move: name_of(&raw.known_move),
        known_move_type: name_of(&raw.known_move_type),
        min_happiness: raw.min_happiness,
        min_affection: raw.min_affection,
        min_beauty: raw.min_beauty,
        // The API uses an empty string rather than null for "any time of day".
        time_of_day: (!raw.time_of_day.is_empty()).then(|| raw.time_of_day.clone()),
        location: name_of(&raw.location),
        gender: raw.gender,
        needs_overworld_rain: raw.needs_overworld_rain,
        turn_upside_down: raw.turn_upside_down,
        trade_species: name_of(&raw.trade_species),
        party_species: name_of(&raw.party_species),
        party_type: name_of(&raw.party_type),
        relative_physical_stats: raw.relative_physical_stats,
    }
}

// --- Raw wire-format types (private) -------------------------------------

#[derive(serde::Deserialize)]
struct NamedList {
    results: Vec<NamedResource>,
}

#[derive(serde::Deserialize)]
struct NamedResource {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    url: String,
}

#[derive(serde::Deserialize)]
struct RawPokemon {
    id: u32,
    name: String,
    height: u32,
    weight: u32,
    types: Vec<RawTypeSlot>,
    stats: Vec<RawStatSlot>,
    sprites: RawSprites,
    species: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawSprites {
    #[serde(default)]
    front_default: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawTypeSlot {
    slot: u8,
    #[serde(rename = "type")]
    type_: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawStatSlot {
    base_stat: u16,
    stat: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawSpecies {
    id: u32,
    #[serde(default)]
    is_legendary: bool,
    #[serde(default)]
    is_mythical: bool,
    #[serde(default)]
    is_baby: bool,
    evolution_chain: Option<RawChainRef>,
    #[serde(default)]
    genera: Vec<RawGenus>,
    #[serde(default)]
    flavor_text_entries: Vec<RawFlavorText>,
}

#[derive(serde::Deserialize)]
struct RawGenus {
    genus: String,
    language: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawFlavorText {
    flavor_text: String,
    language: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawChainRef {
    url: String,
}

#[derive(serde::Deserialize)]
struct RawEvolutionChain {
    chain: RawChainLink,
}

#[derive(serde::Deserialize)]
struct RawChainLink {
    species: NamedResource,
    #[serde(default)]
    evolution_details: Vec<RawEvolutionDetail>,
    evolves_to: Vec<RawChainLink>,
}

/// One way a species can evolve. PokeAPI sends every field on every entry, but
/// `serde(default)` keeps us resilient to the payload growing or shrinking.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct RawEvolutionDetail {
    trigger: Option<NamedResource>,
    min_level: Option<u32>,
    item: Option<NamedResource>,
    held_item: Option<NamedResource>,
    known_move: Option<NamedResource>,
    known_move_type: Option<NamedResource>,
    min_happiness: Option<u32>,
    min_affection: Option<u32>,
    min_beauty: Option<u32>,
    time_of_day: String,
    location: Option<NamedResource>,
    gender: Option<u8>,
    needs_overworld_rain: bool,
    turn_upside_down: bool,
    trade_species: Option<NamedResource>,
    party_species: Option<NamedResource>,
    party_type: Option<NamedResource>,
    relative_physical_stats: Option<i8>,
}

#[derive(serde::Deserialize)]
struct MyMemoryResponse {
    #[serde(rename = "responseData")]
    response_data: MyMemoryData,
}

#[derive(serde::Deserialize)]
struct MyMemoryData {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed `/evolution-chain` payload in the exact shape PokeAPI sends:
    /// Eevee branching into an item evolution and a conditional one.
    const EEVEE_CHAIN: &str = r#"{
      "chain": {
        "species": { "name": "eevee", "url": "" },
        "evolution_details": [],
        "evolves_to": [
          {
            "species": { "name": "vaporeon", "url": "" },
            "evolution_details": [{
              "trigger": { "name": "use-item", "url": "" },
              "item": { "name": "water-stone", "url": "" },
              "min_level": null,
              "time_of_day": "",
              "needs_overworld_rain": false,
              "turn_upside_down": false
            }],
            "evolves_to": []
          },
          {
            "species": { "name": "umbreon", "url": "" },
            "evolution_details": [{
              "trigger": { "name": "level-up", "url": "" },
              "min_happiness": 160,
              "time_of_day": "night",
              "needs_overworld_rain": false,
              "turn_upside_down": false
            }],
            "evolves_to": []
          }
        ]
      }
    }"#;

    fn eevee() -> EvolutionTree {
        let raw: RawEvolutionChain = serde_json::from_str(EEVEE_CHAIN).unwrap();
        parse_chain(&raw.chain)
    }

    #[test]
    fn chain_root_has_no_condition() {
        // Nothing evolves *into* Eevee, so there is no requirement to show.
        assert!(eevee().condition.is_none());
    }

    #[test]
    fn item_evolutions_carry_their_item() {
        let tree = eevee();
        let vaporeon = tree.find("vaporeon").unwrap();
        let condition = vaporeon.condition.as_ref().unwrap();
        assert_eq!(condition.trigger, Some(EvolutionTrigger::UseItem));
        assert_eq!(condition.item.as_deref(), Some("water-stone"));
        assert_eq!(condition.min_level, None);
    }

    #[test]
    fn layered_conditions_are_all_kept() {
        let tree = eevee();
        let umbreon = tree.find("umbreon").unwrap();
        let condition = umbreon.condition.as_ref().unwrap();
        assert_eq!(condition.trigger, Some(EvolutionTrigger::LevelUp));
        assert_eq!(condition.min_happiness, Some(160));
        assert_eq!(condition.time_of_day.as_deref(), Some("night"));
    }

    #[test]
    fn empty_time_of_day_is_treated_as_unset() {
        // PokeAPI sends "" rather than null for "any time of day".
        let tree = eevee();
        let vaporeon = tree.find("vaporeon").unwrap();
        assert_eq!(vaporeon.condition.as_ref().unwrap().time_of_day, None);
    }

    #[test]
    fn missing_fields_do_not_break_parsing() {
        // Only the fields we care about are guaranteed; the rest must default.
        let json = r#"{
          "chain": {
            "species": { "name": "pichu", "url": "" },
            "evolves_to": [{
              "species": { "name": "pikachu", "url": "" },
              "evolution_details": [{ "trigger": { "name": "level-up", "url": "" } }],
              "evolves_to": []
            }]
          }
        }"#;
        let raw: RawEvolutionChain = serde_json::from_str(json).unwrap();
        let tree = parse_chain(&raw.chain);
        let pikachu = tree.find("pikachu").unwrap();
        assert_eq!(
            pikachu.condition.as_ref().unwrap().trigger,
            Some(EvolutionTrigger::LevelUp)
        );
    }
}
