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
use crate::typechart::{self, TYPES};

/// Largest party the analyser accepts, matching the games.
pub const MAX_MEMBERS: usize = 6;

/// How many members an attacking type must hit super-effectively before it
/// counts as a *shared* weakness rather than one member's own business.
const SHARED_WEAKNESS_MIN: usize = 2;

/// One attacking type, weighed against the whole team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreatRow {
    pub attacker: &'static str,
    /// Members taking super-effective damage (×2 or ×4) from it.
    pub weak: usize,
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
}

/// Analyses a party. An empty team yields an empty analysis rather than
/// "weak to everything": with nothing to defend, there is nothing to report.
pub fn analyse(team: &[&PokemonDetail]) -> TeamAnalysis {
    if team.is_empty() {
        return TeamAnalysis::default();
    }

    let mut shared_weaknesses = Vec::new();
    let mut unresisted = Vec::new();

    for attacker in TYPES {
        let multipliers = team.iter().map(|m| typechart::combined(attacker, &m.types));

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

    let offense_gaps = TYPES
        .into_iter()
        .filter(|defender| {
            !team.iter().any(|member| {
                member
                    .types
                    .iter()
                    .any(|attacker| typechart::effectiveness(attacker, defender) > 1.5)
            })
        })
        .collect();

    TeamAnalysis { shared_weaknesses, unresisted, offense_gaps }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn member(name: &str, types: &[&str]) -> PokemonDetail {
        PokemonDetail {
            name: name.to_string(),
            species: name.to_string(),
            dex_number: 0,
            is_legendary: false,
            is_mythical: false,
            is_baby: false,
            types: types.iter().map(|t| t.to_string()).collect(),
            abilities: Vec::new(),
            stats: Vec::new(),
            height: 0,
            weight: 0,
            sprite_url: None,
            genera: HashMap::new(),
            flavors: HashMap::new(),
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
        assert_eq!(analyse(&[]), TeamAnalysis::default());
    }

    #[test]
    fn one_members_weakness_is_not_shared() {
        let charizard = member("charizard", &["fire", "flying"]);
        let analysis = analyse(&[&charizard]);
        // Rock is ×4 on Charizard, but a party of one has nothing to share it
        // with, so it is that member's problem and not the team's.
        assert!(analysis.shared_weaknesses.is_empty());
    }

    #[test]
    fn a_weakness_two_members_share_is_reported() {
        // Both are hit ×2 by Electric and neither resists it.
        let gyarados = member("gyarados", &["water", "flying"]);
        let pelipper = member("pelipper", &["water", "flying"]);
        let analysis = analyse(&[&gyarados, &pelipper]);

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
        let analysis = analyse(&refs);

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
        let analysis = analyse(&[&magnezone, &gyarados]);

        // Magnezone resists Electric (Steel ×½ · Electric ×½), so the team has
        // an answer to it even though Gyarados is weak.
        assert!(!analysis.unresisted.contains(&"electric"));
        // And with only one member weak to it, it is not a *shared* weakness.
        assert_eq!(weakness_count(&analysis, "electric"), 0);
    }

    #[test]
    fn offense_gaps_are_what_nobody_hits_hard() {
        let charizard = member("charizard", &["fire", "flying"]);
        let analysis = analyse(&[&charizard]);

        // Fire hits Grass/Ice/Bug/Steel; Flying hits Grass/Fighting/Bug.
        for covered in ["grass", "ice", "bug", "steel", "fighting"] {
            assert!(!analysis.offense_gaps.contains(&covered), "{covered} is covered");
        }
        // Nothing it carries is strong against Water or Dragon.
        assert!(analysis.offense_gaps.contains(&"water"));
        assert!(analysis.offense_gaps.contains(&"dragon"));
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
        let analysis = analyse(&refs);

        assert_eq!(refs.len(), MAX_MEMBERS);
        // Six well-spread typings should leave few types untouched offensively.
        assert!(
            analysis.offense_gaps.len() <= 3,
            "unexpected gaps: {:?}",
            analysis.offense_gaps
        );
    }
}
