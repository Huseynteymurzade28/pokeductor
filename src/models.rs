//! Core domain layer: clean, API-agnostic data structures.
//!
//! Nothing in this module knows about PokeAPI's JSON wire format; the API
//! client (see `api.rs`) is responsible for translating raw responses into
//! these types. This keeps the rest of the application decoupled from the
//! quirks of the upstream service.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single entry in the master Pokemon list shown in the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PokemonEntry {
    /// API identifier, e.g. `"pikachu"` (lowercase, possibly hyphenated).
    pub name: String,
    /// PokeAPI's numeric id, read straight out of the list response's URL so
    /// the sidebar can show a dex number without fetching anything. For the
    /// default form of a species this *is* its National Pokedex number;
    /// alternate forms (Alolan Raichu and friends) are numbered from 10001 up.
    pub id: u32,
}

impl PokemonEntry {
    /// The National Pokedex number to display, or `None` for an alternate form
    /// whose id carries no dex meaning.
    pub fn dex_number(&self) -> Option<u32> {
        (self.id <= MAX_DEX_NUMBER).then_some(self.id)
    }

    /// Which generation introduced this species, derived from its dex number.
    ///
    /// Alternate forms return `None`: their ids sit in a separate range that
    /// says nothing about the species they belong to, and resolving that would
    /// cost a request per form.
    pub fn generation(&self) -> Option<u8> {
        let dex = self.dex_number()?;
        GENERATION_RANGES
            .iter()
            .position(|&last| dex <= last)
            .map(|idx| idx as u8 + 1)
    }
}

/// Highest National Pokedex number PokeAPI currently carries.
const MAX_DEX_NUMBER: u32 = 1025;

/// Last dex number of each generation, in order. These boundaries are fixed
/// history — a released generation never gains or loses species — so deriving
/// the generation locally beats spending a request on it.
const GENERATION_RANGES: [u32; 9] = [151, 251, 386, 493, 649, 721, 809, 905, MAX_DEX_NUMBER];

/// The six canonical base stats every Pokemon has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stat {
    pub kind: StatKind,
    pub base: u16,
}

/// Fully resolved details for one Pokemon, ready to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PokemonDetail {
    /// Raw API name (lowercase). Use [`title_case`] for display.
    pub name: String,
    /// Base species slug, which can differ from `name` for alternate forms
    /// (e.g. `name = "raichu-alola"` but `species = "raichu"`). This is the key
    /// the species and evolution endpoints expect.
    pub species: String,
    /// National Pokedex number (from the species record), which is stable across
    /// a species' alternate forms — unlike [`id`](Self::id).
    pub dex_number: u32,
    /// Whether the species is flagged Legendary / Mythical / a baby Pokemon.
    pub is_legendary: bool,
    pub is_mythical: bool,
    pub is_baby: bool,
    pub types: Vec<String>,
    pub stats: Vec<Stat>,
    /// Height in decimetres, as returned by the API.
    pub height: u32,
    /// Weight in hectograms, as returned by the API.
    pub weight: u32,
    /// URL of the front-facing PNG artwork, if the species has one.
    pub sprite_url: Option<String>,
    /// Pokedex genus (e.g. `"Seed Pokémon"`) keyed by PokeAPI language code.
    pub genera: HashMap<String, String>,
    /// Pokedex flavor-text blurbs, cleaned of control characters, keyed by
    /// PokeAPI language code.
    pub flavors: HashMap<String, String>,
}

impl PokemonDetail {
    /// Sum of all base stats — a common "power level" heuristic.
    pub fn stat_total(&self) -> u32 {
        self.stats.iter().map(|s| s.base as u32).sum()
    }

    /// Genus in the requested language, falling back to English when that
    /// language has no entry (PokeAPI has no Turkish text, for instance).
    pub fn genus_for(&self, code: &str) -> Option<&str> {
        self.genera
            .get(code)
            .or_else(|| self.genera.get("en"))
            .map(String::as_str)
    }
}

/// What sets an evolution in motion. PokeAPI has a long tail of one-off
/// triggers (spin, three-critical-hits, ...), so anything beyond the four
/// common ones is carried through verbatim and displayed as-is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvolutionTrigger {
    LevelUp,
    Trade,
    UseItem,
    Shed,
    Other(String),
}

impl EvolutionTrigger {
    pub fn from_api(slug: &str) -> Self {
        match slug {
            "level-up" => Self::LevelUp,
            "trade" => Self::Trade,
            "use-item" => Self::UseItem,
            "shed" => Self::Shed,
            other => Self::Other(other.to_string()),
        }
    }
}

/// The requirements for one species to evolve into another.
///
/// Every field is optional because the games layer conditions freely: Umbreon
/// needs happiness *and* night, Milotic needs beauty, Shedinja needs a free
/// party slot. The renderer turns whichever fields are set into readable text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionCondition {
    pub trigger: Option<EvolutionTrigger>,
    pub min_level: Option<u32>,
    /// Item used on the Pokemon (e.g. `"water-stone"`).
    pub item: Option<String>,
    /// Item the Pokemon must be holding.
    pub held_item: Option<String>,
    pub known_move: Option<String>,
    pub known_move_type: Option<String>,
    pub min_happiness: Option<u32>,
    pub min_affection: Option<u32>,
    pub min_beauty: Option<u32>,
    /// `"day"`, `"night"` or `"dusk"`.
    pub time_of_day: Option<String>,
    pub location: Option<String>,
    /// PokeAPI gender id: 1 = female, 2 = male.
    pub gender: Option<u8>,
    pub needs_overworld_rain: bool,
    pub turn_upside_down: bool,
    /// The species that must be traded for (Karrablast ↔ Shelmet).
    pub trade_species: Option<String>,
    pub party_species: Option<String>,
    pub party_type: Option<String>,
    /// Attack compared to Defense: 1 = greater, 0 = equal, -1 = less (Tyrogue).
    pub relative_physical_stats: Option<i8>,
}

/// A node in a parsed evolution chain.
///
/// PokeAPI returns evolution data as a recursively nested structure where each
/// species can evolve into zero or more others. We mirror that as an n-ary
/// tree so branching evolutions (Eevee, Tyrogue, Wurmple, ...) are represented
/// naturally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionTree {
    /// Raw API name of the species at this node.
    pub name: String,
    /// How the *parent* evolves into this node. `None` at the root of a chain,
    /// which nothing evolves into.
    pub condition: Option<EvolutionCondition>,
    pub children: Vec<EvolutionTree>,
}

impl EvolutionTree {
    /// Collects every species name in the chain (depth-first) into `out`.
    pub fn collect_names(&self, out: &mut Vec<String>) {
        out.push(self.name.clone());
        for child in &self.children {
            child.collect_names(out);
        }
    }

    /// Number of leaf species — i.e. how many vertical lanes a sprite layout
    /// needs to give every branch its own row.
    pub fn leaf_count(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            self.children.iter().map(EvolutionTree::leaf_count).sum()
        }
    }

    /// Finds the node for `name` anywhere in the chain. Species appear at most
    /// once per chain, so the first match is the only match.
    pub fn find(&self, name: &str) -> Option<&EvolutionTree> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(name))
    }

    /// Length of the longest evolution path (number of stages), e.g. 3 for
    /// Bulbasaur → Ivysaur → Venusaur.
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(EvolutionTree::depth).max().unwrap_or(0)
    }
}

/// A decoded Pokemon sprite, stored as raw RGBA pixels ready to be rendered
/// in the terminal with Unicode half-blocks.
///
/// Sprites are tiny (PokeAPI's `front_default` is 96×96), so we keep the full
/// image in memory and downsample at draw time to whatever space is available.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, four bytes per pixel.
    pub pixels: Vec<[u8; 4]>,
}

impl Sprite {
    /// Average RGBA over the source box `[x0..=x1] × [y0..=y1]`, weighting color
    /// by alpha so transparent pixels don't muddy the result. The returned alpha
    /// is the box's mean coverage. Averaging (rather than nearest-neighbour point
    /// sampling) is what keeps downscaled sprites smooth instead of leaving the
    /// hard black outline pixels as ragged lines.
    pub fn box_average(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> [u8; 4] {
        let x1 = x1.min(self.width.saturating_sub(1)).max(x0);
        let y1 = y1.min(self.height.saturating_sub(1)).max(y0);
        let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = self.pixels[(y * self.width + x) as usize];
                let pa = p[3] as u32;
                r += p[0] as u32 * pa;
                g += p[1] as u32 * pa;
                b += p[2] as u32 * pa;
                a += pa;
                n += 1;
            }
        }
        if a == 0 || n == 0 {
            return [0, 0, 0, 0];
        }
        [(r / a) as u8, (g / a) as u8, (b / a) as u8, (a / n) as u8]
    }

    /// Tight bounding box `(x0, y0, x1, y1)` (inclusive) of the non-transparent
    /// pixels. PokeAPI artwork sits in a large transparent margin; cropping to
    /// this box lets the visible Pokemon fill its on-screen cell. Falls back to
    /// the full image if nothing is opaque.
    pub fn content_bounds(&self) -> (u32, u32, u32, u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (self.width, self.height, 0u32, 0u32);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.pixels[(y * self.width + x) as usize][3] >= 128 {
                    found = true;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if found {
            (x0, y0, x1, y1)
        } else {
            (0, 0, self.width.saturating_sub(1), self.height.saturating_sub(1))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u32) -> PokemonEntry {
        PokemonEntry { name: String::new(), id }
    }

    #[test]
    fn generation_boundaries_land_on_the_right_side() {
        // First and last species of every generation.
        for (id, gen) in [
            (1, 1), (151, 1),
            (152, 2), (251, 2),
            (252, 3), (386, 3),
            (387, 4), (493, 4),
            (494, 5), (649, 5),
            (650, 6), (721, 6),
            (722, 7), (809, 7),
            (810, 8), (905, 8),
            (906, 9), (1025, 9),
        ] {
            assert_eq!(entry(id).generation(), Some(gen), "dex #{id}");
        }
    }

    #[test]
    fn alternate_forms_have_no_dex_number_or_generation() {
        let alolan_raichu = entry(10100);
        assert_eq!(alolan_raichu.dex_number(), None);
        assert_eq!(alolan_raichu.generation(), None);
    }
}
