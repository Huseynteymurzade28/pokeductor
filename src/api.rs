//! PokeAPI client.
//!
//! All network access lives here. Functions take a shared [`reqwest::Client`]
//! and return clean domain types from `models.rs`. Raw wire-format structs are
//! kept private so the JSON shape never leaks out of this module.

use crate::models::{EvolutionTree, PokemonDetail, PokemonEntry, Sprite, Stat, StatKind};

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
    detail.genus = species.genus;
    detail.flavor = species.flavor;
    let evolution = fetch_chain(client, &species.chain_url).await?;

    // The sprite is a nice-to-have: a missing or undecodable image must not
    // sink the whole bundle, so failures here degrade to "no sprite".
    let sprite = match &detail.sprite_url {
        Some(url) => fetch_sprite(client, url).await.ok(),
        None => None,
    };

    Ok((detail, evolution, sprite))
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
        id: raw.id,
        name: raw.name,
        species: raw.species.name,
        types: types.into_iter().map(|(_, name)| name).collect(),
        stats,
        height: raw.height,
        weight: raw.weight,
        sprite_url: raw.sprites.front_default,
        genus: None,
        flavor: None,
    })
}

/// The slice of species data the rest of the app cares about.
struct SpeciesInfo {
    chain_url: String,
    genus: Option<String>,
    flavor: Option<String>,
}

/// Fetches a species record, pulling out the evolution-chain URL plus the
/// English genus and flavor text for the info card.
async fn fetch_species(client: &reqwest::Client, name: &str) -> Result<SpeciesInfo, ApiError> {
    let url = format!("{BASE_URL}/pokemon-species/{name}");
    let species: RawSpecies = client.get(url).send().await?.error_for_status()?.json().await?;

    let chain_url = species
        .evolution_chain
        .map(|c| c.url)
        .ok_or(ApiError::MissingEvolutionChain)?;

    let genus = species
        .genera
        .iter()
        .find(|g| g.language.name == "en")
        .map(|g| g.genus.clone());

    let flavor = species
        .flavor_text_entries
        .iter()
        .find(|e| e.language.name == "en")
        .map(|e| clean_flavor(&e.flavor_text));

    Ok(SpeciesInfo { chain_url, genus, flavor })
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
        children: link.evolves_to.iter().map(parse_chain).collect(),
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
    evolves_to: Vec<RawChainLink>,
}
