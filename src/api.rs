//! PokeAPI client.
//!
//! All network access lives here. Functions take a shared [`reqwest::Client`]
//! and return clean domain types from `models.rs`. Raw wire-format structs are
//! kept private so the JSON shape never leaks out of this module.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Semaphore;

use crate::models::{
    Ability, AbilityInfo, EvolutionCondition, EvolutionTree, EvolutionTrigger, PokemonDetail,
    PokemonEntry, Sprite, SpriteVariant, Stat, StatKind,
};
use crate::retry::{self, FailureKind};

const BASE_URL: &str = "https://pokeapi.co/api/v2";
/// How many Pokemon to load into the sidebar. Covers all current species.
const LIST_LIMIT: u32 = 1302;

/// Ceiling on requests in flight at once across the whole process.
///
/// Fetches are dispatched from independent tasks that know nothing about each
/// other, so without a shared limit a single keypress can produce a burst: an
/// evolution chain requests a sprite per member, and Eevee's is nine. PokeAPI
/// asks clients to be considerate of its fair-use policy, and this is where we
/// hold ourselves to it.
const MAX_CONCURRENT_REQUESTS: usize = 6;

/// How long to wait for a connection to establish before giving up on it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long any single attempt may take end to end. `reqwest` applies no
/// timeout of its own, so without this a connection that opens and then stalls
/// leaves the request pending forever — and the UI showing a load that can
/// never resolve.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Errors that can occur while talking to PokeAPI.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("could not locate the evolution chain for this Pokemon")]
    MissingEvolutionChain,
    /// The server answered 404. Unlike the transport failures [`Network`]
    /// covers, this is a permanent answer about a name, so callers can write it
    /// down instead of re-asking on every run.
    ///
    /// [`Network`]: ApiError::Network
    #[error("no such resource: {0}")]
    NotFound(String),
    #[error("could not decode sprite image: {0}")]
    Image(#[from] image::ImageError),
}

/// Builds the shared HTTP client used for every request in the session.
pub fn build_client() -> Result<reqwest::Client, ApiError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("pokeductor/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    Ok(client)
}

/// The process-wide cap on in-flight requests.
///
/// A `static` rather than a field on the client because the limit belongs to
/// PokeAPI, not to any one caller: every task must queue behind the same
/// permits for the cap to mean anything.
fn request_permits() -> &'static Semaphore {
    static PERMITS: OnceLock<Semaphore> = OnceLock::new();
    PERMITS.get_or_init(|| Semaphore::new(MAX_CONCURRENT_REQUESTS))
}

/// Wraps a failed request in the error the caller sees.
///
/// A 404 is separated out because it is the one failure that says something
/// durable about the *name* rather than about the connection: callers may cache
/// it, which they must never do for a timeout.
fn api_error(err: reqwest::Error) -> ApiError {
    if err.status() == Some(reqwest::StatusCode::NOT_FOUND) {
        let what = err
            .url()
            .map(|u| u.path().trim_start_matches("/api/v2/").to_string())
            .unwrap_or_else(|| "resource".to_string());
        return ApiError::NotFound(what);
    }
    ApiError::Network(err)
}

/// Sorts a `reqwest` failure into the classes the retry policy reasons about.
fn classify(err: &reqwest::Error) -> FailureKind {
    if let Some(status) = err.status() {
        return FailureKind::Status(status.as_u16());
    }
    if err.is_decode() {
        return FailureKind::Decode;
    }
    // Timeouts, connection failures, redirect loops and DNS errors all mean no
    // usable response arrived, which is the case worth another attempt.
    FailureKind::Transport
}

/// A uniform draw from `[0, 1)` for the backoff jitter.
///
/// Taken from the clock rather than from `rand`: the requirement is that
/// simultaneous retries stop colliding, which nanosecond scheduling noise
/// satisfies, and it is not worth a dependency for that.
fn jitter_fraction() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos) / f64::from(1_000_000_000u32)
}

/// Sends a request, retrying transient failures, and returns the successful
/// response.
///
/// Takes a closure rather than a `RequestBuilder` because a builder is consumed
/// by `send` and each attempt needs a fresh one. The concurrency permit is
/// acquired per attempt, so a task waiting out a backoff is not holding a slot
/// another task could be using.
async fn send_with_retry<F>(build: F) -> Result<reqwest::Response, ApiError>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let mut attempt = 0;
    loop {
        let result = {
            // `request_permits` is never closed, so acquisition cannot fail.
            let _permit = request_permits().acquire().await;
            build().send().await.and_then(|r| r.error_for_status())
        };

        let err = match result {
            Ok(response) => return Ok(response),
            Err(err) => err,
        };

        let last_attempt = attempt + 1 >= retry::MAX_ATTEMPTS;
        if last_attempt || !retry::is_retryable(classify(&err)) {
            return Err(api_error(err));
        }

        tokio::time::sleep(retry::backoff_delay(attempt, jitter_fraction())).await;
        attempt += 1;
    }
}

/// `GET` a URL and deserialize the JSON body.
async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, ApiError> {
    let response = send_with_retry(|| client.get(url)).await?;
    Ok(response.json().await?)
}

/// `GET` a URL and return the raw body.
async fn get_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, ApiError> {
    let response = send_with_retry(|| client.get(url)).await?;
    Ok(response.bytes().await?.to_vec())
}

/// Fetches the master list of Pokemon names for the sidebar.
pub async fn fetch_pokemon_list(client: &reqwest::Client) -> Result<Vec<PokemonEntry>, ApiError> {
    let url = format!("{BASE_URL}/pokemon?limit={LIST_LIMIT}&offset=0");
    let raw: NamedList = get_json(client, &url).await?;
    let entries = raw
        .results
        .into_iter()
        .map(|r| PokemonEntry {
            id: id_from_url(&r.url),
            name: r.name,
        })
        .collect();
    Ok(entries)
}

/// Every name PokeAPI files under a given type, e.g. every Water Pokemon.
///
/// One request answers a whole `type:` filter, which is why the sidebar can
/// offer type filtering at all: the alternative would be fetching all 1300
/// species just to read their typing.
pub async fn fetch_type_members(
    client: &reqwest::Client,
    type_name: &str,
) -> Result<Vec<String>, ApiError> {
    let url = format!("{BASE_URL}/type/{type_name}");
    let raw: RawType = get_json(client, &url).await?;
    Ok(raw.pokemon.into_iter().map(|p| p.pokemon.name).collect())
}

/// Reads the trailing numeric id out of a PokeAPI resource URL, which always
/// ends `.../pokemon/25/`. Returns 0 for a URL that does not carry one, which
/// simply reads as "no dex number" downstream.
fn id_from_url(url: &str) -> u32 {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|segment| segment.parse().ok())
        .unwrap_or(0)
}

/// Fetches everything needed to display a Pokemon: its details and its parsed
/// evolution tree. Performed as one logical unit so the UI receives a complete
/// payload in a single message.
pub async fn fetch_pokemon_bundle(
    client: &reqwest::Client,
    name: &str,
    variant: SpriteVariant,
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
    // sink the whole bundle, so failures here degrade to "no sprite". Only the
    // palette currently on screen is fetched; the other one waits until the
    // shiny toggle actually asks for it.
    let sprite = match detail.sprite_url_for(variant) {
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
    let response = send_with_retry(|| {
        client
            .get("https://api.mymemory.translated.net/get")
            .query(&[("q", clamped.as_str()), ("langpair", pair.as_str())])
    })
    .await?;
    let resp: MyMemoryResponse = response.json().await?;
    Ok(resp.response_data.translated_text)
}

/// Fetches the localized name and description of one ability.
///
/// The description comes from the game flavor text rather than the effect
/// entries: PokeAPI carries flavor in all five languages the info card uses,
/// while effect text exists only in English, German and French.
pub async fn fetch_ability(client: &reqwest::Client, name: &str) -> Result<AbilityInfo, ApiError> {
    let url = format!("{BASE_URL}/ability/{name}");
    let raw: RawAbility = get_json(client, &url).await?;

    let mut names = HashMap::new();
    for n in &raw.names {
        if CARD_LANGS.contains(&n.language.name.as_str()) {
            names
                .entry(n.language.name.clone())
                .or_insert_with(|| n.name.clone());
        }
    }

    // One entry per game version per language; the first is a fine
    // representative, exactly as for species flavor text.
    let mut flavors = HashMap::new();
    for e in &raw.flavor_text_entries {
        if CARD_LANGS.contains(&e.language.name.as_str()) {
            flavors
                .entry(e.language.name.clone())
                .or_insert_with(|| clean_flavor(&e.flavor_text));
        }
    }

    Ok(AbilityInfo {
        name: raw.name,
        names,
        flavors,
    })
}

/// Fetches and decodes just the sprite for a named Pokemon, in the requested
/// palette. Used to lazily populate the evolution overlay, where every member
/// of the chain needs art.
///
/// `name` is a `/pokemon` key, i.e. a *variety* name — callers holding a
/// species name resolve it through [`fetch_default_variety`] first.
pub async fn fetch_named_sprite(
    client: &reqwest::Client,
    name: &str,
    variant: SpriteVariant,
) -> Result<Option<Sprite>, ApiError> {
    let detail = fetch_detail(client, name).await?;
    match detail.sprite_url_for(variant) {
        Some(url) => Ok(Some(fetch_sprite(client, url).await?)),
        None => Ok(None),
    }
}

/// Downloads a sprite PNG and decodes it into raw RGBA pixels.
pub async fn fetch_sprite(client: &reqwest::Client, url: &str) -> Result<Sprite, ApiError> {
    let bytes = get_bytes(client, url).await?;
    let image = image::load_from_memory(&bytes)?.to_rgba8();
    let (width, height) = image.dimensions();
    let pixels = image.pixels().map(|p| p.0).collect();
    Ok(Sprite::new(width, height, pixels))
}

async fn fetch_detail(client: &reqwest::Client, name: &str) -> Result<PokemonDetail, ApiError> {
    let url = format!("{BASE_URL}/pokemon/{name}");
    let raw: RawPokemon = get_json(client, &url).await?;

    let mut types: Vec<(u8, String)> = raw
        .types
        .into_iter()
        .map(|t| (t.slot, t.type_.name))
        .collect();
    types.sort_by_key(|(slot, _)| *slot);

    let mut abilities: Vec<(u8, Ability)> = raw
        .abilities
        .into_iter()
        .map(|a| {
            (
                a.slot,
                Ability {
                    name: a.ability.name,
                    is_hidden: a.is_hidden,
                },
            )
        })
        .collect();
    abilities.sort_by_key(|(slot, _)| *slot);

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
        abilities: abilities.into_iter().map(|(_, ability)| ability).collect(),
        stats,
        height: raw.height,
        weight: raw.weight,
        sprite_url: raw.sprites.front_default,
        shiny_sprite_url: raw.sprites.front_shiny,
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

/// Resolves the `/pokemon` key a species' artwork is filed under.
///
/// Evolution chains name *species* (`giratina`), but sprites hang off
/// varieties (`giratina-altered`), and for a species whose default form has its
/// own name the two differ — `/pokemon/giratina` is a 404. The species record
/// carries the mapping, so ask it rather than guessing.
pub async fn fetch_default_variety(
    client: &reqwest::Client,
    species: &str,
) -> Result<String, ApiError> {
    let url = format!("{BASE_URL}/pokemon-species/{species}");
    let raw: RawSpecies = get_json(client, &url).await?;
    Ok(default_variety_name(&raw.varieties, species))
}

/// Picks the default variety out of a species' form list.
///
/// Falling back to the species name covers the ordinary case where the two
/// names agree, and is the right answer for a payload that marks no default at
/// all — better a request that may work than none.
fn default_variety_name(varieties: &[RawVariety], species: &str) -> String {
    varieties
        .iter()
        .find(|v| v.is_default)
        .map(|v| v.pokemon.name.clone())
        .unwrap_or_else(|| species.to_string())
}

/// Fetches a species record, pulling out the evolution-chain URL plus the genus
/// and flavor text in every language we care about for the info card.
async fn fetch_species(client: &reqwest::Client, name: &str) -> Result<SpeciesInfo, ApiError> {
    let url = format!("{BASE_URL}/pokemon-species/{name}");
    let species: RawSpecies = get_json(client, &url).await?;

    let chain_url = species
        .evolution_chain
        .map(|c| c.url)
        .ok_or(ApiError::MissingEvolutionChain)?;

    let mut genera = HashMap::new();
    for g in &species.genera {
        if CARD_LANGS.contains(&g.language.name.as_str()) {
            genera
                .entry(g.language.name.clone())
                .or_insert_with(|| g.genus.clone());
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
    let chain: RawEvolutionChain = get_json(client, url).await?;
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
    #[serde(default)]
    url: String,
}

#[derive(serde::Deserialize)]
struct RawAbilitySlot {
    ability: NamedResource,
    is_hidden: bool,
    slot: u8,
}

#[derive(serde::Deserialize)]
struct RawAbility {
    name: String,
    names: Vec<RawAbilityName>,
    flavor_text_entries: Vec<RawAbilityFlavor>,
}

#[derive(serde::Deserialize)]
struct RawAbilityName {
    name: String,
    language: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawAbilityFlavor {
    flavor_text: String,
    language: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawType {
    pokemon: Vec<RawTypeMember>,
}

#[derive(serde::Deserialize)]
struct RawTypeMember {
    pokemon: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawPokemon {
    id: u32,
    name: String,
    height: u32,
    weight: u32,
    types: Vec<RawTypeSlot>,
    #[serde(default)]
    abilities: Vec<RawAbilitySlot>,
    stats: Vec<RawStatSlot>,
    sprites: RawSprites,
    species: NamedResource,
}

#[derive(serde::Deserialize)]
struct RawSprites {
    #[serde(default)]
    front_default: Option<String>,
    #[serde(default)]
    front_shiny: Option<String>,
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
    #[serde(default)]
    varieties: Vec<RawVariety>,
}

/// One entry of a species' `varieties` list: the forms it ships as, exactly one
/// of which is the default.
#[derive(serde::Deserialize)]
struct RawVariety {
    #[serde(default)]
    is_default: bool,
    pokemon: NamedResource,
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

    /// End-to-end check against the live API, covering the whole request path:
    /// the permit, the retry wrapper, the real timeouts, and parsing a payload
    /// nobody wrote by hand.
    ///
    /// Ignored by default so neither CI nor a routine `cargo test` depends on
    /// the network or spends PokeAPI's quota. Run it deliberately after
    /// touching this module:
    ///
    /// ```text
    /// cargo test --all-features -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "requires network access to pokeapi.co"]
    async fn the_live_api_still_answers() {
        let client = build_client().expect("client builds");

        let list = fetch_pokemon_list(&client).await.expect("list fetch");
        assert!(list.len() > 1000, "got {} entries", list.len());

        let (detail, tree, sprite) = fetch_pokemon_bundle(&client, "eevee", SpriteVariant::Normal)
            .await
            .expect("bundle fetch");
        assert_eq!(detail.name, "eevee");
        assert!(detail.types.contains(&"normal".to_string()));
        assert!(sprite.is_some(), "eevee should have artwork");
        assert!(
            detail.shiny_sprite_url.is_some(),
            "eevee should have shiny artwork"
        );
        assert!(
            tree.leaf_count() >= 8,
            "eevee branches {} ways",
            tree.leaf_count()
        );

        // The fact the evolution-card fix rests on: a species whose default
        // form is named after that form resolves to the form's name, which is
        // the only one `/pokemon` will answer to.
        let variety = fetch_default_variety(&client, "giratina")
            .await
            .expect("species fetch");
        assert_eq!(variety, "giratina-altered");
        assert!(
            fetch_named_sprite(&client, &variety, SpriteVariant::Normal)
                .await
                .expect("sprite fetch")
                .is_some(),
            "giratina-altered should have artwork"
        );

        // A name that does not exist must fail fast rather than burn the full
        // retry budget on an answer that will not change.
        let started = std::time::Instant::now();
        let missing =
            fetch_pokemon_bundle(&client, "missingno-not-a-species", SpriteVariant::Normal).await;
        assert!(
            matches!(missing, Err(ApiError::NotFound(_))),
            "a bogus species should read as a 404, not a transport failure"
        );
        assert!(
            started.elapsed() < REQUEST_TIMEOUT,
            "a 404 took {:?}, so it was retried",
            started.elapsed()
        );
    }

    /// Parses just the `varieties` list out of a `/pokemon-species` payload,
    /// which is all `default_variety_name` reads.
    fn varieties(json: &str) -> Vec<RawVariety> {
        serde_json::from_str::<RawSpecies>(json)
            .expect("species payload parses")
            .varieties
    }

    #[test]
    fn a_species_named_after_its_form_resolves_to_the_form() {
        let raw = varieties(
            r#"{
              "id": 487,
              "evolution_chain": { "url": "" },
              "varieties": [
                { "is_default": true,  "pokemon": { "name": "giratina-altered" } },
                { "is_default": false, "pokemon": { "name": "giratina-origin"  } }
              ]
            }"#,
        );
        assert_eq!(default_variety_name(&raw, "giratina"), "giratina-altered");
    }

    #[test]
    fn an_ordinary_species_resolves_to_its_own_name() {
        let raw = varieties(
            r#"{
              "id": 483,
              "evolution_chain": { "url": "" },
              "varieties": [{ "is_default": true, "pokemon": { "name": "dialga" } }]
            }"#,
        );
        assert_eq!(default_variety_name(&raw, "dialga"), "dialga");
    }

    #[test]
    fn a_payload_marking_no_default_falls_back_to_the_species_name() {
        let raw = varieties(
            r#"{
              "id": 1,
              "evolution_chain": { "url": "" },
              "varieties": [{ "is_default": false, "pokemon": { "name": "odd-form" } }]
            }"#,
        );
        assert_eq!(default_variety_name(&raw, "bulbasaur"), "bulbasaur");
        assert_eq!(default_variety_name(&[], "bulbasaur"), "bulbasaur");
    }

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
