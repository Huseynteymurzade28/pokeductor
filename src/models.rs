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
    /// Base species slug, which can differ from `name` for alternate forms
    /// (e.g. `name = "raichu-alola"` but `species = "raichu"`). This is the key
    /// the species and evolution endpoints expect.
    pub species: String,
    pub types: Vec<String>,
    pub stats: Vec<Stat>,
    /// Height in decimetres, as returned by the API.
    pub height: u32,
    /// Weight in hectograms, as returned by the API.
    pub weight: u32,
    /// URL of the front-facing PNG artwork, if the species has one.
    pub sprite_url: Option<String>,
    /// Pokedex genus, e.g. `"Seed Pokémon"`, if the species lists one.
    pub genus: Option<String>,
    /// A short Pokedex flavor-text blurb, cleaned of control characters.
    pub flavor: Option<String>,
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
