//! Every entry's base stats, built into the binary.
//!
//! The master list carries names and ids and nothing else; base stats live on
//! `/pokemon/{name}`, one request per entry. That is fine for the info panel,
//! which shows one species, and hopeless for a search term or a sort order,
//! which has to know all thirteen hundred at once. So the table is generated
//! ahead of time from PokeAPI's own CSV data by `scripts/base-stats.py` and
//! compiled in, the same way the type chart is: instant, offline, and the same
//! answer whatever happens to be cached.
//!
//! An entry newer than the table has no row. Stat terms then leave it out and
//! the stat sorts put it last, rather than guessing; regenerating the table
//! brings it in.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The generated table: a `#` comment header, then one tab-separated row per
/// entry, `name hp atk def spa spd spe`.
const TABLE: &str = include_str!("base_stats.tsv");

/// One of the six base stats, or their total — anything a stat term or a stat
/// sort can be keyed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKey {
    Hp,
    Attack,
    Defense,
    SpecialAttack,
    SpecialDefense,
    Speed,
    Total,
}

impl StatKey {
    /// Reads the key of a search term. The short names are the ones every
    /// community tool writes; the long ones are there so that spelling a stat
    /// out also works.
    pub fn parse(key: &str) -> Option<Self> {
        match key.to_lowercase().as_str() {
            "hp" => Some(Self::Hp),
            "atk" | "attack" => Some(Self::Attack),
            "def" | "defense" => Some(Self::Defense),
            "spa" | "spatk" => Some(Self::SpecialAttack),
            "spd" | "spdef" => Some(Self::SpecialDefense),
            "spe" | "speed" => Some(Self::Speed),
            "bst" | "total" => Some(Self::Total),
            _ => None,
        }
    }
}

/// An entry's six base stats, in the order the info panel draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseStats([u8; 6]);

impl BaseStats {
    pub fn get(self, key: StatKey) -> u16 {
        let [hp, atk, def, spa, spd, spe] = self.0.map(u16::from);
        match key {
            StatKey::Hp => hp,
            StatKey::Attack => atk,
            StatKey::Defense => def,
            StatKey::SpecialAttack => spa,
            StatKey::SpecialDefense => spd,
            StatKey::Speed => spe,
            StatKey::Total => hp + atk + def + spa + spd + spe,
        }
    }
}

/// The base stats of the entry PokeAPI calls `name`, alternate forms included.
pub fn lookup(name: &str) -> Option<BaseStats> {
    static PARSED: OnceLock<HashMap<&'static str, BaseStats>> = OnceLock::new();
    PARSED.get_or_init(|| parse(TABLE)).get(name).copied()
}

/// Reads the table. A malformed row is skipped rather than fatal; the test
/// below holds the table to having none.
fn parse(table: &'static str) -> HashMap<&'static str, BaseStats> {
    table
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(parse_row)
        .collect()
}

fn parse_row(line: &'static str) -> Option<(&'static str, BaseStats)> {
    let mut fields = line.split('\t');
    let name = fields.next()?;
    let mut stats = [0u8; 6];
    for stat in &mut stats {
        *stat = fields.next()?.parse().ok()?;
    }
    fields.next().is_none().then_some((name, BaseStats(stats)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MAX_DEX_NUMBER;

    #[test]
    fn every_row_of_the_table_parses() {
        let rows = TABLE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty());
        for row in rows {
            assert!(parse_row(row).is_some(), "malformed row: {row:?}");
        }
    }

    #[test]
    fn the_table_is_no_older_than_the_dex() {
        // The script writes rows in PokeAPI id order, species first, and
        // species are numbered 1..=MAX_DEX_NUMBER without gaps. A table with
        // fewer rows than that predates the newest generation, and stat terms
        // would silently drop it.
        let rows = TABLE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .count();
        assert!(rows >= MAX_DEX_NUMBER as usize);
        assert!(lookup("bulbasaur").is_some());
    }

    #[test]
    fn known_species_read_back_their_stats() {
        let shuckle = lookup("shuckle").unwrap();
        assert_eq!(shuckle.get(StatKey::Defense), 230);
        assert_eq!(shuckle.get(StatKey::Speed), 5);
        assert_eq!(lookup("ninjask").unwrap().get(StatKey::Speed), 160);
        assert_eq!(lookup("blissey").unwrap().get(StatKey::Hp), 255);
    }

    #[test]
    fn the_total_is_the_sum_of_the_six() {
        assert_eq!(lookup("pikachu").unwrap().get(StatKey::Total), 320);
        assert_eq!(lookup("arceus").unwrap().get(StatKey::Total), 720);
    }

    #[test]
    fn alternate_forms_have_rows_of_their_own() {
        // Alolan Raichu trades Attack for Special Attack; a table keyed by
        // species would give both the same answer.
        let alola = lookup("raichu-alola").unwrap();
        let kanto = lookup("raichu").unwrap();
        assert_eq!(alola.get(StatKey::SpecialAttack), 95);
        assert_eq!(kanto.get(StatKey::SpecialAttack), 90);
    }

    #[test]
    fn an_unknown_name_has_no_stats() {
        assert_eq!(lookup("missingno"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn stat_keys_answer_to_short_and_long_names_in_any_case() {
        assert_eq!(StatKey::parse("spe"), Some(StatKey::Speed));
        assert_eq!(StatKey::parse("Speed"), Some(StatKey::Speed));
        assert_eq!(StatKey::parse("BST"), Some(StatKey::Total));
        assert_eq!(StatKey::parse("spdef"), Some(StatKey::SpecialDefense));
        assert_eq!(StatKey::parse("luck"), None);
    }
}
