//! PokeAPI client.
//!
//! All network access lives here. Functions take a shared [`reqwest::Client`]
//! and return clean domain types from `models.rs`. Raw wire-format structs are
//! kept private so the JSON shape never leaks out of this module.

use crate::models::{EvolutionTree, PokemonDetail, PokemonEntry, Stat, StatKind};

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
) -> Result<(PokemonDetail, EvolutionTree), ApiError> {
    let detail = fetch_detail(client, name).await?;
    let evolution = fetch_evolution(client, &detail.name).await?;
    Ok((detail, evolution))
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
        types: types.into_iter().map(|(_, name)| name).collect(),
        stats,
        height: raw.height,
        weight: raw.weight,
    })
}

/// Resolves species -> evolution-chain URL -> parsed tree.
async fn fetch_evolution(
    client: &reqwest::Client,
    name: &str,
) -> Result<EvolutionTree, ApiError> {
    let species_url = format!("{BASE_URL}/pokemon-species/{name}");
    let species: RawSpecies = client
        .get(species_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let chain_url = species
        .evolution_chain
        .map(|c| c.url)
        .ok_or(ApiError::MissingEvolutionChain)?;

    let chain: RawEvolutionChain = client
        .get(chain_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(parse_chain(&chain.chain))
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
