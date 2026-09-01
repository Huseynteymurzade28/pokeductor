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

/// A property of a species that the master list cannot answer on its own.
///
/// The list response carries a name and an id and nothing else, so a filter
/// like `type:ghost` has to be resolved against a *roster*: the membership
/// list an endpoint returns when asked which species carry that property. One
/// request answers a whole roster, and rosters only ever grow with a new
/// generation, so each is fetched at most once per install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RosterKind {
    /// `type:ghost` — every Pokemon of that type.
    Type,
    /// `ability:levitate` — every Pokemon that can have that ability, hidden
    /// slots included.
    Ability,
    /// `egg:dragon` — every species in that breeding group.
    EggGroup,
}

/// One roster filter: a kind and the value asked of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RosterTerm {
    pub kind: RosterKind,
    /// The API slug, lowercased — `"ghost"`, `"levitate"`, `"monster"`.
    pub value: String,
}

impl RosterTerm {
    pub fn new(kind: RosterKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
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

/// One of a species' possible abilities.
///
/// A Pokemon lists every ability it *can* have; in a given game it carries
/// exactly one of them. The hidden one is only obtainable by special means,
/// which is why it is worth flagging separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    /// API slug, e.g. `"cursed-body"`.
    pub name: String,
    pub is_hidden: bool,
}

/// The localized text for one ability, fetched on demand from its own endpoint.
///
/// Mirrors the shape of [`PokemonDetail`]'s genus/flavor maps: keyed by PokeAPI
/// language code, with English as the universal fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityInfo {
    /// API slug, matching [`Ability::name`].
    pub name: String,
    /// Display name per language, e.g. `de -> "Schwebe"`.
    pub names: HashMap<String, String>,
    /// Short description per language.
    pub flavors: HashMap<String, String>,
}

impl AbilityInfo {
    /// Display name in the requested language, falling back to English and
    /// then to a title-cased slug.
    pub fn name_for(&self, code: &str) -> String {
        self.names
            .get(code)
            .or_else(|| self.names.get("en"))
            .cloned()
            .unwrap_or_else(|| title_case(&self.name))
    }

    /// Description in the requested language, falling back to English.
    pub fn flavor_for(&self, code: &str) -> Option<&str> {
        self.flavors
            .get(code)
            .or_else(|| self.flavors.get("en"))
            .map(String::as_str)
    }
}

/// Which palette of a species' front artwork to show.
///
/// This is app-wide state rather than a property of a species: the shiny toggle
/// stays on while the user moves through the list, so a whole evolution chain
/// can be inspected in its shiny colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SpriteVariant {
    #[default]
    Normal,
    Shiny,
}

impl SpriteVariant {
    /// The other palette, for the toggle key.
    pub fn toggled(self) -> Self {
        match self {
            Self::Normal => Self::Shiny,
            Self::Shiny => Self::Normal,
        }
    }

    pub fn is_shiny(self) -> bool {
        matches!(self, Self::Shiny)
    }

    /// Filename infix that keeps a shiny PNG from overwriting the normal one in
    /// the on-disk cache. Empty for [`Normal`](Self::Normal), so sprites cached
    /// before shinies existed stay valid.
    pub fn file_suffix(self) -> &'static str {
        match self {
            Self::Normal => "",
            Self::Shiny => ".shiny",
        }
    }
}

/// Fully resolved details for one Pokemon, ready to render.
/// How a Pokemon comes to know a move, in the games this app reports on.
///
/// PokeAPI names a few more — `form-change`, `train`, the transfer-only ones —
/// which say nothing about a learnset and are dropped on the way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearnMethod {
    /// Learned on levelling up, at [`LearnedMove::level`].
    LevelUp,
    /// Taught from a TM/HM.
    Machine,
    /// Inherited by breeding.
    Egg,
    /// Taught by a move tutor.
    Tutor,
}

impl LearnMethod {
    pub fn from_api(slug: &str) -> Option<Self> {
        match slug {
            "level-up" => Some(LearnMethod::LevelUp),
            "machine" => Some(LearnMethod::Machine),
            "egg" => Some(LearnMethod::Egg),
            "tutor" => Some(LearnMethod::Tutor),
            _ => None,
        }
    }

    /// Order the methods are grouped in on the moves card: the level-up set
    /// first, because it is the one that describes the species rather than the
    /// player's bag.
    pub fn order(self) -> u8 {
        match self {
            LearnMethod::LevelUp => 0,
            LearnMethod::Egg => 1,
            LearnMethod::Machine => 2,
            LearnMethod::Tutor => 3,
        }
    }
}

/// One move in a species' learnset, as the species record carries it: which
/// move, how it is learned, and at what level when that is the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnedMove {
    /// API slug, e.g. `"shadow-ball"`.
    pub name: String,
    pub method: LearnMethod,
    /// Level it is learned at, for [`LearnMethod::LevelUp`] only. Zero
    /// elsewhere — and for the moves a species starts out knowing, which the
    /// games and PokeAPI both record as level zero.
    pub level: u32,
}

/// A move's own record, fetched per move rather than per species: a learnset
/// runs to eighty entries or more, and asking for all of them to open a card
/// nobody may scroll would cost eighty requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveInfo {
    /// API slug, matching [`LearnedMove::name`].
    pub name: String,
    /// Display name per language, e.g. `de -> "Spukball"`.
    pub names: HashMap<String, String>,
    /// Short description per language.
    pub flavors: HashMap<String, String>,
    /// Elemental type, lowercased — the same vocabulary as
    /// [`PokemonDetail::types`], so the type palette applies unchanged.
    pub type_name: String,
    /// `"physical"`, `"special"` or `"status"`.
    pub damage_class: String,
    /// Absent for status moves, and for the few whose power is situational.
    pub power: Option<u16>,
    /// Absent for the moves that cannot miss.
    pub accuracy: Option<u16>,
    pub pp: Option<u16>,
}

impl MoveInfo {
    /// Display name in the requested language, falling back to English and
    /// then to a title-cased slug.
    pub fn name_for(&self, code: &str) -> String {
        self.names
            .get(code)
            .or_else(|| self.names.get("en"))
            .cloned()
            .unwrap_or_else(|| title_case(&self.name))
    }

    /// Description in the requested language, falling back to English.
    pub fn flavor_for(&self, code: &str) -> Option<&str> {
        self.flavors
            .get(code)
            .or_else(|| self.flavors.get("en"))
            .map(String::as_str)
    }
}

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
    /// Every ability the species can have, in PokeAPI slot order.
    pub abilities: Vec<Ability>,
    pub stats: Vec<Stat>,
    /// Height in decimetres, as returned by the API.
    pub height: u32,
    /// Weight in hectograms, as returned by the API.
    pub weight: u32,
    /// URL of the front-facing PNG artwork, if the species has one.
    pub sprite_url: Option<String>,
    /// URL of the same artwork in the shiny palette. Absent for the handful of
    /// species PokeAPI ships no shiny sprite for.
    pub shiny_sprite_url: Option<String>,
    /// Pokedex genus (e.g. `"Seed Pokémon"`) keyed by PokeAPI language code.
    pub genera: HashMap<String, String>,
    /// Pokedex flavor-text blurbs, cleaned of control characters, keyed by
    /// PokeAPI language code.
    pub flavors: HashMap<String, String>,
    /// The learnset from the newest games this species appears in, grouped by
    /// method and then in reading order within each group. Carried on the
    /// species record because `/pokemon/{name}` already answers with it — the
    /// moves card costs no request of its own to open.
    pub moves: Vec<LearnedMove>,
    /// Which games [`moves`](Self::moves) is the learnset from, as PokeAPI's
    /// version-group slug (`"scarlet-violet"`). Shown on the card, because a
    /// learnset means little without knowing which games it belongs to.
    pub learnset_games: Option<String>,
}

impl PokemonDetail {
    /// Sum of all base stats — a common "power level" heuristic.
    pub fn stat_total(&self) -> u32 {
        self.stats.iter().map(|s| s.base as u32).sum()
    }

    /// Artwork URL in the requested palette, falling back to the normal one for
    /// a species with no shiny art — a familiar sprite beats an empty card.
    pub fn sprite_url_for(&self, variant: SpriteVariant) -> Option<&str> {
        match variant {
            SpriteVariant::Normal => self.sprite_url.as_deref(),
            SpriteVariant::Shiny => self
                .shiny_sprite_url
                .as_deref()
                .or(self.sprite_url.as_deref()),
        }
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
        1 + self
            .children
            .iter()
            .map(EvolutionTree::depth)
            .max()
            .unwrap_or(0)
    }
}

/// A decoded Pokemon sprite, stored as raw RGBA pixels ready to be rendered
/// in the terminal with Unicode half-blocks.
///
/// Sprites are tiny (PokeAPI's `front_default` is 96×96), so we keep the full
/// image in memory and downsample at draw time to whatever space is available.
/// The fields are private because [`bounds`](Self::content_bounds) is derived
/// from the pixels: letting anything rewrite them after the fact would leave a
/// crop box describing an image that no longer exists.
#[derive(Debug, Clone)]
pub struct Sprite {
    width: u32,
    height: u32,
    /// Row-major RGBA, four bytes per pixel.
    pixels: Vec<[u8; 4]>,
    /// Tight bounding box of the opaque pixels, computed once here rather than
    /// on every frame. See [`content_bounds`](Self::content_bounds).
    bounds: (u32, u32, u32, u32),
}

impl Sprite {
    /// Decodes into a sprite, working out the crop box as it goes.
    ///
    /// The box is a property of the pixels and nothing else, so computing it
    /// here costs one pass over an image we have just finished decoding
    /// anyway — against a full 96x96 scan per sprite per frame, which is what
    /// it replaces.
    pub fn new(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        let bounds = compute_content_bounds(width, height, &pixels);
        Sprite {
            width,
            height,
            pixels,
            bounds,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Row-major RGBA, for re-encoding the image on its way to the cache.
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

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
    /// this box lets the visible Pokemon fill its on-screen cell.
    ///
    /// Read straight off the struct. It used to be a full scan of the image,
    /// run afresh every time the sprite was drawn — which meant once per frame
    /// per sprite, and a frame showing an evolution chain draws ten of them.
    /// Nothing about the answer depends on anything that changes between
    /// frames, so it is worked out once in [`Sprite::new`] instead.
    pub fn content_bounds(&self) -> (u32, u32, u32, u32) {
        self.bounds
    }
}

/// The scan behind [`Sprite::content_bounds`], as a free function so it can run
/// before there is a `Sprite` to call it on. Falls back to the whole image when
/// nothing is opaque, which keeps a fully transparent sprite renderable rather
/// than making it a special case downstream.
fn compute_content_bounds(width: u32, height: u32, pixels: &[[u8; 4]]) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0u32, 0u32);
    let mut found = false;
    for y in 0..height {
        for x in 0..width {
            if pixels[(y * width + x) as usize][3] >= 128 {
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
        (0, 0, width.saturating_sub(1), height.saturating_sub(1))
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

    /// A sprite with one opaque rectangle inside a transparent margin, which is
    /// the shape every PokeAPI sprite has.
    fn framed_sprite(width: u32, height: u32, box_: (u32, u32, u32, u32)) -> Sprite {
        let (x0, y0, x1, y1) = box_;
        let pixels = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                let opaque = (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
                [10, 20, 30, if opaque { 255 } else { 0 }]
            })
            .collect();
        Sprite::new(width, height, pixels)
    }

    #[test]
    fn the_stored_crop_box_is_the_one_a_scan_would_have_found() {
        for box_ in [(18, 13, 77, 82), (0, 0, 95, 95), (40, 40, 41, 41)] {
            let sprite = framed_sprite(96, 96, box_);
            assert_eq!(sprite.content_bounds(), box_);
            assert_eq!(
                sprite.content_bounds(),
                compute_content_bounds(sprite.width(), sprite.height(), sprite.pixels()),
                "the box on the struct must not drift from the pixels it describes"
            );
        }
    }

    #[test]
    fn a_sprite_with_nothing_opaque_falls_back_to_the_whole_image() {
        let sprite = Sprite::new(4, 3, vec![[0, 0, 0, 0]; 12]);
        assert_eq!(sprite.content_bounds(), (0, 0, 3, 2));
        assert_eq!(
            sprite.content_bounds(),
            compute_content_bounds(4, 3, sprite.pixels())
        );
    }

    #[test]
    fn half_transparent_pixels_do_not_count_towards_the_crop() {
        // The scan's threshold is alpha >= 128, so a faint edge does not drag
        // the box back out to the margin it was cropped away from.
        let mut pixels = vec![[0u8, 0, 0, 0]; 16];
        pixels[5] = [10, 20, 30, 255];
        pixels[0] = [10, 20, 30, 127];
        let sprite = Sprite::new(4, 4, pixels);
        assert_eq!(sprite.content_bounds(), (1, 1, 1, 1));
    }

    fn entry(id: u32) -> PokemonEntry {
        PokemonEntry {
            name: String::new(),
            id,
        }
    }

    #[test]
    fn generation_boundaries_land_on_the_right_side() {
        // First and last species of every generation.
        for (id, gen) in [
            (1, 1),
            (151, 1),
            (152, 2),
            (251, 2),
            (252, 3),
            (386, 3),
            (387, 4),
            (493, 4),
            (494, 5),
            (649, 5),
            (650, 6),
            (721, 6),
            (722, 7),
            (809, 7),
            (810, 8),
            (905, 8),
            (906, 9),
            (1025, 9),
        ] {
            assert_eq!(entry(id).generation(), Some(gen), "dex #{id}");
        }
    }

    fn detail_with_sprites(normal: Option<&str>, shiny: Option<&str>) -> PokemonDetail {
        PokemonDetail {
            name: "pikachu".into(),
            species: "pikachu".into(),
            dex_number: 25,
            is_legendary: false,
            is_mythical: false,
            is_baby: false,
            types: Vec::new(),
            abilities: Vec::new(),
            stats: Vec::new(),
            height: 0,
            weight: 0,
            sprite_url: normal.map(str::to_string),
            shiny_sprite_url: shiny.map(str::to_string),
            genera: HashMap::new(),
            flavors: HashMap::new(),
            moves: Vec::new(),
            learnset_games: None,
        }
    }

    #[test]
    fn shiny_artwork_falls_back_to_the_normal_palette() {
        let both = detail_with_sprites(Some("front.png"), Some("shiny.png"));
        assert_eq!(both.sprite_url_for(SpriteVariant::Shiny), Some("shiny.png"));
        assert_eq!(
            both.sprite_url_for(SpriteVariant::Normal),
            Some("front.png")
        );

        // No shiny art: show the normal sprite rather than an empty card.
        let normal_only = detail_with_sprites(Some("front.png"), None);
        assert_eq!(
            normal_only.sprite_url_for(SpriteVariant::Shiny),
            Some("front.png")
        );

        // No art at all stays "no art" in either palette.
        let neither = detail_with_sprites(None, None);
        assert_eq!(neither.sprite_url_for(SpriteVariant::Shiny), None);
    }

    #[test]
    fn alternate_forms_have_no_dex_number_or_generation() {
        let alolan_raichu = entry(10100);
        assert_eq!(alolan_raichu.dex_number(), None);
        assert_eq!(alolan_raichu.generation(), None);
    }
}
