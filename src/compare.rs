//! Head-to-head comparison of two species.
//!
//! `typechart` answers a question about one Pokemon and `team` about a party of
//! six. This module answers the one a Pokedex is most often opened for and the
//! only one the app used to leave to the reader: of these two, which is faster,
//! which is bulkier, which hits the other harder.
//!
//! Everything here is a pure function of two records the app already holds, so
//! the card costs no request to open and no arithmetic to read.

use std::cmp::Ordering;

use crate::models::{PokemonDetail, StatKind};
use crate::team;
use crate::typechart;

/// The stats every row of a comparison covers, in display order.
///
/// Listed here rather than read off either record: the two columns only line up
/// if both are laid out against the same order, whatever order the records
/// happen to carry their stats in.
pub const STAT_ORDER: [StatKind; 6] = [
    StatKind::Hp,
    StatKind::Attack,
    StatKind::Defense,
    StatKind::SpecialAttack,
    StatKind::SpecialDefense,
    StatKind::Speed,
];

/// Which side of a comparison comes out ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    /// Dead level. Worth its own variant rather than folding into one of the
    /// sides: "these two are the same here" is an answer, not a near miss.
    Tie,
}

/// Which of two numbers wins. Also used for the totals line, which is the same
/// question asked of a wider number.
pub fn side(left: u32, right: u32) -> Side {
    match left.cmp(&right) {
        Ordering::Greater => Side::Left,
        Ordering::Less => Side::Right,
        Ordering::Equal => Side::Tie,
    }
}

/// One base stat, as both species have it.
///
/// Which side it favours is [`side`]'s answer and how far apart they are is
/// `left.abs_diff(right)`, both left to the caller: the totals line asks the
/// same two questions of a number that is not a base stat, and one definition
/// answering both is what keeps the two consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatRow {
    pub kind: StatKind,
    pub left: u16,
    pub right: u16,
}

/// Both species' base stats, paired up in [`STAT_ORDER`].
///
/// A stat a record does not carry reads as 0 rather than dropping the row: the
/// rows are what the card is read across, so one of them going missing on one
/// side would silently shift the other five out of line.
pub fn stat_rows(left: &PokemonDetail, right: &PokemonDetail) -> Vec<StatRow> {
    STAT_ORDER
        .iter()
        .map(|&kind| StatRow {
            kind,
            left: base(left, kind),
            right: base(right, kind),
        })
        .collect()
}

/// The highest number anywhere in a set of rows, which is what the card scales
/// its bars against. Scaling to the pair rather than to the 255 a stat can
/// theoretically reach is what makes the difference between the two visible:
/// against the theoretical ceiling every ordinary species draws the same short
/// bar.
pub fn peak(rows: &[StatRow]) -> u16 {
    rows.iter()
        .map(|row| row.left.max(row.right))
        .max()
        .unwrap_or(0)
}

fn base(species: &PokemonDetail, kind: StatKind) -> u16 {
    species
        .stats
        .iter()
        .find(|stat| stat.kind == kind)
        .map_or(0, |stat| stat.base)
}

/// The hardest hit one species' typing lands on the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit<'a> {
    /// The attacker's own type that gets there.
    pub attack_type: &'a str,
    pub multiplier: f32,
}

/// The best same-type hit `attacker` has on `defender`, if it has a typing at
/// all.
///
/// Same-type moves are the ones a species certainly has access to, which is
/// what makes this the decisive number rather than a guess at a moveset. A
/// defender's *certain* ability immunity overrules the chart here exactly as it
/// does on the matchup card: Levitate makes Ground 0× however the chart reads
/// the body carrying it. An immunity the defender merely might have is left
/// alone — a maybe is not a multiplier.
pub fn best_hit<'a>(attacker: &'a PokemonDetail, defender: &PokemonDetail) -> Option<Hit<'a>> {
    let immune: Vec<&str> = team::ability_immunities(defender)
        .iter()
        .filter(|immunity| immunity.certain)
        .map(|immunity| immunity.immune_to)
        .collect();

    attacker
        .types
        .iter()
        .map(|attack_type| Hit {
            attack_type,
            multiplier: match immune.contains(&attack_type.as_str()) {
                true => 0.0,
                false => typechart::combined(attack_type, &defender.types),
            },
        })
        // Ties go to the type listed first, which is the species' primary.
        .max_by(|a, b| {
            a.multiplier
                .partial_cmp(&b.multiplier)
                .unwrap_or(Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Ability, Stat};
    use std::collections::HashMap;

    fn species(name: &str, types: &[&str], stats: &[(StatKind, u16)]) -> PokemonDetail {
        with_abilities(name, types, stats, &[])
    }

    fn with_abilities(
        name: &str,
        types: &[&str],
        stats: &[(StatKind, u16)],
        abilities: &[&str],
    ) -> PokemonDetail {
        PokemonDetail {
            name: name.to_string(),
            species: name.to_string(),
            dex_number: 0,
            is_legendary: false,
            is_mythical: false,
            is_baby: false,
            types: types.iter().map(|t| t.to_string()).collect(),
            abilities: abilities
                .iter()
                .map(|a| Ability {
                    name: a.to_string(),
                    is_hidden: false,
                })
                .collect(),
            stats: stats
                .iter()
                .map(|&(kind, base)| Stat { kind, base })
                .collect(),
            height: 0,
            weight: 0,
            sprite_url: None,
            shiny_sprite_url: None,
            genera: HashMap::new(),
            flavors: HashMap::new(),
            moves: Vec::new(),
            learnset_games: None,
        }
    }

    #[test]
    fn each_row_names_its_winner_and_by_how_much() {
        let fast = species("jolteon", &["electric"], &[(StatKind::Speed, 130)]);
        let slow = species("snorlax", &["normal"], &[(StatKind::Speed, 30)]);

        let rows = stat_rows(&fast, &slow);
        let speed = rows.last().expect("six rows");
        assert_eq!(speed.kind, StatKind::Speed);
        assert_eq!(side(speed.left.into(), speed.right.into()), Side::Left);
        assert_eq!(speed.left.abs_diff(speed.right), 100);
    }

    #[test]
    fn a_stat_they_share_is_reported_as_a_tie_rather_than_a_near_miss() {
        let a = species("a", &["normal"], &[(StatKind::Attack, 100)]);
        let b = species("b", &["normal"], &[(StatKind::Attack, 100)]);

        let attack = stat_rows(&a, &b)[1];
        assert_eq!(side(attack.left.into(), attack.right.into()), Side::Tie);
        assert_eq!(attack.left.abs_diff(attack.right), 0);
    }

    #[test]
    fn the_rows_line_up_however_the_records_order_their_stats() {
        // PokeAPI's order is stable, but nothing downstream should depend on
        // both records having listed the same stats in the same places.
        let forwards = species(
            "a",
            &["normal"],
            &[(StatKind::Hp, 10), (StatKind::Speed, 20)],
        );
        let backwards = species(
            "b",
            &["normal"],
            &[(StatKind::Speed, 40), (StatKind::Hp, 30)],
        );

        let rows = stat_rows(&forwards, &backwards);
        assert_eq!(rows[0].kind, StatKind::Hp);
        assert_eq!((rows[0].left, rows[0].right), (10, 30));
        assert_eq!(rows[5].kind, StatKind::Speed);
        assert_eq!((rows[5].left, rows[5].right), (20, 40));
    }

    #[test]
    fn a_stat_neither_record_carries_still_gets_a_row() {
        let rows = stat_rows(&species("a", &[], &[]), &species("b", &[], &[]));
        assert_eq!(rows.len(), STAT_ORDER.len());
        assert!(rows
            .iter()
            .all(|row| side(row.left.into(), row.right.into()) == Side::Tie));
    }

    #[test]
    fn the_bars_scale_to_the_biggest_number_on_the_card() {
        let rows = stat_rows(
            &species("a", &[], &[(StatKind::Hp, 55)]),
            &species("b", &[], &[(StatKind::Hp, 120)]),
        );
        assert_eq!(peak(&rows), 120);
    }

    #[test]
    fn the_best_hit_is_the_hardest_of_the_attackers_own_types() {
        // Gengar into Alakazam: Ghost is x2 on Psychic, Poison only x1.
        let gengar = species("gengar", &["ghost", "poison"], &[]);
        let alakazam = species("alakazam", &["psychic"], &[]);

        let hit = best_hit(&gengar, &alakazam).expect("a typing to hit with");
        assert_eq!(hit.attack_type, "ghost");
        assert_eq!(hit.multiplier, 2.0);
    }

    #[test]
    fn a_dual_type_defender_multiplies_both_ways() {
        // Ice into Dragon/Flying is x4, the chart's worst news.
        let glaceon = species("glaceon", &["ice"], &[]);
        let dragonite = species("dragonite", &["dragon", "flying"], &[]);

        assert_eq!(best_hit(&glaceon, &dragonite).unwrap().multiplier, 4.0);
    }

    #[test]
    fn a_certain_ability_immunity_answers_the_chart() {
        let sandslash = species("sandslash", &["ground"], &[]);
        // Ground is x2 on an Electric body, and 0x when it cannot not levitate.
        let eelektross = with_abilities("eelektross", &["electric"], &[], &["levitate"]);
        assert_eq!(best_hit(&sandslash, &eelektross).unwrap().multiplier, 0.0);

        // With a second possible ability it is a maybe, and a maybe is not a
        // multiplier: the chart's answer stands.
        let maybe = with_abilities("maybe", &["electric"], &[], &["levitate", "static"]);
        assert_eq!(best_hit(&sandslash, &maybe).unwrap().multiplier, 2.0);
    }

    #[test]
    fn a_species_with_no_typing_has_no_hit_to_report() {
        assert_eq!(
            best_hit(&species("a", &[], &[]), &species("b", &[], &[])),
            None
        );
    }
}
