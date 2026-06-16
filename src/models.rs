//! Core domain layer: clean, API-agnostic data structures.
//!
//! Nothing in this module knows about PokeAPI's JSON wire format; the API
//! client (see `api.rs`) is responsible for translating raw responses into
//! these types. This keeps the rest of the application decoupled from the
//! quirks of the upstream service.

/// A single entry in the master Pokemon list shown in the sidebar.
#[derive(Debug, Clone)]
pub struct PokemonEntry {
    /// API identifier, e.g. `"pikachu"` (lowercase, possibly hyphenated).
    pub name: String,
}

/// The six canonical base stats every Pokemon has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKind {
    Hp,
    Attack,
    Defense,
    SpecialAttack,
    SpecialDefense,
    Speed,
}

impl StatKind {
    /// Maps PokeAPI's stat slug (`"special-attack"`, etc.) to a [`StatKind`].
    pub fn from_api(slug: &str) -> Option<Self> {
        match slug {
            "hp" => Some(Self::Hp),
            "attack" => Some(Self::Attack),
            "defense" => Some(Self::Defense),
            "special-attack" => Some(Self::SpecialAttack),
            "special-defense" => Some(Self::SpecialDefense),
            "speed" => Some(Self::Speed),
            _ => None,
        }
    }

    /// Stable display order so stats always render top-to-bottom consistently.
    pub fn order(&self) -> u8 {
        match self {
            Self::Hp => 0,
            Self::Attack => 1,
            Self::Defense => 2,
            Self::SpecialAttack => 3,
            Self::SpecialDefense => 4,
            Self::Speed => 5,
        }
    }
}

/// A single base stat value (0..=255 in practice).
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub kind: StatKind,
    pub base: u16,
}

/// Fully resolved details for one Pokemon, ready to render.
#[derive(Debug, Clone)]
pub struct PokemonDetail {
    pub id: u32,
    /// Raw API name (lowercase). Use [`title_case`] for display.
    pub name: String,
    pub types: Vec<String>,
    pub stats: Vec<Stat>,
    /// Height in decimetres, as returned by the API.
    pub height: u32,
    /// Weight in hectograms, as returned by the API.
    pub weight: u32,
}

impl PokemonDetail {
    /// Sum of all base stats — a common "power level" heuristic.
    pub fn stat_total(&self) -> u32 {
        self.stats.iter().map(|s| s.base as u32).sum()
    }
}

/// A node in a parsed evolution chain.
///
/// PokeAPI returns evolution data as a recursively nested structure where each
/// species can evolve into zero or more others. We mirror that as an n-ary
/// tree so branching evolutions (Eevee, Tyrogue, Wurmple, ...) are represented
/// naturally.
#[derive(Debug, Clone)]
pub struct EvolutionTree {
    /// Raw API name of the species at this node.
    pub name: String,
    pub children: Vec<EvolutionTree>,
}

/// Turns a raw API name like `"mr-mime"` into a display label `"Mr Mime"`.
pub fn title_case(raw: &str) -> String {
    raw.split(['-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
