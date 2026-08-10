//! Static type-effectiveness chart (Generation VI onwards).
//!
//! This is pure, offline data: no PokeAPI round-trip is needed to answer "what
//! is this Pokemon weak to?", so the matchup card opens instantly for any
//! species whose types are already known.
//!
//! The chart is expressed from the *attacker's* point of view — [`effectiveness`]
//! answers "how much damage does a move of type A deal to a Pokemon of type B?"
//! — and the defensive view is derived by multiplying across the defender's
//! types, exactly as the games do.

/// Every type slug, in the canonical Pokedex order PokeAPI uses.
pub const TYPES: [&str; 18] = [
    "normal", "fire", "water", "electric", "grass", "ice", "fighting", "poison", "ground",
    "flying", "psychic", "bug", "rock", "ghost", "dragon", "dark", "steel", "fairy",
];

/// Damage multiplier for a move of type `attacker` hitting a *single* type
/// `defender`. Only the non-neutral pairings are listed; everything else is 1×.
pub fn effectiveness(attacker: &str, defender: &str) -> f32 {
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
/// `defender_types` — the product of the per-type multipliers, so a dual type
/// can land anywhere from 0× to 4×.
pub fn combined(attacker: &str, defender_types: &[String]) -> f32 {
    defender_types
        .iter()
        .map(|d| effectiveness(attacker, d))
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
pub fn defensive_groups(defender_types: &[String]) -> Vec<MatchupGroup> {
    BUCKETS
        .iter()
        .filter_map(|&(multiplier, label)| {
            let types: Vec<&'static str> = TYPES
                .iter()
                .copied()
                .filter(|attacker| same_multiplier(combined(attacker, defender_types), multiplier))
                .collect();
            (!types.is_empty()).then_some(MatchupGroup { label, types })
        })
        .collect()
}

/// Types this Pokemon hits for super-effective damage with a same-type move —
/// the union over its own types, since it can carry a move of each.
pub fn offensive_coverage(attacker_types: &[String]) -> Vec<&'static str> {
    TYPES
        .iter()
        .copied()
        .filter(|defender| {
            attacker_types
                .iter()
                .any(|attacker| effectiveness(attacker, defender) > 1.5)
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

    #[test]
    fn chart_is_symmetric_where_the_games_are() {
        // Spot-checks of the classic starter triangle.
        assert_eq!(effectiveness("water", "fire"), 2.0);
        assert_eq!(effectiveness("fire", "grass"), 2.0);
        assert_eq!(effectiveness("grass", "water"), 2.0);
        assert_eq!(effectiveness("grass", "fire"), 0.5);
    }

    #[test]
    fn immunities_are_zero() {
        assert_eq!(effectiveness("normal", "ghost"), 0.0);
        assert_eq!(effectiveness("ghost", "normal"), 0.0);
        assert_eq!(effectiveness("electric", "ground"), 0.0);
        assert_eq!(effectiveness("dragon", "fairy"), 0.0);
        assert_eq!(effectiveness("poison", "steel"), 0.0);
    }

    #[test]
    fn unlisted_pairings_are_neutral() {
        assert_eq!(effectiveness("normal", "normal"), 1.0);
        assert_eq!(effectiveness("water", "steel"), 1.0);
    }

    #[test]
    fn dual_types_stack_multiplicatively() {
        // Charizard (fire/flying): rock hits both halves for 2× → 4×.
        assert_eq!(combined("rock", &types(&["fire", "flying"])), 4.0);
        // Grass is resisted by fire *and* flying → ¼×.
        assert_eq!(combined("grass", &types(&["fire", "flying"])), 0.25);
        // Ground is 2× on fire but 0× on flying → immune overall.
        assert_eq!(combined("ground", &types(&["fire", "flying"])), 0.0);
    }

    #[test]
    fn defensive_groups_cover_charizard() {
        let groups = defensive_groups(&types(&["fire", "flying"]));
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
    fn neutral_matchups_are_omitted() {
        let groups = defensive_groups(&types(&["normal"]));
        let listed: usize = groups.iter().map(|g| g.types.len()).sum();
        // Normal only cares about fighting (2×) and ghost (0×).
        assert_eq!(listed, 2);
    }

    #[test]
    fn offensive_coverage_is_the_union_of_both_types() {
        let coverage = offensive_coverage(&types(&["fire", "flying"]));
        assert!(coverage.contains(&"grass")); // 2× from both
        assert!(coverage.contains(&"steel")); // 2× from fire only
        assert!(coverage.contains(&"fighting")); // 2× from flying only
        assert!(!coverage.contains(&"water"));
    }
}
