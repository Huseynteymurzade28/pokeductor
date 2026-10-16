//! Static type-effectiveness charts, one per era of the games.
//!
//! This is pure, offline data: no PokeAPI round-trip is needed to answer "what
//! is this Pokemon weak to?", so the matchup card opens instantly for any
//! species whose types are already known.
//!
//! The chart is expressed from the *attacker's* point of view — [`Chart::effectiveness`]
//! answers "how much damage does a move of type A deal to a Pokemon of type B?"
//! — and the defensive view is derived by multiplying across the defender's
//! types, exactly as the games do.
//!
//! The games have used three charts. Generation II added Dark and Steel and
//! fixed four Generation I cells; Generation VI added Fairy and took away
//! Steel's resistance to Ghost and Dark. Every other cell has held since Red
//! and Blue, so the older charts are written as their differences from the
//! current one rather than as two more full tables to keep in step.

/// Every type slug, in the canonical Pokedex order PokeAPI uses.
///
/// The order is also the order the types arrived in, which is what lets
/// [`Chart::types`] take a prefix: the first fifteen are Generation I's, Dark
/// and Steel came next, and Fairy last.
pub const TYPES: [&str; 18] = [
    "normal", "fire", "water", "electric", "grass", "ice", "fighting", "poison", "ground",
    "flying", "psychic", "bug", "rock", "ghost", "dragon", "dark", "steel", "fairy",
];

/// One of the three type charts the games have used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chart {
    /// Red, Blue and Yellow.
    Gen1,
    /// Gold and Silver through Black 2 and White 2.
    Gen2To5,
    /// X and Y onwards.
    Gen6On,
}

impl Chart {
    /// The types that exist under this chart, in canonical order.
    pub fn types(self) -> &'static [&'static str] {
        match self {
            Chart::Gen1 => &TYPES[..15],
            Chart::Gen2To5 => &TYPES[..17],
            Chart::Gen6On => &TYPES,
        }
    }

    /// Whether `slug` is a type under this chart.
    pub fn has(self, slug: &str) -> bool {
        self.types().contains(&slug)
    }

    /// Damage multiplier for a move of type `attacker` hitting a *single* type
    /// `defender`.
    ///
    /// A type the chart does not have is neutral on either side. Nothing in
    /// the games can produce that pairing, so there is no right answer to
    /// give, and 1× is the one that leaves the other half of a dual type to
    /// decide the product.
    pub fn effectiveness(self, attacker: &str, defender: &str) -> f32 {
        if !self.has(attacker) || !self.has(defender) {
            return 1.0;
        }
        match (self, attacker, defender) {
            // Generation I's four cells that Generation II changed. Bug and
            // Poison were each other's weakness, Ghost could not touch the
            // Psychic types it was meant to answer (a bug, but the one the
            // games shipped), and Fire took Ice moves at full strength.
            (Chart::Gen1, "bug", "poison") => 2.0,
            (Chart::Gen1, "poison", "bug") => 2.0,
            (Chart::Gen1, "ghost", "psychic") => 0.0,
            (Chart::Gen1, "ice", "fire") => 1.0,
            // Steel resisted Ghost and Dark until Generation VI. Generation I
            // has no Steel or Dark, so the guard above has already answered it.
            (Chart::Gen2To5, "ghost" | "dark", "steel") => 0.5,
            _ => current(attacker, defender),
        }
    }
}

/// The newest generation the charts know about. Also the default: a reader who
/// has not asked for an older game is reading about the current ones.
pub const LATEST_GENERATION: u8 = 9;

/// The generation whose rules the matchup, party and head-to-head cards read.
///
/// A generation rather than a [`Chart`] because the chart is not all that
/// changes. Rotom's forms were Electric/Ghost all through Generation IV and
/// took their second type in Generation V, both under the same chart, and
/// abilities, which can overrule a chart outright, did not exist before
/// Generation III.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(u8);

impl Generation {
    pub const LATEST: Generation = Generation(LATEST_GENERATION);

    /// `number` as a generation, if it is one the games have had.
    pub fn new(number: u8) -> Option<Self> {
        (1..=LATEST_GENERATION)
            .contains(&number)
            .then_some(Generation(number))
    }

    pub fn number(self) -> u8 {
        self.0
    }

    /// The chart this generation's games use.
    pub fn chart(self) -> Chart {
        match self.0 {
            1 => Chart::Gen1,
            2..=5 => Chart::Gen2To5,
            _ => Chart::Gen6On,
        }
    }

    /// Whether Pokemon had abilities yet. They arrived in Ruby and Sapphire.
    pub fn has_abilities(self) -> bool {
        self.0 >= 3
    }

    /// The next generation, wrapping from the newest back to the first, so one
    /// key walks through all of them.
    pub fn next(self) -> Self {
        Generation(self.0 % LATEST_GENERATION + 1)
    }

    /// The previous generation, wrapping from the first to the newest.
    pub fn previous(self) -> Self {
        Generation((self.0 + LATEST_GENERATION - 2) % LATEST_GENERATION + 1)
    }

    /// The generation in roman numerals, as the games and every reference
    /// write it. Language-neutral.
    pub fn roman(self) -> &'static str {
        ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"][self.0 as usize - 1]
    }
}

impl Default for Generation {
    fn default() -> Self {
        Generation::LATEST
    }
}

/// The current chart. Only the non-neutral pairings are listed; everything
/// else is 1×.
fn current(attacker: &str, defender: &str) -> f32 {
    match (attacker, defender) {
        ("normal", "rock" | "steel") => 0.5,
        ("normal", "ghost") => 0.0,

        ("fire", "fire" | "water" | "rock" | "dragon") => 0.5,
        ("fire", "grass" | "ice" | "bug" | "steel") => 2.0,

        ("water", "water" | "grass" | "dragon") => 0.5,
        ("water", "fire" | "ground" | "rock") => 2.0,

        ("electric", "electric" | "grass" | "dragon") => 0.5,
        ("electric", "ground") => 0.0,
        ("electric", "water" | "flying") => 2.0,

        ("grass", "fire" | "grass" | "poison" | "flying" | "bug" | "dragon" | "steel") => 0.5,
        ("grass", "water" | "ground" | "rock") => 2.0,

        ("ice", "fire" | "water" | "ice" | "steel") => 0.5,
        ("ice", "grass" | "ground" | "flying" | "dragon") => 2.0,

        ("fighting", "poison" | "flying" | "psychic" | "bug" | "fairy") => 0.5,
        ("fighting", "ghost") => 0.0,
        ("fighting", "normal" | "ice" | "rock" | "dark" | "steel") => 2.0,

        ("poison", "poison" | "ground" | "rock" | "ghost") => 0.5,
        ("poison", "steel") => 0.0,
        ("poison", "grass" | "fairy") => 2.0,

        ("ground", "grass" | "bug") => 0.5,
        ("ground", "flying") => 0.0,
        ("ground", "fire" | "electric" | "poison" | "rock" | "steel") => 2.0,

        ("flying", "electric" | "rock" | "steel") => 0.5,
        ("flying", "grass" | "fighting" | "bug") => 2.0,

        ("psychic", "psychic" | "steel") => 0.5,
        ("psychic", "dark") => 0.0,
        ("psychic", "fighting" | "poison") => 2.0,

        ("bug", "fire" | "fighting" | "poison" | "flying" | "ghost" | "steel" | "fairy") => 0.5,
        ("bug", "grass" | "psychic" | "dark") => 2.0,

        ("rock", "fighting" | "ground" | "steel") => 0.5,
        ("rock", "fire" | "ice" | "flying" | "bug") => 2.0,

        ("ghost", "dark") => 0.5,
        ("ghost", "normal") => 0.0,
        ("ghost", "psychic" | "ghost") => 2.0,

        ("dragon", "steel") => 0.5,
        ("dragon", "fairy") => 0.0,
        ("dragon", "dragon") => 2.0,

        ("dark", "fighting" | "dark" | "fairy") => 0.5,
        ("dark", "psychic" | "ghost") => 2.0,

        ("steel", "fire" | "water" | "electric" | "steel") => 0.5,
        ("steel", "ice" | "rock" | "fairy") => 2.0,

        ("fairy", "fire" | "poison" | "steel") => 0.5,
        ("fairy", "fighting" | "dragon" | "dark") => 2.0,

        _ => 1.0,
    }
}

/// Multiplier a move of type `attacker` deals to a Pokemon carrying
/// `defender_types` under `chart` — the product of the per-type multipliers,
/// so a dual type can land anywhere from 0× to 4×.
pub fn combined(chart: Chart, attacker: &str, defender_types: &[String]) -> f32 {
    defender_types
        .iter()
        .map(|d| chart.effectiveness(attacker, d))
        .product()
}

/// One row of the matchup card: every attacking type that lands on the same
/// multiplier, e.g. `×4 → [rock]`.
#[derive(Debug, Clone)]
pub struct MatchupGroup {
    /// Display label for the multiplier, e.g. `"×½"`. Language-neutral.
    pub label: &'static str,
    /// Attacking types that hit for this multiplier, in canonical order.
    pub types: Vec<&'static str>,
}

/// Multipliers a dual type can produce, in the order the card lists them:
/// worst news for the defender first.
const BUCKETS: [(f32, &str); 5] = [
    (4.0, "×4"),
    (2.0, "×2"),
    (0.5, "×½"),
    (0.25, "×¼"),
    (0.0, "×0"),
];

/// Groups every attacking type by how much damage it deals to a Pokemon with
/// `defender_types`. Neutral (1×) matchups are omitted — they are the default
/// and listing them would bury the interesting rows. Empty groups are dropped,
/// so the caller can render the result directly.
///
/// Every type in `immune_to` lands in the ×0 row whatever the chart makes of
/// it. The chart knows typings and nothing else, and an ability can overrule
/// it outright: Ground is ×2 on an Electric/Ghost body and 0× on Rotom, which
/// carries that body and Levitate. Taking the overrides here rather than
/// letting callers patch the rows afterwards is what guarantees a type moved
/// into ×0 also leaves the row it came from, instead of being listed twice
/// with two different answers.
///
/// Only immunities the Pokemon certainly has belong in `immune_to`. One it
/// might not have is not a multiplier at all, and saying so is the caller's
/// job.
///
/// Only the types `chart` has are listed: a card about Generation I has no
/// Steel row to put anything in.
pub fn defensive_groups(
    chart: Chart,
    defender_types: &[String],
    immune_to: &[&str],
) -> Vec<MatchupGroup> {
    BUCKETS
        .iter()
        .filter_map(|&(multiplier, label)| {
            let types: Vec<&'static str> = chart
                .types()
                .iter()
                .copied()
                .filter(|attacker| match immune_to.contains(attacker) {
                    true => same_multiplier(multiplier, 0.0),
                    false => same_multiplier(combined(chart, attacker, defender_types), multiplier),
                })
                .collect();
            (!types.is_empty()).then_some(MatchupGroup { label, types })
        })
        .collect()
}

/// Display label for a multiplier, e.g. `"×½"` — the same wording the matchup
/// card's rows carry, and language-neutral like them.
///
/// Neutral is spelled out here, unlike in [`defensive_groups`], which omits it:
/// a group of neutral matchups is the default and says nothing, but a
/// head-to-head reports the number its two species land on whatever it is.
pub fn multiplier_label(multiplier: f32) -> &'static str {
    BUCKETS
        .iter()
        .find(|&&(value, _)| same_multiplier(multiplier, value))
        .map_or("×1", |&(_, label)| label)
}

/// Types this Pokemon hits for super-effective damage with a same-type move —
/// the union over its own types, since it can carry a move of each. Only types
/// `chart` has can be hit.
pub fn offensive_coverage(chart: Chart, attacker_types: &[String]) -> Vec<&'static str> {
    chart
        .types()
        .iter()
        .copied()
        .filter(|defender| {
            attacker_types
                .iter()
                .any(|attacker| chart.effectiveness(attacker, defender) > 1.5)
        })
        .collect()
}

/// Compares two multipliers. Every value the chart can produce is an exact
/// binary fraction, so the comparison is safe, but a tolerance keeps it robust
/// if the bucket list ever grows a value that isn't.
fn same_multiplier(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// Every cell where `older` and `newer` disagree, as
    /// `(attacker, defender, older, newer)`, over the types both charts have.
    fn differences(older: Chart, newer: Chart) -> Vec<(&'static str, &'static str, f32, f32)> {
        let mut cells = Vec::new();
        for &attacker in older.types() {
            for &defender in older.types() {
                let (was, is) = (
                    older.effectiveness(attacker, defender),
                    newer.effectiveness(attacker, defender),
                );
                if !same_multiplier(was, is) {
                    cells.push((attacker, defender, was, is));
                }
            }
        }
        cells
    }

    #[test]
    fn generation_two_changed_exactly_four_cells_of_the_first_chart() {
        assert_eq!(
            differences(Chart::Gen1, Chart::Gen2To5),
            [
                ("ice", "fire", 1.0, 0.5),
                ("poison", "bug", 2.0, 1.0),
                ("bug", "poison", 2.0, 0.5),
                ("ghost", "psychic", 0.0, 2.0),
            ]
        );
    }

    #[test]
    fn generation_six_changed_exactly_two_cells_of_the_second_chart() {
        assert_eq!(
            differences(Chart::Gen2To5, Chart::Gen6On),
            [("ghost", "steel", 0.5, 1.0), ("dark", "steel", 0.5, 1.0)]
        );
    }

    #[test]
    fn each_chart_has_the_types_of_its_era() {
        assert_eq!(Chart::Gen1.types().len(), 15);
        assert!(!Chart::Gen1.has("dark") && !Chart::Gen1.has("steel"));
        assert!(Chart::Gen2To5.has("steel") && !Chart::Gen2To5.has("fairy"));
        assert!(Chart::Gen6On.has("fairy"));
    }

    #[test]
    fn a_type_the_chart_does_not_have_is_neutral_on_either_side() {
        assert_eq!(Chart::Gen2To5.effectiveness("dragon", "fairy"), 1.0);
        assert_eq!(Chart::Gen2To5.effectiveness("fairy", "dragon"), 1.0);
        // Magnemite's Steel half, were it still attached in Generation I.
        assert_eq!(
            combined(Chart::Gen1, "ground", &types(&["electric", "steel"])),
            2.0
        );
    }

    #[test]
    fn an_old_chart_lists_none_of_the_types_it_does_not_have() {
        // Under Generation I, Gengar is Ghost/Poison and nothing on the card
        // can be Dark, Steel or Fairy.
        let groups = defensive_groups(Chart::Gen1, &types(&["ghost", "poison"]), &[]);
        let listed: Vec<&str> = groups.iter().flat_map(|g| g.types.clone()).collect();
        assert!(listed.iter().all(|t| Chart::Gen1.has(t)), "{listed:?}");
        // Bug was super effective on Poison and Ghost did nothing to Psychic,
        // so Gengar was weak to Bug and, Ghost hitting Ghost aside, nothing
        // Ghost-typed itself was any help against Psychic.
        let find = |label: &str| {
            groups
                .iter()
                .find(|g| g.label == label)
                .map(|g| g.types.clone())
                .unwrap_or_default()
        };
        assert_eq!(find("×2"), vec!["ground", "psychic", "ghost"]);
        assert!(find("×½").contains(&"grass"));
        assert!(
            !find("×½").contains(&"bug"),
            "bug was ×1 on Gengar in Gen I"
        );

        let coverage = offensive_coverage(Chart::Gen1, &types(&["ghost"]));
        assert_eq!(coverage, vec!["ghost"], "ghost did not hit psychic");
    }

    #[test]
    fn generations_map_onto_their_charts() {
        let chart = |n| Generation::new(n).unwrap().chart();
        assert_eq!(chart(1), Chart::Gen1);
        assert_eq!(chart(2), Chart::Gen2To5);
        assert_eq!(chart(5), Chart::Gen2To5);
        assert_eq!(chart(6), Chart::Gen6On);
        assert_eq!(chart(9), Chart::Gen6On);
        assert_eq!(Generation::new(0), None);
        assert_eq!(Generation::new(LATEST_GENERATION + 1), None);
        assert_eq!(Generation::default(), Generation::LATEST);
    }

    #[test]
    fn stepping_through_the_generations_wraps_both_ways() {
        let first = Generation::new(1).unwrap();
        assert_eq!(Generation::LATEST.next(), first);
        assert_eq!(first.previous(), Generation::LATEST);
        assert_eq!(first.next().number(), 2);
        assert_eq!(
            Generation::LATEST.previous().number(),
            LATEST_GENERATION - 1
        );
        let mut gen = first;
        for _ in 0..LATEST_GENERATION {
            gen = gen.next();
        }
        assert_eq!(gen, first);
    }

    #[test]
    fn abilities_begin_in_the_third_generation() {
        assert!(!Generation::new(2).unwrap().has_abilities());
        assert!(Generation::new(3).unwrap().has_abilities());
        assert_eq!(Generation::new(4).unwrap().roman(), "IV");
    }

    #[test]
    fn chart_is_symmetric_where_the_games_are() {
        // Spot-checks of the classic starter triangle.
        assert_eq!(Chart::Gen6On.effectiveness("water", "fire"), 2.0);
        assert_eq!(Chart::Gen6On.effectiveness("fire", "grass"), 2.0);
        assert_eq!(Chart::Gen6On.effectiveness("grass", "water"), 2.0);
        assert_eq!(Chart::Gen6On.effectiveness("grass", "fire"), 0.5);
    }

    #[test]
    fn immunities_are_zero() {
        assert_eq!(Chart::Gen6On.effectiveness("normal", "ghost"), 0.0);
        assert_eq!(Chart::Gen6On.effectiveness("ghost", "normal"), 0.0);
        assert_eq!(Chart::Gen6On.effectiveness("electric", "ground"), 0.0);
        assert_eq!(Chart::Gen6On.effectiveness("dragon", "fairy"), 0.0);
        assert_eq!(Chart::Gen6On.effectiveness("poison", "steel"), 0.0);
    }

    #[test]
    fn unlisted_pairings_are_neutral() {
        assert_eq!(Chart::Gen6On.effectiveness("normal", "normal"), 1.0);
        assert_eq!(Chart::Gen6On.effectiveness("water", "steel"), 1.0);
    }

    #[test]
    fn dual_types_stack_multiplicatively() {
        // Charizard (fire/flying): rock hits both halves for 2× → 4×.
        assert_eq!(
            combined(Chart::Gen6On, "rock", &types(&["fire", "flying"])),
            4.0
        );
        // Grass is resisted by fire *and* flying → ¼×.
        assert_eq!(
            combined(Chart::Gen6On, "grass", &types(&["fire", "flying"])),
            0.25
        );
        // Ground is 2× on fire but 0× on flying → immune overall.
        assert_eq!(
            combined(Chart::Gen6On, "ground", &types(&["fire", "flying"])),
            0.0
        );
    }

    #[test]
    fn defensive_groups_cover_charizard() {
        let groups = defensive_groups(Chart::Gen6On, &types(&["fire", "flying"]), &[]);
        let find = |label: &str| {
            groups
                .iter()
                .find(|g| g.label == label)
                .map(|g| g.types.clone())
                .unwrap_or_default()
        };
        assert_eq!(find("×4"), vec!["rock"]);
        assert_eq!(find("×2"), vec!["water", "electric"]);
        assert_eq!(find("×¼"), vec!["grass", "bug"]);
        assert_eq!(find("×0"), vec!["ground"]);
    }

    #[test]
    fn a_forced_immunity_moves_its_type_out_of_the_row_it_was_in() {
        // Rotom's Electric/Ghost body takes ×2 from Ground. Rotom does not.
        let rotom = types(&["electric", "ghost"]);
        let groups = defensive_groups(Chart::Gen6On, &rotom, &["ground"]);
        let find = |label: &str| {
            groups
                .iter()
                .find(|g| g.label == label)
                .map(|g| g.types.clone())
                .unwrap_or_default()
        };

        assert!(find("×0").contains(&"ground"));
        assert!(
            !find("×2").contains(&"ground"),
            "a type cannot be both ×2 and immune"
        );
        // The rest of the card is the chart's business and stays untouched.
        assert_eq!(find("×2"), vec!["ghost", "dark"]);
        assert!(find("×0").contains(&"normal"), "ghost still ignores normal");
    }

    #[test]
    fn a_forced_immunity_the_chart_already_knew_is_not_listed_twice() {
        // Flying is immune to Ground on its own; Levitate on top changes
        // nothing, least of all the number of rows Ground appears in.
        let groups = defensive_groups(Chart::Gen6On, &types(&["flying"]), &["ground"]);
        let ground_rows: usize = groups
            .iter()
            .filter(|g| g.types.contains(&"ground"))
            .count();
        assert_eq!(ground_rows, 1);
    }

    #[test]
    fn every_multiplier_the_chart_can_produce_has_a_label() {
        assert_eq!(multiplier_label(4.0), "×4");
        assert_eq!(multiplier_label(2.0), "×2");
        assert_eq!(multiplier_label(1.0), "×1");
        assert_eq!(multiplier_label(0.5), "×½");
        assert_eq!(multiplier_label(0.25), "×¼");
        assert_eq!(multiplier_label(0.0), "×0");
    }

    #[test]
    fn neutral_matchups_are_omitted() {
        let groups = defensive_groups(Chart::Gen6On, &types(&["normal"]), &[]);
        let listed: usize = groups.iter().map(|g| g.types.len()).sum();
        // Normal only cares about fighting (2×) and ghost (0×).
        assert_eq!(listed, 2);
    }

    #[test]
    fn offensive_coverage_is_the_union_of_both_types() {
        let coverage = offensive_coverage(Chart::Gen6On, &types(&["fire", "flying"]));
        assert!(coverage.contains(&"grass")); // 2× from both
        assert!(coverage.contains(&"steel")); // 2× from fire only
        assert!(coverage.contains(&"fighting")); // 2× from flying only
        assert!(!coverage.contains(&"water"));
    }
}
