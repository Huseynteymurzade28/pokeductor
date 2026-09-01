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

use crate::models::{
    AbilityInfo, EvolutionTree, MoveInfo, PokemonDetail, PokemonEntry, RosterKind, RosterTerm,
    Sprite, SpriteVariant,
};

/// Bumped whenever the cached representation changes shape. Files written by
/// an older build are treated as misses and overwritten on the next fetch,
/// which saves us from ever deserializing stale data into the wrong struct.
const VERSION: u32 = 5;

/// How long the master species list stays fresh. Individual records never
/// expire — PokeAPI does not rewrite history, it only appends new species, and
/// those show up when the list is refreshed.
const LIST_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// How long a recorded "this species has no artwork" answer is trusted.
///
/// The reasoning that makes every other record permanent — PokeAPI appends, it
/// does not rewrite — does not hold for artwork: sprites get backfilled for
/// newly added species, and those are exactly the ones likely to be missing art
/// the first time anyone looks. Without an expiry that species stays blank on
/// this install forever, with no way for the user to know why. A successfully
/// decoded sprite still never expires; only the negative answer does.
const MISSING_SPRITE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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
    Some(CachedList {
        entries,
        fresh: age(&path).await.is_some_and(|a| a < LIST_TTL),
    })
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
    let bundle = CachedBundle {
        detail: detail.clone(),
        evolution: evolution.clone(),
    };
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

/// One move's record, if cached. A move's typing and numbers are as fixed as a
/// species' are, so this never expires either.
pub async fn load_move(name: &str) -> Option<MoveInfo> {
    let path = move_path(name)?;
    read_json(&path).await
}

pub async fn store_move(name: &str, info: &MoveInfo) {
    let Some(path) = move_path(name) else {
        return;
    };
    write_json(&path, info).await;
}

/// The membership list behind one filter term — `type:water`, `egg:monster`,
/// `ability:levitate` — if cached. A roster only grows with a new generation,
/// so it never expires.
pub async fn load_roster(term: &RosterTerm) -> Option<Vec<String>> {
    let path = roster_path(term)?;
    read_json(&path).await
}

pub async fn store_roster(term: &RosterTerm, members: &[String]) {
    let Some(path) = roster_path(term) else {
        return;
    };
    write_json(&path, &members.to_vec()).await;
}

/// A decoded sprite, if one has ever been cached for `name` in `variant`.
///
/// Sprites are stored re-encoded as PNG rather than as raw RGBA: a species'
/// artwork is a few kilobytes compressed against ~36 KB flattened, and the
/// decoder is already a dependency.
pub async fn load_sprite(name: &str, variant: SpriteVariant) -> Option<Sprite> {
    let path = sprite_path(name, variant)?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(Sprite::new(
        width,
        height,
        image.pixels().map(|p| p.0).collect(),
    ))
}

pub async fn store_sprite(name: &str, sprite: &Sprite, variant: SpriteVariant) {
    let Some(path) = sprite_path(name, variant) else {
        return;
    };
    let Some(bytes) = encode_png(sprite) else {
        return;
    };
    write_atomic(&path, &bytes).await;
}

/// Records that `name` has no artwork in `variant`, so we do not re-ask the
/// network on every run. Stored as an empty file, which [`load_sprite`] fails to
/// decode and therefore reports as "no sprite" — the same answer, without the
/// request.
pub async fn store_missing_sprite(name: &str, variant: SpriteVariant) {
    let Some(path) = sprite_path(name, variant) else {
        return;
    };
    write_atomic(&path, &[]).await;
}

/// Whether we have already resolved `name`'s artwork in `variant` one way or
/// the other. Distinguishes "never looked" from "looked, and there is none",
/// per palette: a species can be cached in one and unknown in the other.
///
/// A third state sits between them — "looked a while ago, and there was none",
/// which reads as a miss so the question gets asked again. See
/// [`MISSING_SPRITE_TTL`].
pub async fn has_sprite_answer(name: &str, variant: SpriteVariant) -> bool {
    match sprite_path(name, variant) {
        Some(path) => sprite_answer_is_current(&path).await,
        None => false,
    }
}

/// Whether the answer stored at `path` is still one we are willing to reuse.
///
/// Split out from [`has_sprite_answer`] so the expiry can be tested against a
/// real file without going through the process-wide cache root.
async fn sprite_answer_is_current(path: &Path) -> bool {
    let Ok(meta) = tokio::fs::metadata(path).await else {
        return false; // never looked
    };
    // A decoded PNG is final. Only the empty marker written by
    // `store_missing_sprite` is allowed to go stale.
    if meta.len() > 0 {
        return true;
    }
    age(path).await.is_some_and(|a| a < MISSING_SPRITE_TTL)
}

/// The `/pokemon` name a species' artwork is filed under, if we have ever
/// resolved it. The mapping is a property of the species and never changes, so
/// it is stored without an expiry — one request per species per install.
pub async fn load_default_variety(species: &str) -> Option<String> {
    let path = variety_path(species)?;
    let name = tokio::fs::read_to_string(&path).await.ok()?;
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

pub async fn store_default_variety(species: &str, variety: &str) {
    let Some(path) = variety_path(species) else {
        return;
    };
    write_atomic(&path, variety.as_bytes()).await;
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

// --- Cache management ----------------------------------------------------

/// Where this build keeps its cache, if a directory could be worked out.
///
/// Exposed for the `--cache-dir` and `--clear-cache` commands, which exist
/// because the answer was previously a matter of reading this file's docs and
/// guessing at environment variables.
pub fn dir() -> Option<&'static Path> {
    root()
}

/// Removes the cache tree, reporting whether there was one to remove.
///
/// The only function here that is not best-effort. Everywhere else a failed
/// filesystem operation is a cache miss and the app carries on; this one runs
/// because the user named it, so "I could not delete it" is an answer they
/// need rather than one to swallow. An absent directory is not a failure —
/// the tree is gone either way, which is what was asked for.
///
/// Takes the path rather than reading [`root`] so a test can point it at a
/// directory it owns.
pub async fn clear(dir: &Path) -> std::io::Result<bool> {
    match tokio::fs::remove_dir_all(dir).await {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
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
    Some(
        root()?
            .join("abilities")
            .join(format!("{}.json", slug(name))),
    )
}

/// Where one roster is stored. Each kind gets its own directory, because the
/// same word means different things to two of them — `poison` is a type and an
/// ability both — and because `abilities/` already holds ability *descriptions*
/// rather than membership lists.
fn move_path(name: &str) -> Option<PathBuf> {
    Some(root()?.join("moves").join(format!("{}.json", slug(name))))
}

fn roster_path(term: &RosterTerm) -> Option<PathBuf> {
    let dir = match term.kind {
        RosterKind::Type => "types",
        RosterKind::Ability => "ability-members",
        RosterKind::EggGroup => "egg-groups",
    };
    Some(
        root()?
            .join(dir)
            .join(format!("{}.json", slug(&term.value))),
    )
}

fn sprite_path(name: &str, variant: SpriteVariant) -> Option<PathBuf> {
    Some(root()?.join("sprites").join(sprite_file(name, variant)))
}

/// Filename one species' artwork is stored under. The palette is part of the
/// name, or a shiny PNG would silently overwrite the normal one.
fn sprite_file(name: &str, variant: SpriteVariant) -> String {
    format!("{}{}.png", slug(name), variant.file_suffix())
}

fn variety_path(species: &str) -> Option<PathBuf> {
    Some(
        root()?
            .join("varieties")
            .join(format!("{}.txt", slug(species))),
    )
}

fn translation_path(name: &str, lang: &str) -> Option<PathBuf> {
    Some(
        root()?
            .join("translations")
            .join(format!("{}.{}.txt", slug(name), slug(lang))),
    )
}

/// Reduces an API name to something safe to use as a single filename.
///
/// PokeAPI names are already lowercase URL slugs, so in practice this is a
/// no-op; it exists so that a surprising name can never escape the cache
/// directory or collide with a path separator.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

// --- File helpers --------------------------------------------------------

async fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let envelope: Envelope<T> = serde_json::from_slice(&bytes).ok()?;
    (envelope.version == VERSION).then_some(envelope.data)
}

async fn write_json<T: Serialize>(path: &Path, data: &T) {
    let envelope = Envelope {
        version: VERSION,
        data,
    };
    if let Ok(bytes) = serde_json::to_vec(&envelope) {
        write_atomic(path, &bytes).await;
    }
}

/// Writes `bytes` to `path` via a temporary file and a rename, so an
/// interrupted run can never leave a half-written entry behind for the next
/// one to read back as valid.
///
/// Shared with [`crate::session`], which writes outside this tree but wants
/// exactly the same guarantee.
pub(crate) async fn write_atomic(path: &Path, bytes: &[u8]) {
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
    let flat: Vec<u8> = sprite.pixels().iter().flatten().copied().collect();
    let buffer = image::RgbaImage::from_raw(sprite.width(), sprite.height(), flat)?;
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
    fn shiny_artwork_gets_its_own_filename() {
        assert_eq!(sprite_file("pikachu", SpriteVariant::Normal), "pikachu.png");
        assert_eq!(
            sprite_file("pikachu", SpriteVariant::Shiny),
            "pikachu.shiny.png"
        );
    }

    #[tokio::test]
    async fn clearing_removes_the_tree_and_reports_that_it_did() {
        let dir = scratch_dir("clear");
        std::fs::create_dir_all(dir.join("species")).expect("nested entry");
        std::fs::write(dir.join("species").join("mew.json"), b"{}").expect("entry");

        assert!(
            clear(&dir).await.expect("removable"),
            "found a tree to remove"
        );
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn clearing_a_cache_that_was_never_written_is_not_a_failure() {
        let dir = scratch_dir("clear-absent");
        std::fs::remove_dir_all(&dir).expect("start from nothing");

        assert!(!clear(&dir).await.expect("absence is not an error"));
    }

    #[test]
    fn envelope_from_another_version_is_a_miss() {
        let raw = br#"{"version":999,"data":[{"name":"bulbasaur","id":1}]}"#;
        let envelope: Envelope<Vec<PokemonEntry>> = serde_json::from_slice(raw).unwrap();
        assert_ne!(envelope.version, VERSION);
    }

    /// A directory of our own under the system temp dir. The cache root is
    /// resolved once per process and shared by every test, so the expiry tests
    /// work on a path they own instead.
    fn scratch_dir(label: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pokeductor-test-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Rewinds a file's mtime, which is the only input to [`age`].
    fn backdate(path: &Path, by: Duration) {
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open for touch");
        let when = SystemTime::now() - by;
        file.set_times(std::fs::FileTimes::new().set_modified(when))
            .expect("backdate mtime");
    }

    #[tokio::test]
    async fn a_species_never_looked_up_reads_as_a_miss() {
        let dir = scratch_dir("never");
        assert!(!sprite_answer_is_current(&dir.join("absent.png")).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_fresh_no_artwork_answer_still_suppresses_the_request() {
        let dir = scratch_dir("fresh");
        let path = dir.join("marker.png");
        std::fs::write(&path, []).expect("write marker");
        assert!(sprite_answer_is_current(&path).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_stale_no_artwork_answer_is_asked_again() {
        let dir = scratch_dir("stale");
        let path = dir.join("marker.png");
        std::fs::write(&path, []).expect("write marker");
        backdate(&path, MISSING_SPRITE_TTL + Duration::from_secs(60));
        assert!(!sprite_answer_is_current(&path).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_cached_sprite_never_expires() {
        let dir = scratch_dir("kept");
        let path = dir.join("sprite.png");
        std::fs::write(&path, b"not really a png, but not empty").expect("write sprite");
        backdate(&path, MISSING_SPRITE_TTL * 52);
        assert!(sprite_answer_is_current(&path).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sprites_survive_a_png_round_trip() {
        let sprite = Sprite::new(2, 1, vec![[255, 0, 0, 255], [0, 128, 255, 128]]);
        let bytes = encode_png(&sprite).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(
            decoded.pixels().map(|p| p.0).collect::<Vec<_>>(),
            sprite.pixels()
        );
    }
}
