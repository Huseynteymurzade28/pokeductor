//! Session state that outlives one run.
//!
//! Everything in [`crate::cache`] is a second copy of something PokeAPI
//! already knows: delete it and the next run refills it. What this module
//! keeps is the opposite — the choices the user made. The party they
//! assembled, the language they read the interface in, the palette and the
//! ordering they left the sidebar in. Nothing can reconstruct those, and
//! losing a six-member party to a stray `q` is what makes a tool feel
//! disposable.
//!
//! That difference is also why this does not live beside the cache. The XDG
//! spec puts state that should survive a restart but is not user data proper
//! under `$XDG_STATE_HOME`, and defines a cache directory as safe to wipe at
//! any time. A wiped party would be a bug rather than a refill, so it belongs
//! on the other side of that line.
//!
//! Like the cache, the layer is best-effort: a file that cannot be read is a
//! fresh session, never an error the user sees. It is written once, on the way
//! out, so a run that is killed rather than quit leaves the previous session
//! in place, and two instances quitting in turn means the last one wins.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::cache;
use crate::team;

/// Bumped whenever the stored shape changes. A file stamped with anything else
/// is discarded rather than parsed, exactly as in the cache.
const VERSION: u32 = 1;

/// What one run hands to the next.
///
/// The preferences are stored as codes rather than as the enums themselves so
/// the file stays readable and stable: reordering a Rust enum must not silently
/// switch somebody's interface language. Each is optional, so a file written by
/// a build that did not know about a setting leaves it at its default instead
/// of pinning it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Party members, in the order they were added, as raw API names.
    #[serde(default)]
    pub team: Vec<String>,
    /// Interface language, as [`crate::i18n::Language::flavor_code`].
    #[serde(default)]
    pub language: Option<String>,
    /// Sidebar ordering, as [`crate::app::SortKey::code`].
    #[serde(default)]
    pub sort: Option<String>,
    /// Whether the shiny palette was on.
    #[serde(default)]
    pub shiny: bool,
}

impl Session {
    /// The same session with anything the app could not have produced itself
    /// removed: blank names, repeats, and members past the party limit.
    ///
    /// The file sits in a directory users are invited to look inside, so it is
    /// treated as untrusted input on the way in. Restoring a nine-member party
    /// would break an invariant the rest of the app relies on, and it is not
    /// worth an error message when dropping the overflow says the same thing.
    fn sanitized(mut self) -> Self {
        let mut seen = Vec::with_capacity(self.team.len());
        self.team.retain(|name| {
            let keep = !name.trim().is_empty() && !seen.contains(name);
            if keep {
                seen.push(name.clone());
            }
            keep
        });
        self.team.truncate(team::MAX_MEMBERS);
        self
    }
}

/// Wrapper that carries the format version, mirroring the cache's envelope.
#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    session: Session,
}

/// The previous session, or a default one if there is nothing usable to read.
pub async fn load() -> Session {
    let Some(path) = path() else {
        return Session::default();
    };
    match tokio::fs::read(path).await {
        Ok(bytes) => decode(&bytes),
        Err(_) => Session::default(),
    }
}

/// Records `session` for the next run. Called once, as the app exits.
pub async fn store(session: &Session) {
    let Some(path) = path() else {
        return;
    };
    if let Some(bytes) = encode(session) {
        cache::write_atomic(path, &bytes).await;
    }
}

/// Renders a session file's contents. The counterpart to [`decode`], and split
/// out from [`store`] for the same reason: so the pair can be tested together
/// without going through the real state directory.
fn encode(session: &Session) -> Option<Vec<u8>> {
    // Cloning to build the owned envelope copies a handful of short strings,
    // once per run, which is cheaper than threading lifetimes through serde.
    let stored = Stored {
        version: VERSION,
        session: session.clone(),
    };
    // Pretty-printed: this file is small, lands in a directory users are
    // expected to be able to poke at, and is the only place the app writes
    // something they might want to read back.
    serde_json::to_vec_pretty(&stored).ok()
}

/// Parses a session file's contents. Split out from [`load`] so the decoding
/// rules can be tested without going through the real state directory.
fn decode(bytes: &[u8]) -> Session {
    let Ok(stored) = serde_json::from_slice::<Stored>(bytes) else {
        return Session::default();
    };
    if stored.version != VERSION {
        return Session::default();
    }
    stored.session.sanitized()
}

/// Where the session file lives, resolved once per process.
///
/// Follows the XDG base-directory spec where it applies and falls back to the
/// platform's usual per-user location otherwise. `None` means we could not work
/// out a home directory, which disables persistence entirely.
fn path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))?;
        Some(base.join("pokeductor").join("session.json"))
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_team(names: &[&str]) -> Session {
        Session {
            team: names.iter().map(|n| n.to_string()).collect(),
            ..Session::default()
        }
    }

    #[test]
    fn a_session_survives_the_round_trip() {
        let session = Session {
            team: vec!["snorlax".into(), "gyarados".into()],
            language: Some("tr".into()),
            sort: Some("name".into()),
            shiny: true,
        };
        let bytes = encode(&session).expect("encode");
        assert_eq!(decode(&bytes), session);
    }

    #[test]
    fn a_file_from_another_version_reads_as_a_fresh_session() {
        let raw = br#"{"version":999,"session":{"team":["pikachu"],"shiny":true}}"#;
        assert_eq!(decode(raw), Session::default());
    }

    #[test]
    fn nonsense_reads_as_a_fresh_session() {
        assert_eq!(decode(b"not json at all"), Session::default());
        assert_eq!(decode(b""), Session::default());
    }

    #[test]
    fn settings_a_file_omits_stay_at_their_defaults() {
        let raw = br#"{"version":1,"session":{"team":["pikachu"]}}"#;
        let session = decode(raw);
        assert_eq!(session.team, ["pikachu"]);
        assert_eq!(session.language, None);
        assert_eq!(session.sort, None);
        assert!(!session.shiny);
    }

    #[test]
    fn an_oversized_party_is_trimmed_to_the_limit() {
        let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        let session = session_with_team(&names).sanitized();
        assert_eq!(session.team.len(), team::MAX_MEMBERS);
        assert_eq!(session.team[0], "a");
    }

    #[test]
    fn repeats_and_blanks_are_dropped_in_order() {
        let session = session_with_team(&["snorlax", "", "gyarados", "snorlax", "   "]).sanitized();
        assert_eq!(session.team, ["snorlax", "gyarados"]);
    }
}
