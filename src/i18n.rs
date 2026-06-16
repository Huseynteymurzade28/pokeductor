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
    French,
    Spanish,
    Italian,
}

impl Language {
    /// Every supported language, in picker order.
    pub const ALL: [Language; 6] = [
        Language::English,
        Language::Turkish,
        Language::German,
        Language::French,
        Language::Spanish,
        Language::Italian,
    ];

    /// Position of this language within [`Language::ALL`].
    pub fn index(self) -> usize {
        Language::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    /// Endonym shown in the language picker, e.g. `"Türkçe"`.
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Turkish => "Türkçe",
            Language::German => "Deutsch",
            Language::French => "Français",
            Language::Spanish => "Español",
            Language::Italian => "Italiano",
        }
    }

    /// PokeAPI language code used to pick localized flavor/genus text. PokeAPI
    /// has no Turkish entries, so Turkish maps to `"tr"` and falls back to
    /// English at the point of use.
    pub fn flavor_code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Turkish => "tr",
            Language::German => "de",
            Language::French => "fr",
            Language::Spanish => "es",
            Language::Italian => "it",
        }
    }

    /// Short tag shown in the status bar, e.g. `EN`.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "EN",
            Language::Turkish => "TR",
            Language::German => "DE",
            Language::French => "FR",
            Language::Spanish => "ES",
            Language::Italian => "IT",
        }
    }

    /// The full translation table for this language.
    pub fn strings(self) -> Strings {
        match self {
            Language::English => Strings::english(),
            Language::Turkish => Strings::turkish(),
            Language::German => Strings::german(),
            Language::French => Strings::french(),
            Language::Spanish => Strings::spanish(),
            Language::Italian => Strings::italian(),
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
    /// Hint shown in the evolution panel when it is not focused.
    pub expand_hint: &'static str,
    /// Hint shown in the evolution panel while it is focused.
    pub evo_nav_hint: &'static str,
    /// Placeholder under a chain member whose sprite is still loading.
    pub sprite_loading: &'static str,
    /// Title of the language-picker card.
    pub language_title: &'static str,
    /// Badge labels for special species categories.
    pub legendary_label: &'static str,
    pub mythical_label: &'static str,
    pub baby_label: &'static str,
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
            help: " ↑/↓ Navigate · Enter Select · E Evolutions · / Search · L Language · Q Quit ",
            expand_hint: "Press E to browse evolutions",
            evo_nav_hint: "←/→ Select · Enter Jump · Esc Back",
            sprite_loading: "loading…",
            language_title: " Language ",
            legendary_label: "Legendary",
            mythical_label: "Mythical",
            baby_label: "Baby",
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
            help: " ↑/↓ Gezin · Enter Seç · E Evrimler · / Ara · L Dil · Q Çıkış ",
            expand_hint: "Evrimlere göz atmak için E'ye basın",
            evo_nav_hint: "←/→ Seç · Enter Git · Esc Geri",
            sprite_loading: "yükleniyor…",
            language_title: " Dil ",
            legendary_label: "Efsanevi",
            mythical_label: "Mitik",
            baby_label: "Yavru",
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
            help: " ↑/↓ Navigieren · Enter Wählen · E Entwicklung · / Suche · L Sprache · Q Beenden ",
            expand_hint: "Drücke E für die Entwicklungsreihe",
            evo_nav_hint: "←/→ Wählen · Enter Springen · Esc Zurück",
            sprite_loading: "lädt…",
            language_title: " Sprache ",
            legendary_label: "Legendär",
            mythical_label: "Mysteriös",
            baby_label: "Baby",
        }
    }

    fn french() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex & Analyseur d'Évolution ",
            sidebar_title: " Pokemon ",
            search_title: " Recherche ",
            details_title: " Détails ",
            evolution_title: " Chaîne d'Évolution ",
            loading: "Chargement",
            loading_list: "Chargement du Pokedex",
            no_selection: "Choisis un Pokemon et appuie sur Entrée",
            no_results: "Aucun Pokemon trouvé",
            no_evolution: "Pas de données d'évolution",
            types_label: "Types",
            height_label: "Taille",
            weight_label: "Poids",
            total_label: "Total",
            error_prefix: "Erreur",
            stat_hp: "PV",
            stat_attack: "Attaque",
            stat_defense: "Défense",
            stat_sp_attack: "Att. Sp",
            stat_sp_defense: "Déf. Sp",
            stat_speed: "Vitesse",
            help: " ↑/↓ Naviguer · Entrée Choisir · E Évolution · / Recherche · L Langue · Q Quitter ",
            expand_hint: "Appuie sur E pour les évolutions",
            evo_nav_hint: "←/→ Choisir · Entrée Aller · Esc Retour",
            sprite_loading: "chargement…",
            language_title: " Langue ",
            legendary_label: "Légendaire",
            mythical_label: "Fabuleux",
            baby_label: "Bébé",
        }
    }

    fn spanish() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex y Analizador de Evolución ",
            sidebar_title: " Pokemon ",
            search_title: " Buscar ",
            details_title: " Detalles ",
            evolution_title: " Cadena Evolutiva ",
            loading: "Cargando",
            loading_list: "Cargando Pokedex",
            no_selection: "Elige un Pokemon y pulsa Enter",
            no_results: "No se encontraron Pokemon",
            no_evolution: "Sin datos de evolución",
            types_label: "Tipos",
            height_label: "Altura",
            weight_label: "Peso",
            total_label: "Total",
            error_prefix: "Error",
            stat_hp: "PS",
            stat_attack: "Ataque",
            stat_defense: "Defensa",
            stat_sp_attack: "At. Esp",
            stat_sp_defense: "Def. Esp",
            stat_speed: "Velocid.",
            help: " ↑/↓ Navegar · Enter Elegir · E Evolución · / Buscar · L Idioma · Q Salir ",
            expand_hint: "Pulsa E para ver las evoluciones",
            evo_nav_hint: "←/→ Elegir · Enter Ir · Esc Volver",
            sprite_loading: "cargando…",
            language_title: " Idioma ",
            legendary_label: "Legendario",
            mythical_label: "Singular",
            baby_label: "Bebé",
        }
    }

    fn italian() -> Self {
        Strings {
            app_title: " Pokeductor — Pokedex e Analizzatore di Evoluzione ",
            sidebar_title: " Pokemon ",
            search_title: " Cerca ",
            details_title: " Dettagli ",
            evolution_title: " Catena Evolutiva ",
            loading: "Caricamento",
            loading_list: "Caricamento Pokedex",
            no_selection: "Scegli un Pokemon e premi Invio",
            no_results: "Nessun Pokemon trovato",
            no_evolution: "Nessun dato di evoluzione",
            types_label: "Tipi",
            height_label: "Altezza",
            weight_label: "Peso",
            total_label: "Totale",
            error_prefix: "Errore",
            stat_hp: "PS",
            stat_attack: "Attacco",
            stat_defense: "Difesa",
            stat_sp_attack: "Att. Sp",
            stat_sp_defense: "Dif. Sp",
            stat_speed: "Velocità",
            help: " ↑/↓ Naviga · Invio Scegli · E Evoluzione · / Cerca · L Lingua · Q Esci ",
            expand_hint: "Premi E per le evoluzioni",
            evo_nav_hint: "←/→ Scegli · Invio Vai · Esc Indietro",
            sprite_loading: "caricamento…",
            language_title: " Lingua ",
            legendary_label: "Leggendario",
            mythical_label: "Misterioso",
            baby_label: "Cucciolo",
        }
    }
}
