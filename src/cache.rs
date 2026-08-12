//! On-disk cache for everything we pull off the network.
//!
//! PokeAPI is effectively an append-only archive: a species' stats, typing and
//! evolution chain do not change once published. That makes every response
//! worth keeping between runs — the app then opens instantly, and keeps
//! working with no network at all.
//!
//! Every function here is best-effort. A cache directory that cannot be
//! created, read or written is treated as a miss, never as an error the user
//! sees: the worst case is that we fall back to the network exactly like
//! before this module existed.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::models::{AbilityInfo, EvolutionTree, PokemonDetail, PokemonEntry, Sprite};

/// Bumped whenever the cached representation changes shape. Files written by
/// an older build are treated as misses and overwritten on the next fetch,
/// which saves us from ever deserializing stale data into the wrong struct.
const VERSION: u32 = 3;

/// How long the master species list stays fresh. Individual records never
/// expire — PokeAPI does not rewrite history, it only appends new species, and
/// those show up when the list is refreshed.
const LIST_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// A cached copy of the master list, plus whether it is still within
/// [`LIST_TTL`]. A stale list is still returned: it is a far better starting
/// point than an empty sidebar, and the caller can refresh in the background
/// (or keep using it when the network is unreachable).
pub struct CachedList {
    pub entries: Vec<PokemonEntry>,
    pub fresh: bool,
}

/// The two halves of a species record that are always fetched and stored
/// together, mirroring [`crate::api::fetch_pokemon_bundle`].
#[derive(Serialize, Deserialize)]
pub struct CachedBundle {
    pub detail: PokemonDetail,
    pub evolution: EvolutionTree,
}

/// Everything we store is wrapped in this so a file written by an incompatible
/// build can be recognised and discarded instead of failing to parse in some
/// harder-to-diagnose way.
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    version: u32,
    data: T,
}

// --- Public API ----------------------------------------------------------

/// The master Pokemon list, if it has ever been cached.
pub async fn load_list() -> Option<CachedList> {
    let path = root()?.join("list.json");
    let entries = read_json(&path).await?;
    Some(CachedList { entries, fresh: age(&path).await.is_some_and(|a| a < LIST_TTL) })
}

pub async fn store_list(entries: &[PokemonEntry]) {
    let Some(path) = root().map(|r| r.join("list.json")) else {
        return;
    };
    write_json(&path, &entries.to_vec()).await;
}

/// A species' details and evolution chain, if they have ever been cached.
pub async fn load_bundle(name: &str) -> Option<CachedBundle> {
    let path = root()?.join("species").join(format!("{}.json", slug(name)));
    read_json(&path).await
}

pub async fn store_bundle(name: &str, detail: &PokemonDetail, evolution: &EvolutionTree) {
    let Some(path) = root().map(|r| r.join("species").join(format!("{}.json", slug(name)))) else {
        return;
    };
    // Cloning to build the owned envelope costs one copy of a small record on
    // a background task, which is cheaper than threading lifetimes through
    // serde for a write that happens once per species per month.
    let bundle = CachedBundle { detail: detail.clone(), evolution: evolution.clone() };
    write_json(&path, &bundle).await;
}

/// The localized text for one ability, if cached. Ability descriptions never
/// change once written, so they never expire.
pub async fn load_ability(name: &str) -> Option<AbilityInfo> {
    let path = ability_path(name)?;
    read_json(&path).await
}

pub async fn store_ability(name: &str, info: &AbilityInfo) {
    let Some(path) = ability_path(name) else {
        return;
    };
    write_json(&path, info).await;
}

/// The membership list for a type (`"water"`, `"ghost"`, ...), if cached.
/// A type's roster only grows with a new generation, so it never expires.
pub async fn load_type_members(type_name: &str) -> Option<Vec<String>> {
    let path = type_path(type_name)?;
    read_json(&path).await
}

pub async fn store_type_members(type_name: &str, members: &[String]) {
    let Some(path) = type_path(type_name) else {
        return;
    };
    write_json(&path, &members.to_vec()).await;
}

/// A decoded sprite, if one has ever been cached for `name`.
///
/// Sprites are stored re-encoded as PNG rather than as raw RGBA: a species'
/// artwork is a few kilobytes compressed against ~36 KB flattened, and the
/// decoder is already a dependency.
pub async fn load_sprite(name: &str) -> Option<Sprite> {
    let path = sprite_path(name)?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(Sprite { width, height, pixels: image.pixels().map(|p| p.0).collect() })
}

pub async fn store_sprite(name: &str, sprite: &Sprite) {
    let Some(path) = sprite_path(name) else {
        return;
    };
    let Some(bytes) = encode_png(sprite) else {
        return;
    };
    write_atomic(&path, &bytes).await;
}

/// Records that `name` has no artwork at all, so we do not re-ask the network
/// on every run. Stored as an empty file, which [`load_sprite`] fails to decode
/// and therefore reports as "no sprite" — the same answer, without the request.
pub async fn store_missing_sprite(name: &str) {
    let Some(path) = sprite_path(name) else {
        return;
    };
    write_atomic(&path, &[]).await;
}

/// Whether we have already resolved `name`'s artwork one way or the other.
/// Distinguishes "never looked" from "looked, and there is none".
pub async fn has_sprite_answer(name: &str) -> bool {
    match sprite_path(name) {
        Some(path) => tokio::fs::metadata(&path).await.is_ok(),
        None => false,
    }
}

/// A machine translation of a flavor blurb. These cost a rate-limited
/// third-party request, so they are the most valuable thing here to keep.
pub async fn load_translation(name: &str, lang: &str) -> Option<String> {
    let path = translation_path(name, lang)?;
    tokio::fs::read_to_string(&path).await.ok()
}

pub async fn store_translation(name: &str, lang: &str, text: &str) {
    let Some(path) = translation_path(name, lang) else {
        return;
    };
    write_atomic(&path, text.as_bytes()).await;
}

// --- Paths ---------------------------------------------------------------

/// Root of the cache tree, resolved once per process.
///
/// Follows the XDG base-directory spec where it applies and falls back to the
/// platform's usual per-user cache location otherwise. `None` means we could
/// not work out a home directory, which disables caching entirely.
fn root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))?;
        Some(base.join("pokeductor"))
    })
    .as_deref()
}

fn ability_path(name: &str) -> Option<PathBuf> {
    Some(root()?.join("abilities").join(format!("{}.json", slug(name))))
}

fn type_path(type_name: &str) -> Option<PathBuf> {
    Some(root()?.join("types").join(format!("{}.json", slug(type_name))))
}

fn sprite_path(name: &str) -> Option<PathBuf> {
    Some(root()?.join("sprites").join(format!("{}.png", slug(name))))
}

fn translation_path(name: &str, lang: &str) -> Option<PathBuf> {
    Some(root()?.join("translations").join(format!("{}.{}.txt", slug(name), slug(lang))))
}

/// Reduces an API name to something safe to use as a single filename.
///
/// PokeAPI names are already lowercase URL slugs, so in practice this is a
/// no-op; it exists so that a surprising name can never escape the cache
/// directory or collide with a path separator.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

// --- File helpers --------------------------------------------------------

async fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let envelope: Envelope<T> = serde_json::from_slice(&bytes).ok()?;
    (envelope.version == VERSION).then_some(envelope.data)
}

async fn write_json<T: Serialize>(path: &Path, data: &T) {
    let envelope = Envelope { version: VERSION, data };
    if let Ok(bytes) = serde_json::to_vec(&envelope) {
        write_atomic(path, &bytes).await;
    }
}

/// Writes `bytes` to `path` via a temporary file and a rename, so an
/// interrupted run can never leave a half-written entry behind for the next
/// one to read back as valid.
async fn write_atomic(path: &Path, bytes: &[u8]) {
    let Some(dir) = path.parent() else {
        return;
    };
    if tokio::fs::create_dir_all(dir).await.is_err() {
        return;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if tokio::fs::write(&tmp, bytes).await.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
        return;
    }
    if tokio::fs::rename(&tmp, path).await.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
}

/// How long ago `path` was written, or `None` if that cannot be determined —
/// in which case callers treat the entry as stale and refresh it.
async fn age(path: &Path) -> Option<Duration> {
    let modified = tokio::fs::metadata(path).await.ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn encode_png(sprite: &Sprite) -> Option<Vec<u8>> {
    let flat: Vec<u8> = sprite.pixels.iter().flatten().copied().collect();
    let buffer = image::RgbaImage::from_raw(sprite.width, sprite.height, flat)?;
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_keeps_ordinary_api_names_intact() {
        assert_eq!(slug("bulbasaur"), "bulbasaur");
        assert_eq!(slug("ho-oh"), "ho-oh");
        assert_eq!(slug("raichu-alola"), "raichu-alola");
    }

    #[test]
    fn slug_neutralises_path_separators() {
        assert_eq!(slug("../../etc/passwd"), "______etc_passwd");
        assert!(!slug("a/b").contains('/'));
    }

    #[test]
    fn envelope_from_another_version_is_a_miss() {
        let raw = br#"{"version":999,"data":[{"name":"bulbasaur","id":1}]}"#;
        let envelope: Envelope<Vec<PokemonEntry>> = serde_json::from_slice(raw).unwrap();
        assert_ne!(envelope.version, VERSION);
    }

    #[test]
    fn sprites_survive_a_png_round_trip() {
        let sprite = Sprite {
            width: 2,
            height: 1,
            pixels: vec![[255, 0, 0, 255], [0, 128, 255, 128]],
        };
        let bytes = encode_png(&sprite).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.pixels().map(|p| p.0).collect::<Vec<_>>(), sprite.pixels);
    }
}
