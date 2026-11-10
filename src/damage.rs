//! How much one species' move takes off another.
//!
//! The head-to-head card stops at the type chart: it says a hit is 2×, and
//! leaves what that comes to in hit points to a web calculator. The numbers
//! it would need are all already here, the two species' base stats and the
//! move's power, type and category, and the formula from those to a damage
//! range is deterministic.
//!
//! The arithmetic is the current games': the Generation V formula, with the
//! modifiers applied in the games' order and rounded the games' way, since
//! rounding anywhere else lands a point off for some matchups. A card's
//! generation changes the type chart and the two typings, as it does on
//! every other card, and nothing else. An older game's own formula, its
//! critical hits and its split of physical from special by type are not
//! modelled.
//!
//! What is left out is said on the card rather than guessed at. Abilities do
//! nothing except where one makes the defender certainly immune; a move that
//! hits several times is shown for one hit; a move whose power depends on
//! something the card cannot know has no number at all.

use std::cmp::Reverse;

use crate::models::{MoveInfo, PokemonDetail, StatKind};
use crate::statcalc::{self, Nature, MAX_IV};
use crate::team;
use crate::typechart::{self, Generation};

/// Effort values a trained stat holds, and what an untrained one does.
const TRAINED: u16 = 252;
/// The denominator of the games' fixed-point modifiers.
const ONE: u32 = 4096;
/// 1.5× and 0.5× in that fixed point.
const ONE_AND_A_HALF: u32 = 6144;
const HALF: u32 = 2048;
/// Life Orb's 1.3×, which the games store as this rather than as 5325.
const LIFE_ORB: u32 = 5324;
/// The lowest and highest of the sixteen random rolls, in percent.
const MIN_ROLL: u32 = 85;
const MAX_ROLL: u32 = 100;
/// How far a stat stage goes either way.
const MAX_STAGE: i8 = 6;

/// How much the attacker has put into the stat it hits with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Offence {
    /// No effort values, a neutral nature.
    Untrained,
    /// 252 effort values, a neutral nature.
    #[default]
    Trained,
    /// 252 effort values and a nature that raises the stat.
    Boosted,
}

impl Offence {
    fn next(self) -> Self {
        match self {
            Offence::Untrained => Offence::Trained,
            Offence::Trained => Offence::Boosted,
            Offence::Boosted => Offence::Untrained,
        }
    }

    /// As spreads are written: `0`, `252`, `252+`.
    pub fn label(self) -> &'static str {
        match self {
            Offence::Untrained => "0",
            Offence::Trained => "252",
            Offence::Boosted => "252+",
        }
    }

    fn evs(self) -> u16 {
        match self {
            Offence::Untrained => 0,
            Offence::Trained | Offence::Boosted => TRAINED,
        }
    }
}

/// How much the defender has put into taking the hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bulk {
    /// Nothing in HP or in the defending stat.
    #[default]
    Untrained,
    /// 252 HP, nothing in the defending stat.
    Hp,
    /// 252 HP, 252 in the defending stat and a nature that raises it.
    Full,
}

impl Bulk {
    fn next(self) -> Self {
        match self {
            Bulk::Untrained => Bulk::Hp,
            Bulk::Hp => Bulk::Full,
            Bulk::Full => Bulk::Untrained,
        }
    }

    /// The HP half and the defending half, as spreads are written.
    pub fn labels(self) -> (&'static str, &'static str) {
        match self {
            Bulk::Untrained => ("0", "0"),
            Bulk::Hp => ("252", "0"),
            Bulk::Full => ("252", "252+"),
        }
    }

    fn hp_evs(self) -> u16 {
        match self {
            Bulk::Untrained => 0,
            Bulk::Hp | Bulk::Full => TRAINED,
        }
    }

    fn defence(self) -> Offence {
        match self {
            Bulk::Untrained | Bulk::Hp => Offence::Untrained,
            Bulk::Full => Offence::Boosted,
        }
    }
}

/// What the attacker holds, of the items that change the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Item {
    #[default]
    None,
    /// 1.3× to the damage.
    LifeOrb,
    /// Choice Band or Choice Specs, whichever fits the move: 1.5× to the stat.
    Choice,
}

impl Item {
    fn next(self) -> Self {
        match self {
            Item::None => Item::LifeOrb,
            Item::LifeOrb => Item::Choice,
            Item::Choice => Item::None,
        }
    }

    /// In English in every language, as held items are everywhere else here.
    pub fn label(self) -> &'static str {
        match self {
            Item::None => "—",
            Item::LifeOrb => "Life Orb",
            Item::Choice => "Choice",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Clear,
    Sun,
    Rain,
    Sand,
    Snow,
}

impl Weather {
    fn next(self) -> Self {
        match self {
            Weather::Clear => Weather::Sun,
            Weather::Sun => Weather::Rain,
            Weather::Rain => Weather::Sand,
            Weather::Sand => Weather::Snow,
            Weather::Snow => Weather::Clear,
        }
    }
}

/// Everything about a hit that is not the two species and the move.
///
/// Kept by the app across species, as the stat calculator's spread is, so one
/// set of conditions can be read against target after target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup {
    pub level: u8,
    pub offence: Offence,
    pub bulk: Bulk,
    /// The attacker's stage in the stat it hits with, from -6 to +6.
    pub stage: i8,
    pub item: Item,
    pub weather: Weather,
    pub critical: bool,
}

impl Default for Setup {
    /// Level 50, as competitive play is levelled to, a fully trained attacker
    /// into an untrained defender: the usual first question, and the upper
    /// end of what the hit does.
    fn default() -> Self {
        Setup {
            level: 50,
            offence: Offence::default(),
            bulk: Bulk::default(),
            stage: 0,
            item: Item::default(),
            weather: Weather::default(),
            critical: false,
        }
    }
}

impl Setup {
    pub fn cycle_offence(&mut self) {
        self.offence = self.offence.next();
    }

    pub fn cycle_bulk(&mut self) {
        self.bulk = self.bulk.next();
    }

    pub fn cycle_item(&mut self) {
        self.item = self.item.next();
    }

    pub fn cycle_weather(&mut self) {
        self.weather = self.weather.next();
    }

    pub fn toggle_critical(&mut self) {
        self.critical = !self.critical;
    }

    /// Between the two levels battles are fought at.
    pub fn toggle_level(&mut self) {
        self.level = if self.level == 50 { 100 } else { 50 };
    }

    pub fn step_stage(&mut self, step: i8) {
        self.stage = (self.stage + step).clamp(-MAX_STAGE, MAX_STAGE);
    }
}

/// A damage range against a known HP total.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Damage {
    /// The lowest and highest of the sixteen rolls.
    pub min: u16,
    pub max: u16,
    /// The defender's HP at full health.
    pub hp: u16,
    /// What the type chart made of it, for the card to say.
    pub effectiveness: f32,
}

impl Damage {
    /// The range as tenths of a percent of the defender's HP.
    pub fn permille(self) -> (u32, u32) {
        let of_hp = |damage: u16| u32::from(damage) * 1000 / u32::from(self.hp.max(1));
        (of_hp(self.min), of_hp(self.max))
    }

    /// How many hits it takes to knock the defender out: with the highest
    /// roll every time, and with the lowest. Equal when the rolls cannot
    /// change the answer.
    pub fn hits_to_ko(self) -> (u16, u16) {
        let hits = |damage: u16| self.hp.div_ceil(damage.max(1));
        (hits(self.max), hits(self.min))
    }
}

/// What a move does to the defender.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// A status move: there is no damage to work out.
    Status,
    /// A damaging move whose number depends on something this cannot know:
    /// the user's weight, its remaining HP, how many times it hits.
    Varies,
    /// The defender's typing, or an ability it certainly has, takes nothing.
    Immune,
    Damage(Damage),
}

impl Outcome {
    /// Where the move goes in a list ordered by what it does: the hardest
    /// hits first, then what does nothing, then what has no number.
    pub fn rank(self) -> (u8, Reverse<u16>) {
        match self {
            Outcome::Damage(damage) => (0, Reverse(damage.max)),
            Outcome::Immune => (1, Reverse(0)),
            Outcome::Varies => (2, Reverse(0)),
            Outcome::Status => (3, Reverse(0)),
        }
    }
}

/// Moves that take a set amount whatever the stats, as long as the type
/// chart lets them land at all.
fn fixed_damage(slug: &str, level: u8, hp: u16) -> Option<u16> {
    match slug {
        "seismic-toss" | "night-shade" => Some(u16::from(level)),
        "dragon-rage" => Some(40),
        "sonic-boom" => Some(20),
        "super-fang" | "natures-madness" | "ruination" => Some((hp / 2).max(1)),
        _ => None,
    }
}

/// One of a species' final stats under `investment`, with perfect IVs.
fn stat(bases: [u16; 6], kind: StatKind, investment: Offence, level: u8) -> u32 {
    let index = crate::compare::STAT_ORDER
        .iter()
        .position(|&k| k == kind)
        .unwrap_or(0);
    let nature = match investment {
        Offence::Boosted => Nature::raising(kind),
        Offence::Untrained | Offence::Trained => Nature::default(),
    };
    u32::from(statcalc::final_stat(
        kind,
        bases[index],
        MAX_IV,
        investment.evs(),
        level,
        nature,
    ))
}

/// Applies a fixed-point modifier the way the games do: rounded to nearest,
/// with an exact half going down.
fn modify(value: u32, modifier: u32) -> u32 {
    (value * modifier + ONE / 2 - 1) / ONE
}

/// What `mv`, used by `attacker`, does to `defender` under `setup`, with both
/// read as they were typed in `generation`'s games.
pub fn calculate(
    attacker: &PokemonDetail,
    defender: &PokemonDetail,
    mv: &MoveInfo,
    setup: &Setup,
    generation: Generation,
) -> Outcome {
    if mv.damage_class == "status" {
        return Outcome::Status;
    }
    let (Some(attack_bases), Some(defence_bases)) =
        (statcalc::bases(attacker), statcalc::bases(defender))
    else {
        return Outcome::Varies;
    };
    let level = setup.level.clamp(1, statcalc::MAX_LEVEL);
    let chart = generation.chart();
    let attacker_types = attacker.types_in(generation);
    let defender_types = defender.types_in(generation);

    // A certain ability immunity overrules the chart, as on the matchup card.
    // One the defender merely might have is left alone: a maybe is not a
    // multiplier.
    let immune_by_ability = team::ability_immunities(defender, generation)
        .iter()
        .any(|immunity| immunity.certain && immunity.immune_to == mv.type_name);
    let effectiveness = match immune_by_ability {
        true => 0.0,
        false => typechart::combined(chart, &mv.type_name, defender_types),
    };
    if effectiveness == 0.0 {
        return Outcome::Immune;
    }

    let hp_base = defence_bases[0];
    let hp = statcalc::final_stat(
        StatKind::Hp,
        hp_base,
        MAX_IV,
        setup.bulk.hp_evs(),
        level,
        Nature::default(),
    );

    if let Some(amount) = fixed_damage(&mv.name, level, hp) {
        return Outcome::Damage(Damage {
            min: amount,
            max: amount,
            hp,
            effectiveness,
        });
    }
    let Some(power) = mv.power else {
        return Outcome::Varies;
    };

    let physical = mv.damage_class == "physical";
    // The handful of moves that read a different stat than their category
    // says. Body Press hits with the user's Defense, Foul Play with the
    // target's own Attack, and the Psyshock family is special into Defense.
    let mut attack = match mv.name.as_str() {
        "body-press" => stat(attack_bases, StatKind::Defense, setup.offence, level),
        "foul-play" => stat(defence_bases, StatKind::Attack, Offence::Untrained, level),
        _ if physical => stat(attack_bases, StatKind::Attack, setup.offence, level),
        _ => stat(attack_bases, StatKind::SpecialAttack, setup.offence, level),
    };
    let into_defence =
        physical || matches!(mv.name.as_str(), "psyshock" | "psystrike" | "secret-sword");
    let defence_kind = match into_defence {
        true => StatKind::Defense,
        false => StatKind::SpecialDefense,
    };
    let mut defence = stat(defence_bases, defence_kind, setup.bulk.defence(), level);

    // A critical hit ignores the attacker's drops, and keeps its boosts.
    let stage = match setup.critical {
        true => setup.stage.max(0),
        false => setup.stage,
    };
    let stage = i32::from(stage.clamp(-MAX_STAGE, MAX_STAGE));
    attack = match stage >= 0 {
        true => attack * (2 + stage as u32) / 2,
        false => attack * 2 / (2 + stage.unsigned_abs()),
    };
    if setup.item == Item::Choice {
        attack = modify(attack, ONE_AND_A_HALF);
    }
    // The two weathers that harden a type rather than strengthen a move.
    let has = |slug: &str| defender_types.iter().any(|t| t == slug);
    let hardened = match setup.weather {
        Weather::Sand => !into_defence && has("rock"),
        Weather::Snow => into_defence && has("ice"),
        _ => false,
    };
    if hardened {
        defence = modify(defence, ONE_AND_A_HALF);
    }

    let mut base = (2 * u32::from(level) / 5 + 2) * u32::from(power) * attack / defence.max(1);
    base = base / 50 + 2;

    let weather = match (setup.weather, mv.type_name.as_str()) {
        (Weather::Sun, "fire") | (Weather::Rain, "water") => ONE_AND_A_HALF,
        (Weather::Sun, "water") | (Weather::Rain, "fire") => HALF,
        _ => ONE,
    };
    base = modify(base, weather);
    if setup.critical {
        base = modify(base, ONE_AND_A_HALF);
    }

    let same_type = attacker_types.contains(&mv.type_name);
    // Every multiplier the chart can produce is a whole number of quarters,
    // so the type step stays in integers.
    let quarters = (effectiveness * 4.0).round() as u32;
    let roll = |percent: u32| {
        let mut damage = base * percent / 100;
        if same_type {
            damage = modify(damage, ONE_AND_A_HALF);
        }
        damage = damage * quarters / 4;
        if setup.item == Item::LifeOrb {
            damage = modify(damage, LIFE_ORB);
        }
        // A hit that lands always takes something.
        damage.clamp(1, u32::from(u16::MAX)) as u16
    };

    Outcome::Damage(Damage {
        min: roll(MIN_ROLL),
        max: roll(MAX_ROLL),
        hp,
        effectiveness,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::STAT_ORDER;
    use crate::models::{Ability, Stat};
    use std::collections::HashMap;

    const LATEST: Generation = Generation::LATEST;

    fn species(name: &str, dex: u32, types: &[&str], bases: [u16; 6]) -> PokemonDetail {
        PokemonDetail {
            dex_number: dex,
            types: types.iter().map(|t| t.to_string()).collect(),
            stats: STAT_ORDER
                .iter()
                .zip(bases)
                .map(|(&kind, base)| Stat { kind, base })
                .collect(),
            ..crate::app::loaded(name)
        }
    }

    fn garchomp() -> PokemonDetail {
        species(
            "garchomp",
            445,
            &["dragon", "ground"],
            [108, 130, 95, 80, 85, 102],
        )
    }

    fn gengar() -> PokemonDetail {
        species(
            "gengar",
            94,
            &["ghost", "poison"],
            [60, 65, 60, 130, 75, 110],
        )
    }

    fn skarmory() -> PokemonDetail {
        species(
            "skarmory",
            227,
            &["steel", "flying"],
            [65, 80, 140, 40, 70, 70],
        )
    }

    fn tyranitar() -> PokemonDetail {
        species(
            "tyranitar",
            248,
            &["rock", "dark"],
            [100, 134, 110, 95, 100, 61],
        )
    }

    fn a_move(name: &str, type_name: &str, class: &str, power: Option<u16>) -> MoveInfo {
        MoveInfo {
            name: name.to_string(),
            names: HashMap::new(),
            flavors: HashMap::new(),
            type_name: type_name.to_string(),
            damage_class: class.to_string(),
            power,
            accuracy: Some(100),
            pp: Some(10),
        }
    }

    fn earthquake() -> MoveInfo {
        a_move("earthquake", "ground", "physical", Some(100))
    }

    fn range(outcome: Outcome) -> (u16, u16) {
        match outcome {
            Outcome::Damage(damage) => (damage.min, damage.max),
            other => panic!("expected damage, got {other:?}"),
        }
    }

    #[test]
    fn a_trained_garchomp_earthquake_into_gengar_is_the_range_the_games_give() {
        // 182 Attack into 80 Defense at level 50: 22 × 100 × 182 / 80 = 5005,
        // / 50 + 2 = 102. The top roll is 102, × 1.5 for the same type = 153,
        // × 2 for the chart = 306; the bottom is 86, 129, 258. Gengar has
        // 135 HP.
        let outcome = calculate(
            &garchomp(),
            &gengar(),
            &earthquake(),
            &Setup::default(),
            LATEST,
        );
        assert_eq!(range(outcome), (258, 306));
        let Outcome::Damage(damage) = outcome else {
            unreachable!()
        };
        assert_eq!(damage.hp, 135);
        assert_eq!(damage.permille(), (1911, 2266));
        assert_eq!(damage.hits_to_ko(), (1, 1));
    }

    #[test]
    fn the_chart_decides_an_immunity_before_any_arithmetic() {
        let outcome = calculate(
            &garchomp(),
            &skarmory(),
            &earthquake(),
            &Setup::default(),
            LATEST,
        );
        assert_eq!(outcome, Outcome::Immune);
    }

    #[test]
    fn a_certain_ability_immunity_counts_and_a_possible_one_does_not() {
        let ability = |name: &str| Ability {
            name: name.to_string(),
            is_hidden: false,
        };
        let mut floating = gengar();
        floating.abilities = vec![ability("levitate")];
        let setup = Setup::default();
        assert_eq!(
            calculate(&garchomp(), &floating, &earthquake(), &setup, LATEST),
            Outcome::Immune
        );
        floating.abilities.push(ability("cursed-body"));
        assert!(matches!(
            calculate(&garchomp(), &floating, &earthquake(), &setup, LATEST),
            Outcome::Damage(_)
        ));
    }

    #[test]
    fn a_status_move_has_no_damage_and_a_powerless_one_no_number() {
        let setup = Setup::default();
        let dance = a_move("swords-dance", "normal", "status", None);
        assert_eq!(
            calculate(&garchomp(), &gengar(), &dance, &setup, LATEST),
            Outcome::Status
        );
        let knot = a_move("grass-knot", "grass", "special", None);
        assert_eq!(
            calculate(&gengar(), &garchomp(), &knot, &setup, LATEST),
            Outcome::Varies
        );
    }

    #[test]
    fn a_fixed_damage_move_ignores_the_stats_but_not_the_chart() {
        let toss = a_move("seismic-toss", "fighting", "physical", None);
        let setup = Setup::default();
        assert_eq!(
            range(calculate(&garchomp(), &skarmory(), &toss, &setup, LATEST)),
            (50, 50)
        );
        assert_eq!(
            calculate(&garchomp(), &gengar(), &toss, &setup, LATEST),
            Outcome::Immune
        );
    }

    #[test]
    fn training_and_a_nature_each_move_the_number_the_way_they_should() {
        let hit = |setup: &Setup| {
            range(calculate(
                &garchomp(),
                &tyranitar(),
                &earthquake(),
                setup,
                LATEST,
            ))
        };
        let mut setup = Setup {
            offence: Offence::Untrained,
            ..Setup::default()
        };
        let untrained = hit(&setup);
        setup.cycle_offence();
        let trained = hit(&setup);
        setup.cycle_offence();
        let boosted = hit(&setup);
        assert!(untrained.1 < trained.1 && trained.1 < boosted.1);

        setup.cycle_bulk();
        setup.cycle_bulk();
        assert_eq!(setup.bulk, Bulk::Full);
        assert!(hit(&setup).1 < boosted.1, "a trained wall takes less");
    }

    #[test]
    fn bulk_in_hp_raises_the_total_the_range_is_read_against() {
        let hp = |bulk: Bulk| {
            let setup = Setup {
                bulk,
                ..Setup::default()
            };
            match calculate(&garchomp(), &gengar(), &earthquake(), &setup, LATEST) {
                Outcome::Damage(damage) => damage.hp,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(hp(Bulk::Untrained), 135);
        assert_eq!(hp(Bulk::Hp), 167);
    }

    #[test]
    fn a_critical_hit_ignores_a_drop_and_keeps_a_boost() {
        let hit = |stage: i8, critical: bool| {
            let setup = Setup {
                stage,
                critical,
                ..Setup::default()
            };
            range(calculate(
                &garchomp(),
                &tyranitar(),
                &earthquake(),
                &setup,
                LATEST,
            ))
        };
        assert_eq!(hit(-2, true), hit(0, true));
        assert!(hit(2, true).1 > hit(0, true).1);
        assert!(hit(-2, false).1 < hit(0, false).1);
        assert!(hit(0, true).1 > hit(0, false).1);
    }

    #[test]
    fn sun_strengthens_fire_and_weakens_water() {
        let flamethrower = a_move("flamethrower", "fire", "special", Some(90));
        let surf = a_move("surf", "water", "special", Some(90));
        let hit = |mv: &MoveInfo, weather: Weather| {
            let setup = Setup {
                weather,
                ..Setup::default()
            };
            range(calculate(&gengar(), &garchomp(), mv, &setup, LATEST)).1
        };
        assert!(hit(&flamethrower, Weather::Sun) > hit(&flamethrower, Weather::Clear));
        assert!(hit(&surf, Weather::Sun) < hit(&surf, Weather::Clear));
        assert!(hit(&surf, Weather::Rain) > hit(&surf, Weather::Clear));
    }

    #[test]
    fn sand_hardens_a_rock_type_against_special_hits_only() {
        let surf = a_move("surf", "water", "special", Some(90));
        let hit = |mv: &MoveInfo, weather: Weather| {
            let setup = Setup {
                weather,
                ..Setup::default()
            };
            range(calculate(&garchomp(), &tyranitar(), mv, &setup, LATEST)).1
        };
        assert!(hit(&surf, Weather::Sand) < hit(&surf, Weather::Clear));
        assert_eq!(
            hit(&earthquake(), Weather::Sand),
            hit(&earthquake(), Weather::Clear)
        );
    }

    #[test]
    fn a_choice_item_and_a_life_orb_each_add_their_share() {
        let hit = |item: Item| {
            let setup = Setup {
                item,
                ..Setup::default()
            };
            range(calculate(
                &garchomp(),
                &tyranitar(),
                &earthquake(),
                &setup,
                LATEST,
            ))
            .1
        };
        let plain = u32::from(hit(Item::None));
        let orb = u32::from(hit(Item::LifeOrb));
        let choice = u32::from(hit(Item::Choice));
        assert_eq!(orb, modify(plain, LIFE_ORB));
        assert!(choice > orb, "1.5× to the stat beats 1.3× to the damage");
    }

    #[test]
    fn psyshock_is_a_special_move_into_physical_defence() {
        // Skarmory's Defense is twice its Special Defense, so a special move
        // read against it does far less.
        let psychic = a_move("psychic", "psychic", "special", Some(80));
        let psyshock = a_move("psyshock", "psychic", "special", Some(80));
        let setup = Setup::default();
        let into_special = range(calculate(&gengar(), &skarmory(), &psychic, &setup, LATEST));
        let into_physical = range(calculate(&gengar(), &skarmory(), &psyshock, &setup, LATEST));
        assert!(into_physical.1 < into_special.1);
    }

    #[test]
    fn foul_play_hits_with_the_targets_own_attack() {
        let foul = a_move("foul-play", "dark", "physical", Some(95));
        let setup = Setup::default();
        let untrained = range(calculate(&skarmory(), &gengar(), &foul, &setup, LATEST));
        let mut training = setup;
        training.cycle_offence();
        assert_eq!(
            untrained,
            range(calculate(&skarmory(), &gengar(), &foul, &training, LATEST)),
            "the user's own training changes nothing"
        );
    }

    #[test]
    fn an_older_generation_reads_its_own_chart() {
        // Ghost did nothing to Psychic in Generation I.
        let alakazam = species("alakazam", 65, &["psychic"], [55, 50, 45, 135, 95, 120]);
        let lick = a_move("lick", "ghost", "physical", Some(30));
        let setup = Setup::default();
        let gen1 = Generation::new(1).unwrap();
        assert_eq!(
            calculate(&gengar(), &alakazam, &lick, &setup, gen1),
            Outcome::Immune
        );
        assert!(matches!(
            calculate(&gengar(), &alakazam, &lick, &setup, LATEST),
            Outcome::Damage(damage) if damage.effectiveness == 2.0
        ));
    }

    #[test]
    fn a_hit_that_lands_always_takes_at_least_one_point() {
        let tickle = a_move("tackle", "normal", "physical", Some(1));
        let setup = Setup {
            level: 1,
            offence: Offence::Untrained,
            bulk: Bulk::Full,
            ..Setup::default()
        };
        let (min, _) = range(calculate(&gengar(), &skarmory(), &tickle, &setup, LATEST));
        assert_eq!(min, 1);
    }

    #[test]
    fn hits_to_ko_differ_when_the_roll_decides() {
        let damage = Damage {
            min: 45,
            max: 54,
            hp: 100,
            effectiveness: 1.0,
        };
        assert_eq!(damage.hits_to_ko(), (2, 3));
        assert_eq!(damage.permille(), (450, 540));
    }

    #[test]
    fn the_hardest_hits_sort_first_and_status_moves_last() {
        let hit = |max| {
            Outcome::Damage(Damage {
                min: 1,
                max,
                hp: 100,
                effectiveness: 1.0,
            })
        };
        let mut outcomes = [
            Outcome::Status,
            hit(40),
            Outcome::Varies,
            hit(90),
            Outcome::Immune,
        ];
        outcomes.sort_by_key(|outcome| outcome.rank());
        assert_eq!(
            outcomes,
            [
                hit(90),
                hit(40),
                Outcome::Immune,
                Outcome::Varies,
                Outcome::Status
            ]
        );
    }

    #[test]
    fn the_stage_stops_at_six_either_way() {
        let mut setup = Setup::default();
        for _ in 0..10 {
            setup.step_stage(1);
        }
        assert_eq!(setup.stage, 6);
        for _ in 0..20 {
            setup.step_stage(-1);
        }
        assert_eq!(setup.stage, -6);
    }
}
