//! Team-level type analysis.
//!
//! `typechart` answers a question about one species: what is *this* weak to?
//! A party of six asks a different one. What actually decides whether a team
//! holds together is not any single member's weaknesses but the overlap
//! between them:
//!
//! - an attacking type that hits **several** members hard is a hole an
//!   opponent will aim at, while one that hits a single member is just that
//!   member's problem;
//! - an attacking type **nobody** resists is a hit the team has to take on the
//!   chin every time;
//! - a defending type nobody hits back hard is a wall the team cannot break.
//!
//! All three fall straight out of the members' typings, so this stays as
//! offline and as instant as the single-species card.

use std::cmp::Reverse;

use crate::models::PokemonDetail;
use crate::typechart::{self, Generation};

/// Largest party the analyser accepts, matching the games.
pub const MAX_MEMBERS: usize = 6;

/// How many members an attacking type must hit super-effectively before it
/// counts as a *shared* weakness rather than one member's own business.
const SHARED_WEAKNESS_MIN: usize = 2;

/// Abilities that grant an outright immunity to a whole damage type, and the
/// type each one answers.
///
/// Only true immunities are listed. Abilities that merely soften a type (Thick
/// Fat halving Fire and Ice, say) belong to the multipliers the chart already
/// knows nothing about, and abilities keyed to a *class* of move rather than a
/// type (Soundproof, Bulletproof) cannot be expressed as a type at all.
const ABILITY_IMMUNITIES: [(&str, &str); 11] = [
    ("levitate", "ground"),
    ("earth-eater", "ground"),
    ("flash-fire", "fire"),
    ("well-baked-body", "fire"),
    ("water-absorb", "water"),
    ("storm-drain", "water"),
    ("dry-skin", "water"),
    ("volt-absorb", "electric"),
    ("lightning-rod", "electric"),
    ("motor-drive", "electric"),
    ("sap-sipper", "grass"),
];

/// The type `ability` grants immunity to, if any.
fn immunity_from(ability: &str) -> Option<&'static str> {
    ABILITY_IMMUNITIES
        .iter()
        .find(|(slug, _)| *slug == ability)
        .map(|(_, immune_to)| *immune_to)
}

/// One attacking type, weighed against the whole team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreatRow {
    pub attacker: &'static str,
    /// Members taking super-effective damage (×2 or ×4) from it.
    pub weak: usize,
}

/// An immunity a Pokemon owes to an ability rather than to its typing.
///
/// These are deliberately kept out of the team numbers above. A species has
/// one of its listed abilities, not all of them, so an immunity is only
/// guaranteed when there is nothing else it could have had — anything else
/// would make the chart claim a certainty the data does not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbilityImmunity {
    /// API name of the Pokemon that has it. A party member on the team card,
    /// the species the card is about on the single-species one.
    pub pokemon: String,
    /// API slug of the ability granting it.
    pub ability: String,
    pub immune_to: &'static str,
    /// True when the ability is the species' only one, so it cannot not have
    /// it. False when the species could have had a different ability instead.
    pub certain: bool,
}

/// What the team card reports. Every list is in canonical type order, and
/// empty lists are the good news: nothing to worry about in that category.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TeamAnalysis {
    /// Attacking types hitting at least [`SHARED_WEAKNESS_MIN`] members
    /// super-effectively, most members first.
    pub shared_weaknesses: Vec<ThreatRow>,
    /// Attacking types no member resists or is immune to.
    pub unresisted: Vec<&'static str>,
    /// Defending types no member hits super-effectively with a same-type move.
    pub offense_gaps: Vec<&'static str>,
    /// Immunities the members' abilities grant, which the type chart cannot
    /// see. Reported alongside the lists above rather than folded into them.
    pub ability_immunities: Vec<AbilityImmunity>,
    /// Members left out of every list above because their species was not in
    /// the generation's games, as raw API names in party order.
    pub absent: Vec<String>,
}

/// The immunities `species` owes to its abilities in `generation`'s games, in
/// the order PokeAPI lists them. A species whose abilities grant none yields an
/// empty list, and so does every species before abilities existed.
///
/// Both cards read the table through this one function on purpose. They used
/// to answer "what is this immune to?" separately, and only the team card knew
/// abilities existed, so the card that was *about* one species was the less
/// accurate of the two. One mapping is what keeps them from drifting apart
/// again.
pub fn ability_immunities(species: &PokemonDetail, generation: Generation) -> Vec<AbilityImmunity> {
    if !generation.has_abilities() {
        return Vec::new();
    }
    // Certain only when the species has nowhere else to land: with one
    // possible ability it must have this one.
    let certain = species.abilities.len() == 1;
    species
        .abilities
        .iter()
        .filter_map(|ability| {
            Some(AbilityImmunity {
                pokemon: species.name.clone(),
                ability: ability.name.clone(),
                immune_to: immunity_from(&ability.name)?,
                certain,
            })
        })
        .collect()
}

/// Analyses a party under `generation`'s chart, reading each member as it was
/// typed in those games. An empty team yields an empty analysis rather than
/// "weak to everything": with nothing to defend, there is nothing to report.
///
/// A member whose species was not in those games is left out and named in
/// [`TeamAnalysis::absent`]. Reading it anyway would mean inventing a typing
/// for it, and Sylveon under Generation I has none to invent.
pub fn analyse(party: &[&PokemonDetail], generation: Generation) -> TeamAnalysis {
    let (team, absent): (Vec<&PokemonDetail>, Vec<&PokemonDetail>) = party
        .iter()
        .copied()
        .partition(|member| member.exists_in(generation));
    let absent: Vec<String> = absent.iter().map(|member| member.name.clone()).collect();
    if team.is_empty() {
        return TeamAnalysis {
            absent,
            ..TeamAnalysis::default()
        };
    }

    let chart = generation.chart();
    let mut shared_weaknesses = Vec::new();
    let mut unresisted = Vec::new();

    for &attacker in chart.types() {
        let multipliers = team
            .iter()
            .map(|m| typechart::combined(chart, attacker, m.types_in(generation)));

        let mut weak = 0;
        let mut resisted_by_anyone = false;
        for multiplier in multipliers {
            // The same thresholds `typechart` uses: every value the chart can
            // produce is an exact binary fraction, so these never sit near a
            // boundary.
            if multiplier > 1.5 {
                weak += 1;
            } else if multiplier < 0.9 {
                resisted_by_anyone = true;
            }
        }

        if weak >= SHARED_WEAKNESS_MIN {
            shared_weaknesses.push(ThreatRow { attacker, weak });
        }
        if !resisted_by_anyone {
            unresisted.push(attacker);
        }
    }

    // Stable, so types sharing a count keep canonical order.
    shared_weaknesses.sort_by_key(|row| Reverse(row.weak));

    let offense_gaps = chart
        .types()
        .iter()
        .copied()
        .filter(|defender| {
            !team.iter().any(|member| {
                member
                    .types_in(generation)
                    .iter()
                    .any(|attacker| chart.effectiveness(attacker, defender) > 1.5)
            })
        })
        .collect();

    // Every member's, in party order, through the same mapping the
    // single-species card reads. Spelled out as a path because the binding
    // below shares its name.
    let ability_immunities = team
        .iter()
        .copied()
        .flat_map(|member| self::ability_immunities(member, generation))
        .collect();

    TeamAnalysis {
        shared_weaknesses,
        unresisted,
        offense_gaps,
        ability_immunities,
        absent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Ability, FieldData, PastTyping};
    use crate::typechart::Chart;
    use std::collections::HashMap;

    const LATEST: Generation = Generation::LATEST;

    fn gen(number: u8) -> Generation {
        Generation::new(number).unwrap()
    }

    fn member(name: &str, types: &[&str]) -> PokemonDetail {
        with_abilities(name, types, &[])
    }

    fn with_abilities(name: &str, types: &[&str], abilities: &[&str]) -> PokemonDetail {
        PokemonDetail {
            name: name.to_string(),
            species: name.to_string(),
            forms: Vec::new(),
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
            stats: Vec::new(),
            height: 0,
            weight: 0,
            sprite_url: None,
            shiny_sprite_url: None,
            genera: HashMap::new(),
            flavors: HashMap::new(),
            moves: Vec::new(),
            learnset_games: None,
            past_types: Vec::new(),
            field: FieldData::default(),
        }
    }

    fn weakness_count(analysis: &TeamAnalysis, attacker: &str) -> usize {
        analysis
            .shared_weaknesses
            .iter()
            .find(|row| row.attacker == attacker)
            .map_or(0, |row| row.weak)
    }

    #[test]
    fn an_empty_team_reports_nothing() {
        assert_eq!(analyse(&[], LATEST), TeamAnalysis::default());
    }

    #[test]
    fn one_members_weakness_is_not_shared() {
        let charizard = member("charizard", &["fire", "flying"]);
        let analysis = analyse(&[&charizard], LATEST);
        // Rock is ×4 on Charizard, but a party of one has nothing to share it
        // with, so it is that member's problem and not the team's.
        assert!(analysis.shared_weaknesses.is_empty());
    }

    #[test]
    fn a_weakness_two_members_share_is_reported() {
        // Both are hit ×2 by Electric and neither resists it.
        let gyarados = member("gyarados", &["water", "flying"]);
        let pelipper = member("pelipper", &["water", "flying"]);
        let analysis = analyse(&[&gyarados, &pelipper], LATEST);

        assert_eq!(weakness_count(&analysis, "electric"), 2);
        assert!(analysis.unresisted.contains(&"electric"));
    }

    #[test]
    fn the_worst_shared_weakness_comes_first() {
        // Three Water/Flying bodies: Electric hits all three, Rock only ×2 on
        // each of them as well — but Electric is ×4, and more to the point
        // every member is weak to both, so ordering falls back to the count.
        let team: Vec<PokemonDetail> = ["gyarados", "pelipper", "mantine"]
            .iter()
            .map(|n| member(n, &["water", "flying"]))
            .collect();
        let refs: Vec<&PokemonDetail> = team.iter().collect();
        let analysis = analyse(&refs, LATEST);

        let counts: Vec<usize> = analysis.shared_weaknesses.iter().map(|r| r.weak).collect();
        let mut sorted = counts.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(counts, sorted, "rows must be ordered worst-first");
        assert_eq!(weakness_count(&analysis, "electric"), 3);
    }

    #[test]
    fn a_type_someone_resists_is_not_unresisted() {
        let magnezone = member("magnezone", &["electric", "steel"]);
        let gyarados = member("gyarados", &["water", "flying"]);
        let analysis = analyse(&[&magnezone, &gyarados], LATEST);

        // Magnezone resists Electric (Steel ×½ · Electric ×½), so the team has
        // an answer to it even though Gyarados is weak.
        assert!(!analysis.unresisted.contains(&"electric"));
        // And with only one member weak to it, it is not a *shared* weakness.
        assert_eq!(weakness_count(&analysis, "electric"), 0);
    }

    #[test]
    fn offense_gaps_are_what_nobody_hits_hard() {
        let charizard = member("charizard", &["fire", "flying"]);
        let analysis = analyse(&[&charizard], LATEST);

        // Fire hits Grass/Ice/Bug/Steel; Flying hits Grass/Fighting/Bug.
        for covered in ["grass", "ice", "bug", "steel", "fighting"] {
            assert!(
                !analysis.offense_gaps.contains(&covered),
                "{covered} is covered"
            );
        }
        // Nothing it carries is strong against Water or Dragon.
        assert!(analysis.offense_gaps.contains(&"water"));
        assert!(analysis.offense_gaps.contains(&"dragon"));
    }

    #[test]
    fn a_sole_ability_grants_a_certain_immunity() {
        // Rotom has nothing but Levitate, so the Ground immunity is a fact.
        let rotom = with_abilities("rotom", &["electric", "ghost"], &["levitate"]);
        let analysis = analyse(&[&rotom], LATEST);

        assert_eq!(
            analysis.ability_immunities,
            vec![AbilityImmunity {
                pokemon: "rotom".to_string(),
                ability: "levitate".to_string(),
                immune_to: "ground",
                certain: true,
            }]
        );
    }

    #[test]
    fn one_of_several_abilities_is_only_a_possibility() {
        // Vaporeon can have Water Absorb — or Hydration instead.
        let vaporeon = with_abilities("vaporeon", &["water"], &["water-absorb", "hydration"]);
        let analysis = analyse(&[&vaporeon], LATEST);

        assert_eq!(analysis.ability_immunities.len(), 1);
        assert_eq!(analysis.ability_immunities[0].immune_to, "water");
        assert!(!analysis.ability_immunities[0].certain);
    }

    #[test]
    fn abilities_that_grant_no_immunity_are_ignored() {
        let bulbasaur = with_abilities("bulbasaur", &["grass"], &["overgrow", "chlorophyll"]);
        let analysis = analyse(&[&bulbasaur], LATEST);
        assert!(analysis.ability_immunities.is_empty());
    }

    #[test]
    fn an_ability_immunity_does_not_touch_the_type_numbers() {
        // Levitate answers Ground, but the chart-level lists stay chart-level:
        // Gengar's typing alone still has no Ground resistance to report.
        let levitator = with_abilities("gengar", &["ghost", "poison"], &["levitate"]);
        let analysis = analyse(&[&levitator], LATEST);

        assert!(analysis.unresisted.contains(&"ground"));
        assert_eq!(analysis.ability_immunities[0].immune_to, "ground");
    }

    #[test]
    fn both_cards_read_the_same_immunities() {
        // The regression this guards: the single-species card once answered
        // from the type chart alone and disagreed with the party card about
        // the very same Rotom.
        let rotom = with_abilities("rotom", &["electric", "ghost"], &["levitate"]);
        assert_eq!(
            ability_immunities(&rotom, LATEST),
            analyse(&[&rotom], LATEST).ability_immunities
        );
    }

    #[test]
    fn a_certain_immunity_answers_the_chart_on_the_single_species_card() {
        // What the `T` card draws: the certain immunities, folded into the
        // rows, leave nothing claiming Ground still lands on Rotom.
        let rotom = with_abilities("rotom", &["electric", "ghost"], &["levitate"]);
        let certain: Vec<&str> = ability_immunities(&rotom, LATEST)
            .iter()
            .filter(|immunity| immunity.certain)
            .map(|immunity| immunity.immune_to)
            .collect();
        let groups = typechart::defensive_groups(Chart::Gen6On, &rotom.types, &certain);

        for group in &groups {
            assert_eq!(
                group.types.contains(&"ground"),
                group.label == "×0",
                "ground belongs in ×0 and nowhere else"
            );
        }
    }

    #[test]
    fn a_possible_immunity_is_left_out_of_the_rows() {
        // Vaporeon might have Hydration instead, so the chart keeps its say:
        // Water stays where the typing puts it, and the card annotates it.
        let vaporeon = with_abilities("vaporeon", &["water"], &["water-absorb", "hydration"]);
        let certain: Vec<&str> = ability_immunities(&vaporeon, LATEST)
            .iter()
            .filter(|immunity| immunity.certain)
            .map(|immunity| immunity.immune_to)
            .collect();

        assert!(certain.is_empty());
        assert_eq!(
            typechart::defensive_groups(Chart::Gen6On, &vaporeon.types, &certain)
                .iter()
                .map(|g| (g.label, g.types.clone()))
                .collect::<Vec<_>>(),
            typechart::defensive_groups(Chart::Gen6On, &vaporeon.types, &[])
                .iter()
                .map(|g| (g.label, g.types.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_broad_team_closes_most_gaps() {
        let team = [
            member("charizard", &["fire", "flying"]),
            member("gyarados", &["water", "flying"]),
            member("magnezone", &["electric", "steel"]),
            member("gengar", &["ghost", "poison"]),
            member("garchomp", &["dragon", "ground"]),
            member("machamp", &["fighting"]),
        ];
        let refs: Vec<&PokemonDetail> = team.iter().collect();
        let analysis = analyse(&refs, LATEST);

        assert_eq!(refs.len(), MAX_MEMBERS);
        // Six well-spread typings should leave few types untouched offensively.
        assert!(
            analysis.offense_gaps.len() <= 3,
            "unexpected gaps: {:?}",
            analysis.offense_gaps
        );
    }

    /// A member with its national dex number and the typings it used to have.
    fn historical(name: &str, dex: u32, types: &[&str], past: &[(u8, &[&str])]) -> PokemonDetail {
        PokemonDetail {
            dex_number: dex,
            past_types: past
                .iter()
                .map(|&(until, types)| PastTyping {
                    until,
                    types: types.iter().map(|t| t.to_string()).collect(),
                })
                .collect(),
            ..member(name, types)
        }
    }

    #[test]
    fn a_generation_one_party_gets_the_generation_one_answer() {
        // Two Poison types. Bug was super effective on Poison until Gold and
        // Silver, and resisted by it since.
        let arbok = historical("arbok", 24, &["poison"], &[]);
        let muk = historical("muk", 89, &["poison"], &[]);

        let then = analyse(&[&arbok, &muk], gen(1));
        assert_eq!(weakness_count(&then, "bug"), 2);
        assert!(then.unresisted.contains(&"bug"));

        let now = analyse(&[&arbok, &muk], LATEST);
        assert_eq!(weakness_count(&now, "bug"), 0);
        assert!(!now.unresisted.contains(&"bug"));
    }

    #[test]
    fn no_type_the_generation_lacks_appears_anywhere() {
        let gengar = historical("gengar", 94, &["ghost", "poison"], &[]);
        let analysis = analyse(&[&gengar], gen(1));
        let listed = analysis
            .unresisted
            .iter()
            .chain(&analysis.offense_gaps)
            .chain(analysis.shared_weaknesses.iter().map(|row| &row.attacker));
        for slug in listed {
            assert!(Chart::Gen1.has(slug), "{slug} is not a Generation I type");
        }
    }

    #[test]
    fn past_typings_are_applied() {
        let clefairy = historical("clefairy", 35, &["fairy"], &[(5, &["normal"])]);
        let magnemite = historical(
            "magnemite",
            81,
            &["electric", "steel"],
            &[(1, &["electric"])],
        );

        // Normal until Generation V, so Fighting hit it and Dragon did not
        // bounce off.
        assert_eq!(clefairy.types_in(gen(5)), ["normal"]);
        assert_eq!(clefairy.types_in(gen(6)), ["fairy"]);
        let then = analyse(&[&clefairy, &clefairy], gen(5));
        assert_eq!(weakness_count(&then, "fighting"), 2);

        // Steel arrived with Generation II, and Magnemite with it.
        assert_eq!(magnemite.types_in(gen(1)), ["electric"]);
        assert_eq!(magnemite.types_in(gen(2)), ["electric", "steel"]);
        let then = analyse(&[&magnemite, &magnemite], gen(1));
        assert_eq!(weakness_count(&then, "ground"), 2);
        assert!(then.unresisted.contains(&"poison"), "no Steel to stop it");
    }

    #[test]
    fn the_earliest_past_typing_that_still_applies_wins() {
        // A made-up history with two changes, to pin down the ordering.
        let shifty = historical(
            "shifty",
            1,
            &["dragon"],
            &[(2, &["normal"]), (4, &["water"])],
        );
        assert_eq!(shifty.types_in(gen(1)), ["normal"]);
        assert_eq!(shifty.types_in(gen(2)), ["normal"]);
        assert_eq!(shifty.types_in(gen(3)), ["water"]);
        assert_eq!(shifty.types_in(gen(5)), ["dragon"]);
    }

    #[test]
    fn a_member_from_a_later_generation_is_left_out_and_named() {
        let sylveon = historical("sylveon", 700, &["fairy"], &[]);
        let arbok = historical("arbok", 24, &["poison"], &[]);

        let analysis = analyse(&[&sylveon, &arbok], gen(1));
        assert_eq!(analysis.absent, ["sylveon"]);
        assert_eq!(analysis, {
            let alone = analyse(&[&arbok], gen(1));
            TeamAnalysis {
                absent: vec!["sylveon".to_string()],
                ..alone
            }
        });

        let only_sylveon = analyse(&[&sylveon], gen(1));
        assert_eq!(only_sylveon.absent, ["sylveon"]);
        assert!(only_sylveon.unresisted.is_empty());

        assert!(analyse(&[&sylveon, &arbok], gen(6)).absent.is_empty());
    }

    #[test]
    fn there_are_no_ability_immunities_before_abilities_existed() {
        let gengar = with_abilities("gengar", &["ghost", "poison"], &["levitate"]);
        assert!(ability_immunities(&gengar, gen(2)).is_empty());
        assert_eq!(ability_immunities(&gengar, gen(3)).len(), 1);
    }
}
