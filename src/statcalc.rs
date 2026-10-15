//! What a species' stats come to in battle.
//!
//! The info panel shows base stats, which are a property of the species. What
//! a Pokemon actually has also depends on its level, its nature, and the IVs
//! and EVs it carries in each stat — and the formula from those to the number
//! on the summary screen is deterministic, so the app can answer it rather than
//! send the reader to a web calculator.
//!
//! The arithmetic and the editing rules both live here, with no terminal in
//! sight: `app.rs` maps keys to the methods on [`Calculator`], and `ui.rs`
//! draws what it holds.

use crate::compare::STAT_ORDER;
use crate::models::{PokemonDetail, StatKind};

/// Highest level a Pokemon can reach.
pub const MAX_LEVEL: u8 = 100;
/// Highest IV in any stat.
pub const MAX_IV: u8 = 31;
/// Most EVs one stat can usefully hold. More is flagged, not refused: a spread
/// copied from an old guide (where the cap was 255) should still compute.
pub const STAT_EV_CAP: u16 = 252;
/// Most EVs a field accepts, which is the old per-stat cap.
pub const MAX_EV: u16 = 255;
/// Most EVs a Pokemon can hold across all six stats.
pub const TOTAL_EV_CAP: u16 = 510;

/// The five stats a nature can raise or lower, in the order the natures are
/// numbered by. HP is never one of them.
const NATURE_STATS: [StatKind; 5] = [
    StatKind::Attack,
    StatKind::Defense,
    StatKind::Speed,
    StatKind::SpecialAttack,
    StatKind::SpecialDefense,
];

/// The natures in the games' own order. That order is a 5×5 grid — nature
/// `i` raises `NATURE_STATS[i / 5]` and lowers `NATURE_STATS[i % 5]` — so the
/// effects are worked out from the position rather than tabled by hand, and
/// the five on the diagonal, which raise and lower the same stat, are the
/// neutral ones.
///
/// The names stay in English in every interface language, as Showdown and
/// most calculators write them; they are proper nouns a spread is pasted with.
const NATURES: [&str; 25] = [
    "Hardy", "Lonely", "Brave", "Adamant", "Naughty", //
    "Bold", "Docile", "Relaxed", "Impish", "Lax", //
    "Timid", "Hasty", "Serious", "Jolly", "Naive", //
    "Modest", "Mild", "Quiet", "Bashful", "Rash", //
    "Calm", "Gentle", "Sassy", "Careful", "Quirky",
];

/// One of the 25 natures, by its index in the games' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nature(u8);

impl Nature {
    /// Looked up by name; how tests spell a spread.
    #[cfg(test)]
    pub fn named(name: &str) -> Option<Self> {
        NATURES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|idx| Nature(idx as u8))
    }

    pub fn name(self) -> &'static str {
        NATURES[self.0 as usize]
    }

    /// The stat this nature raises by a tenth, or `None` for a neutral one.
    pub fn raised(self) -> Option<StatKind> {
        let (up, down) = (self.0 / 5, self.0 % 5);
        (up != down).then_some(NATURE_STATS[up as usize])
    }

    /// The stat this nature lowers by a tenth, or `None` for a neutral one.
    pub fn lowered(self) -> Option<StatKind> {
        let (up, down) = (self.0 / 5, self.0 % 5);
        (up != down).then_some(NATURE_STATS[down as usize])
    }

    /// The next nature in the games' order, wrapping, `step` places on.
    fn cycled(self, step: i32) -> Self {
        Nature((i32::from(self.0) + step).rem_euclid(NATURES.len() as i32) as u8)
    }
}

/// One stat's final value, as the games compute it.
///
/// Every division truncates, and in this order; rounding anywhere else gives
/// numbers a point off for some spreads. The nature multiplier is applied in
/// whole percent for the same reason — `× 1.1` in floating point is not
/// exactly a tenth.
///
/// Shedinja is the one exception: its HP is 1 whatever it is trained in. It
/// is also the only species with a base HP of 1, which is how it is told apart
/// without passing a name into arithmetic.
pub fn final_stat(kind: StatKind, base: u16, iv: u8, ev: u16, level: u8, nature: Nature) -> u16 {
    let level = u32::from(level.clamp(1, MAX_LEVEL));
    let core = (2 * u32::from(base) + u32::from(iv) + u32::from(ev) / 4) * level / 100;
    let value = match kind {
        StatKind::Hp if base == 1 => 1,
        StatKind::Hp => core + level + 10,
        _ => {
            let percent = if nature.raised() == Some(kind) {
                110
            } else if nature.lowered() == Some(kind) {
                90
            } else {
                100
            };
            (core + 5) * percent / 100
        }
    };
    value as u16
}

/// A species' six base stats in [`STAT_ORDER`], or `None` if its record is
/// missing any of them.
pub fn bases(detail: &PokemonDetail) -> Option<[u16; 6]> {
    let mut out = [0; 6];
    for (slot, kind) in out.iter_mut().zip(STAT_ORDER) {
        *slot = detail.stats.iter().find(|s| s.kind == kind)?.base;
    }
    Some(out)
}

/// The rows of the card, top to bottom: level, nature, then one per stat in
/// [`STAT_ORDER`].
pub const ROWS: usize = 2 + STAT_ORDER.len();
const LEVEL_ROW: usize = 0;
const NATURE_ROW: usize = 1;

/// Which of a stat row's two editable cells the cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Column {
    #[default]
    Iv,
    Ev,
}

/// The cell the cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Level,
    Nature,
    Iv(usize),
    Ev(usize),
}

/// The spread being edited, and where the cursor is in it.
///
/// Kept by the app across species rather than per record, so the same spread
/// can be read off one species after another — which is most of what a
/// calculator is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calculator {
    pub level: u8,
    pub nature: Nature,
    pub ivs: [u8; 6],
    pub evs: [u16; 6],
    pub row: usize,
    pub column: Column,
    /// Whether the next digit starts the value over rather than extending it.
    /// Set whenever the cursor lands on a cell, so typing a number replaces
    /// the one there the way a spreadsheet cell does.
    fresh: bool,
}

impl Default for Calculator {
    /// Level 50, as competitive play is levelled to; perfect IVs and no EVs,
    /// the usual starting point of a spread; a neutral nature.
    fn default() -> Self {
        Calculator {
            level: 50,
            nature: Nature::default(),
            ivs: [MAX_IV; 6],
            evs: [0; 6],
            row: LEVEL_ROW,
            column: Column::Iv,
            fresh: true,
        }
    }
}

impl Calculator {
    /// The cell under the cursor.
    pub fn field(&self) -> Field {
        match (self.row, self.column) {
            (LEVEL_ROW, _) => Field::Level,
            (NATURE_ROW, _) => Field::Nature,
            (row, Column::Iv) => Field::Iv(row - 2),
            (row, Column::Ev) => Field::Ev(row - 2),
        }
    }

    /// Up or down a row, wrapping. The column is kept, so running down the EV
    /// column stays in it.
    pub fn move_row(&mut self, step: i32) {
        self.row = (self.row as i32 + step).rem_euclid(ROWS as i32) as usize;
        self.fresh = true;
    }

    /// Left or right. On a stat row that is between its IV and EV; level and
    /// nature have one value each, so there it steps the value instead, which
    /// is what makes the nature something arrows can pick.
    pub fn move_column(&mut self, step: i32) {
        match self.field() {
            Field::Level | Field::Nature => self.step(step),
            Field::Iv(_) | Field::Ev(_) => {
                self.column = match self.column {
                    Column::Iv => Column::Ev,
                    Column::Ev => Column::Iv,
                };
                self.fresh = true;
            }
        }
    }

    /// `+` and `-`. EVs move in fours, since only every fourth one changes a
    /// stat, and stop at the per-stat cap; typing goes past it.
    pub fn step(&mut self, step: i32) {
        match self.field() {
            Field::Level => {
                self.level = (i32::from(self.level) + step).clamp(1, i32::from(MAX_LEVEL)) as u8;
            }
            Field::Nature => self.nature = self.nature.cycled(step),
            Field::Iv(i) => {
                self.ivs[i] = (i32::from(self.ivs[i]) + step).clamp(0, i32::from(MAX_IV)) as u8;
            }
            Field::Ev(i) => {
                let ev = i32::from(self.evs[i]);
                // From an off-grid value, the first step lands on the grid.
                let next = if step > 0 {
                    (ev / 4 + step) * 4
                } else {
                    (ev + 3) / 4 * 4 + step * 4
                };
                // A typed value past the cap is not pulled back by stepping up.
                let cap = i32::from(STAT_EV_CAP).max(ev);
                self.evs[i] = next.clamp(0, cap) as u16;
            }
        }
        self.fresh = true;
    }

    /// A digit typed on the cell. The first after the cursor arrives replaces
    /// the value, later ones append, and a value past the field's limit is
    /// held at the limit rather than refused.
    pub fn type_digit(&mut self, digit: u8) {
        let digit = u32::from(digit);
        let extend = |value: u32, max: u32, fresh: bool| match fresh {
            true => digit.min(max),
            false => (value * 10 + digit).min(max),
        };
        match self.field() {
            Field::Level => {
                // Level 0 does not exist, but it is what typing `0` on the way
                // to `05` passes through; the arithmetic reads it as 1.
                self.level = extend(self.level.into(), MAX_LEVEL.into(), self.fresh) as u8;
            }
            Field::Nature => return,
            Field::Iv(i) => {
                self.ivs[i] = extend(self.ivs[i].into(), MAX_IV.into(), self.fresh) as u8
            }
            Field::Ev(i) => {
                self.evs[i] = extend(self.evs[i].into(), MAX_EV.into(), self.fresh) as u16
            }
        }
        self.fresh = false;
    }

    /// Backspace: the last digit off the cell's value.
    pub fn backspace(&mut self) {
        match self.field() {
            Field::Level => self.level /= 10,
            Field::Nature => return,
            Field::Iv(i) => self.ivs[i] /= 10,
            Field::Ev(i) => self.evs[i] /= 10,
        }
        self.fresh = false;
    }

    /// Back to the defaults, keeping the cursor where it is.
    pub fn reset(&mut self) {
        *self = Calculator {
            row: self.row,
            column: self.column,
            ..Calculator::default()
        };
    }

    /// The six final stats for `bases`, in [`STAT_ORDER`].
    pub fn finals(&self, bases: [u16; 6]) -> [u16; 6] {
        std::array::from_fn(|i| {
            final_stat(
                STAT_ORDER[i],
                bases[i],
                self.ivs[i],
                self.evs[i],
                self.level,
                self.nature,
            )
        })
    }

    /// EVs spent across all six stats.
    pub fn ev_total(&self) -> u16 {
        self.evs.iter().sum()
    }

    /// Whether the spread holds more EVs than a Pokemon can.
    pub fn over_total(&self) -> bool {
        self.ev_total() > TOTAL_EV_CAP
    }

    /// Whether stat `i` holds more EVs than one stat can.
    pub fn over_stat(&self, i: usize) -> bool {
        self.evs[i] > STAT_EV_CAP
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GARCHOMP: [u16; 6] = [108, 130, 95, 80, 85, 102];

    fn spread(level: u8, nature: &str, evs: [u16; 6]) -> Calculator {
        Calculator {
            level,
            nature: Nature::named(nature).expect("a nature"),
            evs,
            ..Calculator::default()
        }
    }

    #[test]
    fn a_level_100_jolly_garchomp_has_the_stats_the_games_show() {
        // 252 Atk / 4 Def / 252 Spe, the spread every guide quotes.
        let calc = spread(100, "Jolly", [0, 252, 4, 0, 0, 252]);
        assert_eq!(calc.finals(GARCHOMP), [357, 359, 227, 176, 206, 333]);
    }

    #[test]
    fn level_50_is_what_competitive_play_reads() {
        let calc = spread(50, "Jolly", [0, 252, 4, 0, 0, 252]);
        assert_eq!(calc.finals(GARCHOMP), [183, 182, 116, 90, 105, 169]);
    }

    #[test]
    fn shedinja_has_one_hp_whatever_it_is_trained_in() {
        assert_eq!(
            final_stat(StatKind::Hp, 1, 31, 252, 100, Nature::default()),
            1
        );
    }

    #[test]
    fn natures_raise_and_lower_by_their_place_in_the_grid() {
        let effect = |name| {
            let n = Nature::named(name).unwrap();
            (n.raised(), n.lowered())
        };
        use StatKind::*;
        assert_eq!(effect("Adamant"), (Some(Attack), Some(SpecialAttack)));
        assert_eq!(effect("Jolly"), (Some(Speed), Some(SpecialAttack)));
        assert_eq!(effect("Timid"), (Some(Speed), Some(Attack)));
        assert_eq!(effect("Modest"), (Some(SpecialAttack), Some(Attack)));
        assert_eq!(effect("Bold"), (Some(Defense), Some(Attack)));
        assert_eq!(
            effect("Careful"),
            (Some(SpecialDefense), Some(SpecialAttack))
        );
        assert_eq!(effect("Brave"), (Some(Attack), Some(Speed)));
        for neutral in ["Hardy", "Docile", "Serious", "Bashful", "Quirky"] {
            assert_eq!(effect(neutral), (None, None), "{neutral}");
        }
    }

    #[test]
    fn hp_ignores_the_nature() {
        let plain = final_stat(StatKind::Hp, 108, 31, 0, 50, Nature::default());
        for i in 0..25 {
            assert_eq!(final_stat(StatKind::Hp, 108, 31, 0, 50, Nature(i)), plain);
        }
    }

    #[test]
    fn a_calculator_opens_on_level_50_perfect_ivs_and_no_evs() {
        let calc = Calculator::default();
        assert_eq!((calc.level, calc.ivs, calc.evs), (50, [31; 6], [0; 6]));
        assert_eq!(calc.field(), Field::Level);
    }

    #[test]
    fn digits_replace_the_value_on_arrival_and_extend_it_after() {
        let mut calc = Calculator::default();
        calc.type_digit(1);
        calc.type_digit(0);
        calc.type_digit(0);
        assert_eq!(calc.level, 100);
        // A fourth digit would be past the limit, so the value holds there.
        calc.type_digit(5);
        assert_eq!(calc.level, 100);
        calc.backspace();
        assert_eq!(calc.level, 10);

        // Moving away and back starts the next number over.
        calc.move_row(1);
        calc.move_row(-1);
        calc.type_digit(7);
        assert_eq!(calc.level, 7);
    }

    #[test]
    fn arrows_cross_between_iv_and_ev_and_keep_the_column_down_the_rows() {
        let mut calc = Calculator::default();
        calc.move_row(2);
        assert_eq!(calc.field(), Field::Iv(0));
        calc.move_column(1);
        assert_eq!(calc.field(), Field::Ev(0));
        calc.move_row(5);
        assert_eq!(calc.field(), Field::Ev(5));
        // And off the bottom back to the top.
        calc.move_row(1);
        assert_eq!(calc.field(), Field::Level);
    }

    #[test]
    fn arrows_on_the_nature_row_pick_the_nature() {
        let mut calc = Calculator::default();
        calc.move_row(1);
        calc.move_column(1);
        assert_eq!(calc.nature.name(), "Lonely");
        calc.move_column(-2);
        assert_eq!(calc.nature.name(), "Quirky", "wraps");
        // A digit means nothing to a nature.
        calc.type_digit(3);
        assert_eq!(calc.nature.name(), "Quirky");
    }

    #[test]
    fn evs_step_in_fours_and_stop_at_the_stat_cap() {
        let mut calc = Calculator::default();
        calc.move_row(2);
        calc.move_column(1);
        calc.step(1);
        assert_eq!(calc.evs[0], 4);
        calc.evs[0] = 250;
        calc.step(1);
        assert_eq!(calc.evs[0], 252);
        calc.step(1);
        assert_eq!(calc.evs[0], 252);
        calc.evs[0] = 6;
        calc.step(-1);
        assert_eq!(calc.evs[0], 4, "an off-grid value steps onto the grid");
        calc.step(-1);
        calc.step(-1);
        assert_eq!(calc.evs[0], 0);
    }

    #[test]
    fn spreads_past_the_caps_are_flagged_rather_than_refused() {
        let mut calc = Calculator::default();
        calc.move_row(2);
        calc.move_column(1);
        for digit in [2, 5, 5] {
            calc.type_digit(digit);
        }
        assert_eq!(calc.evs[0], 255);
        assert!(calc.over_stat(0));
        assert!(!calc.over_total());

        calc.evs = [252, 252, 8, 0, 0, 0];
        assert!(!calc.over_stat(2));
        assert!(calc.over_total());
        assert_eq!(calc.ev_total(), 512);
    }

    #[test]
    fn reset_restores_the_defaults_but_not_the_cursor() {
        let mut calc = spread(100, "Adamant", [252; 6]);
        calc.move_row(3);
        calc.reset();
        assert_eq!(calc.level, 50);
        assert_eq!(calc.evs, [0; 6]);
        assert_eq!(calc.row, 3);
    }
}
