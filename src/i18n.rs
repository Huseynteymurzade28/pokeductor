//! Localization layer.
//!
//! Every user-facing string flows through [`Language::strings`]. Because the UI
//! re-reads these on every frame, switching language (the `L` hotkey) updates
//! the entire interface instantly with no extra bookkeeping.

use crate::models::StatKind;

/// Supported interface languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Turkish,
    German,
}

impl Language {
    /// Cycles to the next language (wraps around). Bound to `L`.
    pub fn next(self) -> Self {
        match self {
            Language::English => Language::Turkish,
            Language::Turkish => Language::German,
            Language::German => Language::English,
        }
    }

    /// Short tag shown in the status bar, e.g. `EN`.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "EN",
            Language::Turkish => "TR",
            Language::German => "DE",
        }
    }

    /// The full translation table for this language.
    pub fn strings(self) -> Strings {
        match self {
            Language::English => Strings::english(),
            Language::Turkish => Strings::turkish(),
            Language::German => Strings::german(),
        }
    }

    /// Localized label for a single base stat.
    pub fn stat_label(self, kind: StatKind) -> &'static str {
        let s = self.strings();
        match kind {
            StatKind::Hp => s.stat_hp,
            StatKind::Attack => s.stat_attack,
            StatKind::Defense => s.stat_defense,
            StatKind::SpecialAttack => s.stat_sp_attack,
            StatKind::SpecialDefense => s.stat_sp_defense,
            StatKind::Speed => s.stat_speed,
        }
    }
}

/// A fully populated set of UI strings. Using a struct of `&'static str` keeps
/// translations explicit and lets the compiler catch any missing field.
#[derive(Debug, Clone, Copy)]
pub struct Strings {
    pub app_title: &'static str,
    pub sidebar_title: &'static str,
    pub search_title: &'static str,
    pub details_title: &'static str,
    pub evolution_title: &'static str,
    pub loading: &'static str,
    pub loading_list: &'static str,
    pub no_selection: &'static str,
    pub no_results: &'static str,
    pub no_evolution: &'static str,
    pub types_label: &'static str,
    pub height_label: &'static str,
    pub weight_label: &'static str,
    pub total_label: &'static str,
    pub error_prefix: &'static str,
    pub stat_hp: &'static str,
    pub stat_attack: &'static str,
    pub stat_defense: &'static str,
    pub stat_sp_attack: &'static str,
    pub stat_sp_defense: &'static str,
    pub stat_speed: &'static str,
    pub help: &'static str,
}

impl Strings {
    fn english() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex & Evolution Analyzer ",
            sidebar_title: " Pokemon ",
            search_title: " Search ",
            details_title: " Details ",
            evolution_title: " Evolution Chain ",
            loading: "Loading",
            loading_list: "Fetching Pokedex",
            no_selection: "Select a Pokemon and press Enter",
            no_results: "No Pokemon match your search",
            no_evolution: "No evolution data",
            types_label: "Types",
            height_label: "Height",
            weight_label: "Weight",
            total_label: "Total",
            error_prefix: "Error",
            stat_hp: "HP",
            stat_attack: "Attack",
            stat_defense: "Defense",
            stat_sp_attack: "Sp. Atk",
            stat_sp_defense: "Sp. Def",
            stat_speed: "Speed",
            help: " ↑/↓ Navigate · Enter Select · Tab Focus · / Search · L Language · Q Quit ",
        }
    }

    fn turkish() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex ve Evrim Analizcisi ",
            sidebar_title: " Pokemonlar ",
            search_title: " Ara ",
            details_title: " Ayrıntılar ",
            evolution_title: " Evrim Zinciri ",
            loading: "Yükleniyor",
            loading_list: "Pokedex getiriliyor",
            no_selection: "Bir Pokemon seçip Enter'a basın",
            no_results: "Aramanızla eşleşen Pokemon yok",
            no_evolution: "Evrim verisi yok",
            types_label: "Türler",
            height_label: "Boy",
            weight_label: "Ağırlık",
            total_label: "Toplam",
            error_prefix: "Hata",
            stat_hp: "CAN",
            stat_attack: "Saldırı",
            stat_defense: "Savunma",
            stat_sp_attack: "Öz. Sal",
            stat_sp_defense: "Öz. Sav",
            stat_speed: "Hız",
            help: " ↑/↓ Gezin · Enter Seç · Tab Odak · / Ara · L Dil · Q Çıkış ",
        }
    }

    fn german() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex & Evolutions-Analyse ",
            sidebar_title: " Pokemon ",
            search_title: " Suche ",
            details_title: " Details ",
            evolution_title: " Entwicklungsreihe ",
            loading: "Lädt",
            loading_list: "Pokedex wird geladen",
            no_selection: "Wähle ein Pokemon und drücke Enter",
            no_results: "Keine Pokemon gefunden",
            no_evolution: "Keine Entwicklungsdaten",
            types_label: "Typen",
            height_label: "Größe",
            weight_label: "Gewicht",
            total_label: "Summe",
            error_prefix: "Fehler",
            stat_hp: "KP",
            stat_attack: "Angriff",
            stat_defense: "Verteid.",
            stat_sp_attack: "Sp. Ang",
            stat_sp_defense: "Sp. Vert",
            stat_speed: "Tempo",
            help: " ↑/↓ Navigieren · Enter Wählen · Tab Fokus · / Suche · L Sprache · Q Beenden ",
        }
    }
}
