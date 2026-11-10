//! Who would make a party better.
//!
//! The party card says what is wrong with a team: the types that hit several
//! members hard, the ones nobody resists, the ones nobody hits back. What it
//! leaves to the reader is the next question, which is who fixes that, and
//! answering it by hand means holding eighteen types in mind against
//! thirteen hundred species.
//!
//! This module answers it from what is built into the binary: the type chart
//! and the table in [`crate::base_stats`]. Nothing is fetched, so the answer
//! is there the moment the key is pressed and offline.
//!
//! A suggestion is about typing and nothing else. It does not know what
//! moves a species learns, which ability it would be running, or whether it
//! is any good; it knows that a Steel/Flying type takes little from the Ice
//! and Rock moves three members are weak to. The score is a way to order the
//! list, and the reasons beside each entry are the part to read.

use std::cmp::Reverse;

use crate::base_stats::{self, StatKey};
use crate::models::{PokemonDetail, PokemonEntry};
use crate::typechart::{self, Chart, Generation};

/// What resisting a type is worth for each member weak to it.
const COVER_PER_WEAK_MEMBER: i32 = 2;
/// What resisting a type nobody in the party resists is worth, on top of
/// anything it earns for the members weak to it.
const COVER_UNRESISTED: i32 = 3;
/// What an immunity is worth over a resistance, where either earns anything.
const IMMUNITY_BONUS: i32 = 1;
/// What a same-type hit on a type the party cannot hit hard is worth.
const HIT_GAP: i32 = 2;
/// What sharing a weakness costs for each member already weak to the type.
const SHARED_PER_WEAK_MEMBER: i32 = 2;
/// What a double weakness costs on top, wherever it is shared.
const DOUBLE_WEAKNESS: i32 = 1;

/// One species worth adding, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Raw API name.
    pub name: String,
    pub types: Vec<&'static str>,
    pub score: i32,
    /// Attacking types it resists or is immune to that the party has trouble
    /// with: ones a member is weak to, or that nobody resists.
    pub covers: Vec<&'static str>,
    /// Defending types it hits hard with a same-type move that nobody in the
    /// party does.
    pub hits: Vec<&'static str>,
    /// Attacking types it is weak to that a member already is, so adding it
    /// makes a weakness shared, or more shared.
    pub shares: Vec<&'static str>,
}

/// How one attacking type sits against the party, before anyone is added.
struct Threat {
    attacker: &'static str,
    /// Members taking super-effective damage from it.
    weak: i32,
    /// Whether any member resists or is immune to it.
    resisted: bool,
}

/// The entries of `pool` that would improve `party` most, best first, at most
/// `limit` of them. Empty for an empty party, which has nothing to improve.
///
/// Each typing is suggested once, as its strongest species by base stat
/// total.
///
/// A candidate is left out when the answer for it could not be trusted: an
/// alternate form, which the list cannot place in a generation; a species the
/// generation's games did not have; and one typed with something the
/// generation's chart lacks, since what it was typed before that is not in
/// the table. Under the newest generation the last two leave nobody out.
pub fn suggest<'a>(
    party: &[&PokemonDetail],
    generation: Generation,
    pool: impl Iterator<Item = &'a PokemonEntry>,
    limit: usize,
) -> Vec<Suggestion> {
    let team: Vec<&PokemonDetail> = party
        .iter()
        .copied()
        .filter(|member| member.exists_in(generation))
        .collect();
    if team.is_empty() {
        return Vec::new();
    }
    let chart = generation.chart();
    let threats = threats(&team, generation);
    let gaps = offense_gaps(&team, generation);

    let mut found: Vec<(Suggestion, u16)> = pool
        .filter(|entry| {
            entry
                .generation()
                .is_some_and(|introduced| introduced <= generation.number())
        })
        // Nobody is suggested twice: not a member, and not another form of
        // one, which the party could not hold beside it anyway.
        .filter(|entry| {
            !party
                .iter()
                .any(|member| member.name == entry.name || member.species == entry.name)
        })
        .filter_map(|entry| {
            let typing = base_stats::typing(&entry.name)?;
            let types: Vec<&'static str> = typing.iter().collect();
            if !types.iter().all(|slug| chart.has(slug)) {
                return None;
            }
            let total = base_stats::lookup(&entry.name)?.get(StatKey::Total);
            let suggestion = weigh(&entry.name, types, chart, &threats, &gaps);
            (suggestion.score > 0).then_some((suggestion, total))
        })
        .collect();

    // Between two species the typing cannot tell apart, the stronger first,
    // and after that the name, so the order is the same every time.
    found.sort_by(|(a, a_total), (b, b_total)| {
        (Reverse(a.score), Reverse(*a_total), &a.name).cmp(&(
            Reverse(b.score),
            Reverse(*b_total),
            &b.name,
        ))
    });
    // One species per typing. Everything said here is said of a typing, so
    // Pawniard, Bisharp and Kingambit are one answer given three times, and
    // would crowd out the next five. The sort has put the strongest of each
    // first, which is the one kept.
    let mut seen: Vec<Vec<&'static str>> = Vec::new();
    found.retain(|(suggestion, _)| {
        let mut typing = suggestion.types.clone();
        typing.sort_unstable();
        let fresh = !seen.contains(&typing);
        if fresh {
            seen.push(typing);
        }
        fresh
    });
    found.truncate(limit);
    found
        .into_iter()
        .map(|(suggestion, _)| suggestion)
        .collect()
}

fn threats(team: &[&PokemonDetail], generation: Generation) -> Vec<Threat> {
    let chart = generation.chart();
    chart
        .types()
        .iter()
        .map(|&attacker| {
            let mut threat = Threat {
                attacker,
                weak: 0,
                resisted: false,
            };
            for member in team {
                let multiplier = typechart::combined(chart, attacker, member.types_in(generation));
                // The thresholds `team::analyse` uses, for its reason.
                if multiplier > 1.5 {
                    threat.weak += 1;
                } else if multiplier < 0.9 {
                    threat.resisted = true;
                }
            }
            threat
        })
        .collect()
}

fn offense_gaps(team: &[&PokemonDetail], generation: Generation) -> Vec<&'static str> {
    let chart = generation.chart();
    chart
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
        .collect()
}

/// Scores one typing against what the party needs.
fn weigh(
    name: &str,
    types: Vec<&'static str>,
    chart: Chart,
    threats: &[Threat],
    gaps: &[&'static str],
) -> Suggestion {
    let owned: Vec<String> = types.iter().map(|slug| slug.to_string()).collect();
    let mut score = 0;
    let mut covers = Vec::new();
    let mut shares = Vec::new();

    for threat in threats {
        let multiplier = typechart::combined(chart, threat.attacker, &owned);
        if multiplier < 0.9 {
            let mut worth = threat.weak * COVER_PER_WEAK_MEMBER;
            if !threat.resisted {
                worth += COVER_UNRESISTED;
            }
            // A resistance to something the party already handles is no
            // reason to add anyone, and is not listed as one.
            if worth > 0 {
                if multiplier == 0.0 {
                    worth += IMMUNITY_BONUS;
                }
                score += worth;
                covers.push(threat.attacker);
            }
        } else if multiplier > 1.5 && threat.weak > 0 {
            score -= threat.weak * SHARED_PER_WEAK_MEMBER;
            if multiplier > 3.0 {
                score -= DOUBLE_WEAKNESS;
            }
            shares.push(threat.attacker);
        }
    }

    let hits: Vec<&'static str> = gaps
        .iter()
        .copied()
        .filter(|defender| {
            types
                .iter()
                .any(|attacker| chart.effectiveness(attacker, defender) > 1.5)
        })
        .collect();
    score += hits.len() as i32 * HIT_GAP;

    Suggestion {
        name: name.to_string(),
        types,
        score,
        covers,
        hits,
        shares,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PastTyping;

    const LATEST: Generation = Generation::LATEST;

    fn member(name: &str, dex: u32, types: &[&str]) -> PokemonDetail {
        PokemonDetail {
            dex_number: dex,
            types: types.iter().map(|t| t.to_string()).collect(),
            ..crate::app::loaded(name)
        }
    }

    fn entries(list: &[(u32, &str)]) -> Vec<PokemonEntry> {
        list.iter()
            .map(|&(id, name)| PokemonEntry {
                id,
                name: name.to_string(),
            })
            .collect()
    }

    fn names(found: &[Suggestion]) -> Vec<&str> {
        found.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn an_empty_party_is_suggested_nothing() {
        let pool = entries(&[(227, "skarmory")]);
        assert!(suggest(&[], LATEST, pool.iter(), 8).is_empty());
    }

    #[test]
    fn a_typing_that_resists_what_the_party_is_weak_to_comes_first() {
        // Two Dragons share Ice, Dragon and Fairy. Steel resists all three;
        // another Dragon shares all three.
        let dragonite = member("dragonite", 149, &["dragon", "flying"]);
        let garchomp = member("garchomp", 445, &["dragon", "ground"]);
        let pool = entries(&[(373, "salamence"), (376, "metagross"), (143, "snorlax")]);
        let found = suggest(&[&dragonite, &garchomp], LATEST, pool.iter(), 8);

        assert_eq!(names(&found).first(), Some(&"metagross"));
        let metagross = &found[0];
        for covered in ["ice", "dragon", "fairy"] {
            assert!(metagross.covers.contains(&covered), "{covered}");
        }
        assert!(
            !names(&found).contains(&"salamence"),
            "sharing every weakness is not an improvement"
        );
    }

    #[test]
    fn a_shared_weakness_is_named_and_counted_against() {
        let charizard = member("charizard", 6, &["fire", "flying"]);
        let pool = entries(&[(146, "moltres"), (130, "gyarados")]);
        let found = suggest(&[&charizard], LATEST, pool.iter(), 8);
        // Gyarados resists the Water Charizard is weak to; Moltres is another
        // Fire/Flying, and doubles the Rock weakness with it.
        assert_eq!(names(&found).first(), Some(&"gyarados"));
        assert!(found[0].covers.contains(&"water"));
        if let Some(moltres) = found.iter().find(|s| s.name == "moltres") {
            assert!(moltres.shares.contains(&"rock"));
            assert!(moltres.score < found[0].score);
        }
    }

    #[test]
    fn a_resistance_the_party_already_has_is_not_given_as_a_reason() {
        // Gengar is not weak to Fighting and is immune to it, so a second
        // Fighting immunity fixes nothing.
        let gengar = member("gengar", 94, &["ghost", "poison"]);
        let pool = entries(&[(92, "gastly"), (477, "dusknoir")]);
        let found = suggest(&[&gengar], LATEST, pool.iter(), 8);
        for suggestion in &found {
            assert!(!suggestion.covers.contains(&"fighting"), "{suggestion:?}");
        }
    }

    #[test]
    fn hitting_what_nobody_hits_is_a_reason_of_its_own() {
        let snorlax = member("snorlax", 143, &["normal"]);
        let pool = entries(&[(68, "machamp")]);
        let found = suggest(&[&snorlax], LATEST, pool.iter(), 8);
        let machamp = &found[0];
        assert!(machamp.hits.contains(&"steel"));
        assert!(machamp.hits.contains(&"rock"));
    }

    #[test]
    fn members_and_their_other_forms_are_not_suggested() {
        let raichu = PokemonDetail {
            species: "raichu".to_string(),
            ..member("raichu-alola", 26, &["electric", "psychic"])
        };
        let gengar = member("gengar", 94, &["ghost", "poison"]);
        let pool = entries(&[(26, "raichu"), (94, "gengar"), (376, "metagross")]);
        let found = suggest(&[&raichu, &gengar], LATEST, pool.iter(), 8);
        assert!(!names(&found).contains(&"raichu"));
        assert!(!names(&found).contains(&"gengar"));
    }

    #[test]
    fn alternate_forms_in_the_pool_are_passed_over() {
        let charizard = member("charizard", 6, &["fire", "flying"]);
        let pool = entries(&[(10034, "charizard-mega-x"), (130, "gyarados")]);
        let found = suggest(&[&charizard], LATEST, pool.iter(), 8);
        assert_eq!(names(&found), ["gyarados"]);
    }

    #[test]
    fn an_older_generation_suggests_only_what_it_had_and_can_type() {
        let dragonite = member("dragonite", 149, &["dragon", "flying"]);
        let gen1 = Generation::new(1).unwrap();
        // Metagross arrived in Generation III. Magnemite was there, but as a
        // plain Electric type, which the table does not record.
        let pool = entries(&[(376, "metagross"), (81, "magnemite"), (131, "lapras")]);
        let found = suggest(&[&dragonite], gen1, pool.iter(), 8);
        assert!(!names(&found).contains(&"metagross"));
        assert!(!names(&found).contains(&"magnemite"));
    }

    #[test]
    fn a_member_is_read_as_it_was_typed_in_the_generation() {
        // Clefable was Normal until Generation VI, so under Generation V it
        // has a Fighting weakness for a Ghost to cover.
        let clefable = PokemonDetail {
            past_types: vec![PastTyping {
                until: 5,
                types: vec!["normal".to_string()],
            }],
            ..member("clefable", 36, &["fairy"])
        };
        let pool = entries(&[(94, "gengar")]);
        let gen5 = Generation::new(5).unwrap();
        let found = suggest(&[&clefable], gen5, pool.iter(), 8);
        assert!(found[0].covers.contains(&"fighting"));
    }

    #[test]
    fn a_typing_is_suggested_once_as_its_strongest_species() {
        let dragonite = member("dragonite", 149, &["dragon", "flying"]);
        let pool = entries(&[(624, "pawniard"), (625, "bisharp"), (983, "kingambit")]);
        let found = suggest(&[&dragonite], LATEST, pool.iter(), 8);
        assert_eq!(names(&found), ["kingambit"]);

        // The order the two types are listed in is not a different typing.
        let pool = entries(&[(707, "klefki"), (959, "tinkaton")]);
        let found = suggest(&[&dragonite], LATEST, pool.iter(), 8);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn the_list_stops_at_the_limit_and_keeps_the_best() {
        let dragonite = member("dragonite", 149, &["dragon", "flying"]);
        let pool = entries(&[
            (376, "metagross"),
            (227, "skarmory"),
            (208, "steelix"),
            (131, "lapras"),
            (36, "clefable"),
        ]);
        let all = suggest(&[&dragonite], LATEST, pool.iter(), 100);
        let top = suggest(&[&dragonite], LATEST, pool.iter(), 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top, all[..2]);
        assert!(all.windows(2).all(|pair| pair[0].score >= pair[1].score));
    }
}
