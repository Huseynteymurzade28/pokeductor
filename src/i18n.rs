//! Localization layer.
//!
//! Every user-facing string flows through [`Language::strings`]. Because the UI
//! re-reads these on every frame, switching language (the `L` hotkey) updates
//! the entire interface instantly with no extra bookkeeping.

use crate::models::{title_case, EvolutionCondition, EvolutionTrigger, StatKind};

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

    /// The inverse of [`flavor_code`](Self::flavor_code), for reading a
    /// language back out of a stored session. An unknown code — a file written
    /// by a build that shipped a language this one does not — is `None`, and
    /// the caller keeps its default rather than failing.
    pub fn from_code(code: &str) -> Option<Language> {
        Language::ALL
            .into_iter()
            .find(|language| language.flavor_code() == code)
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
    /// The one-line status bar. Deliberately short: the full key map lives in
    /// the help overlay, so this only carries what a first-time reader needs
    /// to find it.
    pub help: &'static str,
    /// Placeholder shown in an empty, unfocused search box. Doubles as the
    /// only place the `dex:` / `type:` / `gen:` syntax is advertised, so it
    /// names them literally — those keywords are not translated.
    pub search_hint: &'static str,
    /// Sidebar sort-order badges.
    pub sort_dex: &'static str,
    pub sort_name: &'static str,
    /// Shown in the list while a `type:` filter waits on its roster.
    pub loading_filter: &'static str,
    /// Title of the team card.
    pub team_title: &'static str,
    /// Shown on the team card while the party is empty.
    pub team_empty: &'static str,
    /// Heading above the attacking types that hit several members hard.
    pub team_shared_weak: &'static str,
    /// Heading above the attacking types nobody on the team resists.
    pub team_unresisted: &'static str,
    /// Heading above the defending types nobody on the team hits hard.
    pub team_offense_gaps: &'static str,
    /// Reassurance shown in place of an empty weakness/gap section.
    pub team_all_clear: &'static str,
    /// Heading above the immunities a Pokemon's abilities grant — on the party
    /// card and on the single-species matchup card alike, since both report
    /// them and neither should word it differently.
    pub immune_by_ability: &'static str,
    /// Marker on an immunity the species might not actually have, because the
    /// ability granting it is only one of several it could carry.
    pub immunity_maybe: &'static str,
    /// Hint at the foot of the team card.
    pub team_close_hint: &'static str,
    /// Label for the ability row on the info card.
    pub abilities_label: &'static str,
    /// Title of the ability card.
    pub abilities_title: &'static str,
    /// Marker on a species' hidden ability.
    pub ability_hidden: &'static str,
    /// Hint at the foot of the ability card.
    pub ability_close_hint: &'static str,
    /// Title of the moves card.
    pub moves_title: &'static str,
    /// Shown in place of the list when a species has no recorded learnset.
    pub moves_empty: &'static str,
    /// Hint at the foot of the moves card.
    pub moves_close_hint: &'static str,
    /// Column headings on the moves card. Kept short: seven columns have to
    /// fit a card narrow enough to leave the list behind it visible.
    pub col_learn: &'static str,
    pub col_move: &'static str,
    pub col_type: &'static str,
    pub col_category: &'static str,
    pub col_power: &'static str,
    pub col_accuracy: &'static str,
    pub col_pp: &'static str,
    /// What stands in the level column for a move no level is learned at.
    pub learn_machine: &'static str,
    pub learn_egg: &'static str,
    pub learn_tutor: &'static str,
    /// A move's damage category.
    pub class_physical: &'static str,
    pub class_special: &'static str,
    pub class_status: &'static str,
    /// Hint shown in the evolution panel when it is not focused.
    pub expand_hint: &'static str,
    /// Hint shown in the evolution panel while it is focused.
    pub evo_nav_hint: &'static str,
    /// Hint at the foot of the full-screen evolution view.
    pub evo_card_hint: &'static str,
    /// Placeholder under a chain member whose sprite is still loading.
    pub sprite_loading: &'static str,
    /// Title of the language-picker card.
    pub language_title: &'static str,
    /// Title of the type-matchup card.
    pub matchups_title: &'static str,
    /// Heading above the "damage this Pokemon takes" groups.
    pub matchups_defense: &'static str,
    /// Heading above the types this Pokemon hits super-effectively.
    pub matchups_offense: &'static str,
    /// Shown in place of an empty matchup group.
    pub matchups_none: &'static str,
    /// Hint at the foot of the matchup card.
    pub close_hint: &'static str,
    /// Title of the head-to-head comparison card.
    pub compare_title: &'static str,
    /// Heading above the two best same-type hits on the comparison card.
    pub compare_best_hit: &'static str,
    /// Shown in a comparison row the two species are level on.
    pub compare_tie: &'static str,
    /// Hint at the foot of the comparison card.
    pub compare_hint: &'static str,
    /// Wording for the help overlay.
    pub help_card: HelpStrings,
    /// Wording for evolution requirements.
    pub evo: EvoStrings,
    /// Badge labels for special species categories.
    pub legendary_label: &'static str,
    pub mythical_label: &'static str,
    pub baby_label: &'static str,
    /// Badge marking the info card while shiny artwork is on display.
    pub shiny_label: &'static str,
}

/// Labels for the help overlay.
///
/// Kept in its own struct because it is a table rather than a handful of
/// captions: the key names are language-neutral and live in `ui.rs`, while
/// every action label is translated here.
#[derive(Debug, Clone, Copy)]
pub struct HelpStrings {
    pub title: &'static str,
    pub ctx_list: &'static str,
    pub ctx_search: &'static str,
    pub ctx_evolution: &'static str,
    pub ctx_cards: &'static str,
    pub act_move: &'static str,
    pub act_jump10: &'static str,
    pub act_load: &'static str,
    pub act_search: &'static str,
    pub act_evolutions: &'static str,
    pub act_types: &'static str,
    pub act_abilities: &'static str,
    pub act_moves: &'static str,
    pub act_shiny: &'static str,
    pub act_party_toggle: &'static str,
    pub act_party_card: &'static str,
    pub act_sort: &'static str,
    pub act_language: &'static str,
    pub act_help: &'static str,
    pub act_quit: &'static str,
    pub act_load_back: &'static str,
    pub act_back: &'static str,
    pub act_by_type: &'static str,
    pub act_by_ability: &'static str,
    pub act_by_egg: &'static str,
    pub act_by_generation: &'static str,
    pub act_chain_move: &'static str,
    pub act_chain_jump: &'static str,
    pub act_chain_expand: &'static str,
    pub act_compare: &'static str,
    pub act_close: &'static str,
    pub close_hint: &'static str,
}

/// Wording for the requirements attached to an evolution.
///
/// Entries containing `{}` are templates: the placeholder is replaced with a
/// value (a level, an item, a species), which lets each language put it where
/// its grammar wants it — "Use Water Stone" vs. "Water Stone kullan".
///
/// Item, move and location names themselves stay in English: they arrive as
/// PokeAPI slugs and localizing them would mean an extra request per name.
#[derive(Debug, Clone, Copy)]
pub struct EvoStrings {
    pub level: &'static str,
    pub level_up: &'static str,
    pub trade: &'static str,
    pub trade_with: &'static str,
    pub use_item: &'static str,
    pub held_item: &'static str,
    pub knows_move: &'static str,
    pub knows_move_type: &'static str,
    pub happiness: &'static str,
    pub affection: &'static str,
    pub beauty: &'static str,
    pub day: &'static str,
    pub night: &'static str,
    pub dusk: &'static str,
    pub location: &'static str,
    pub male: &'static str,
    pub female: &'static str,
    pub rain: &'static str,
    pub upside_down: &'static str,
    pub party_species: &'static str,
    pub party_type: &'static str,
    pub shed: &'static str,
}

impl EvoStrings {
    /// Every requirement in `condition`, phrased for this language and ordered
    /// most-identifying first — so callers with little room can simply take the
    /// first entry and still show the part that matters.
    pub fn parts(&self, condition: &EvolutionCondition) -> Vec<String> {
        let fill = |template: &str, value: &str| template.replace("{}", value);
        let mut parts = Vec::new();

        if let Some(level) = condition.min_level {
            parts.push(fill(self.level, &level.to_string()));
        }
        if let Some(item) = &condition.item {
            parts.push(fill(self.use_item, &title_case(item)));
        }
        match (&condition.trigger, &condition.trade_species) {
            (Some(EvolutionTrigger::Trade), Some(species)) => {
                parts.push(fill(self.trade_with, &title_case(species)));
            }
            (Some(EvolutionTrigger::Trade), None) => parts.push(self.trade.to_string()),
            (Some(EvolutionTrigger::Shed), _) => parts.push(self.shed.to_string()),
            _ => {}
        }
        if let Some(item) = &condition.held_item {
            parts.push(fill(self.held_item, &title_case(item)));
        }
        if let Some(move_) = &condition.known_move {
            parts.push(fill(self.knows_move, &title_case(move_)));
        }
        if let Some(type_) = &condition.known_move_type {
            parts.push(fill(self.knows_move_type, &title_case(type_)));
        }
        if let Some(value) = condition.min_happiness {
            parts.push(fill(self.happiness, &value.to_string()));
        }
        if let Some(value) = condition.min_affection {
            parts.push(fill(self.affection, &value.to_string()));
        }
        if let Some(value) = condition.min_beauty {
            parts.push(fill(self.beauty, &value.to_string()));
        }
        if let Some(time) = condition.time_of_day.as_deref() {
            match time {
                "day" => parts.push(self.day.to_string()),
                "night" => parts.push(self.night.to_string()),
                "dusk" => parts.push(self.dusk.to_string()),
                other => parts.push(title_case(other)),
            }
        }
        if let Some(place) = &condition.location {
            parts.push(fill(self.location, &title_case(place)));
        }
        match condition.gender {
            Some(1) => parts.push(self.female.to_string()),
            Some(2) => parts.push(self.male.to_string()),
            _ => {}
        }
        if let Some(species) = &condition.party_species {
            parts.push(fill(self.party_species, &title_case(species)));
        }
        if let Some(type_) = &condition.party_type {
            parts.push(fill(self.party_type, &title_case(type_)));
        }
        if condition.needs_overworld_rain {
            parts.push(self.rain.to_string());
        }
        if condition.turn_upside_down {
            parts.push(self.upside_down.to_string());
        }
        // Tyrogue's three branches. Symbolic, so it reads the same everywhere.
        if let Some(cmp) = condition.relative_physical_stats {
            parts.push(match cmp {
                1 => "Atk > Def".to_string(),
                -1 => "Atk < Def".to_string(),
                _ => "Atk = Def".to_string(),
            });
        }

        // Nothing but a trigger: name the trigger rather than showing nothing.
        if parts.is_empty() {
            match &condition.trigger {
                Some(EvolutionTrigger::LevelUp) => parts.push(self.level_up.to_string()),
                Some(EvolutionTrigger::Other(slug)) => parts.push(title_case(slug)),
                _ => {}
            }
        }
        parts
    }

    /// Every requirement on one line, for the hint bar.
    pub fn summary(&self, condition: &EvolutionCondition) -> String {
        self.parts(condition).join(" · ")
    }

    /// Just the headline requirement, for the single row under a sprite card.
    pub fn short(&self, condition: &EvolutionCondition) -> Option<String> {
        self.parts(condition).into_iter().next()
    }
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
            help: " ↑/↓ Navigate · Enter Select · / Search · ? Help · Q Quit ",
            search_hint: "name · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Party ",
            team_empty: "Press Space in the list to add a Pokemon",
            team_shared_weak: "Shared weaknesses",
            team_unresisted: "Resisted by nobody",
            team_offense_gaps: "Hit hard by nobody",
            team_all_clear: "nothing — all covered",
            abilities_label: "Abilities",
            abilities_title: " Abilities ",
            ability_hidden: "hidden",
            ability_close_hint: "Esc / A to close",
            moves_title: " Moves ",
            moves_empty: "No learnset recorded",
            moves_close_hint: "↑ ↓ to browse · M / Esc to close",
            col_learn: "Lv",
            col_move: "Move",
            col_type: "Type",
            col_category: "Cat.",
            col_power: "Pow",
            col_accuracy: "Acc",
            col_pp: "PP",
            learn_machine: "TM",
            learn_egg: "Egg",
            learn_tutor: "Tutor",
            class_physical: "Phys",
            class_special: "Spec",
            class_status: "Stat",
            immune_by_ability: "Immune by ability",
            immunity_maybe: "possible",
            team_close_hint: "Esc / P to close",
            loading_filter: "Fetching the filter's list",
            expand_hint: "Press E to browse evolutions",
            evo_nav_hint: "←/→ Select · Enter Jump · F Full screen · Esc Back",
            evo_card_hint: "←/→ Select · Enter Jump · F / Esc Close",
            sprite_loading: "loading…",
            language_title: " Language ",
            matchups_title: " Type Matchups ",
            matchups_defense: "Damage taken",
            matchups_offense: "Super effective against",
            matchups_none: "nothing",
            close_hint: "Esc / T to close",
            compare_title: " Head to Head ",
            compare_best_hit: "Best same-type hit",
            compare_tie: "level",
            compare_hint: "Esc / C to close",
            help_card: HelpStrings {
                title: " Help ",
                ctx_list: "List",
                ctx_search: "Search box",
                ctx_evolution: "Evolution panel",
                ctx_cards: "Any card",
                act_move: "Move selection",
                act_jump10: "Jump ten",
                act_load: "Load",
                act_search: "Search",
                act_evolutions: "Evolutions",
                act_types: "Type matchups",
                act_abilities: "Abilities",
                act_moves: "Moves",
                act_shiny: "Shiny artwork",
                act_party_toggle: "Add / remove from party",
                act_party_card: "Party",
                act_sort: "Sort order",
                act_language: "Language",
                act_help: "This help",
                act_quit: "Quit",
                act_load_back: "Load and return",
                act_back: "Back to list",
                act_by_type: "Filter by type",
                act_by_ability: "Filter by ability",
                act_by_egg: "Filter by egg group",
                act_by_generation: "Filter by generation",
                act_chain_move: "Move between stages",
                act_chain_jump: "Jump to stage",
                act_chain_expand: "Full-screen chain",
                act_compare: "Pin / compare two species",
                act_close: "Close",
                close_hint: "? / Esc to close",
            },
            evo: EvoStrings {
                level: "Lv. {}",
                level_up: "Level up",
                trade: "Trade",
                trade_with: "Trade for {}",
                use_item: "Use {}",
                held_item: "Holding {}",
                knows_move: "Knows {}",
                knows_move_type: "Knows a {} move",
                happiness: "Happiness {}",
                affection: "Affection {}",
                beauty: "Beauty {}",
                day: "Daytime",
                night: "At night",
                dusk: "At dusk",
                location: "At {}",
                male: "Male",
                female: "Female",
                rain: "In rain",
                upside_down: "Console upside down",
                party_species: "With {} in party",
                party_type: "With a {} type in party",
                shed: "Empty party slot",
            },
            legendary_label: "Legendary",
            mythical_label: "Mythical",
            baby_label: "Baby",
            shiny_label: "Shiny",
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
            help: " ↑/↓ Gezin · Enter Seç · / Ara · ? Yardım · Q Çıkış ",
            search_hint: "isim · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Takım ",
            team_empty: "Listede Boşluk tuşuyla Pokemon ekleyin",
            team_shared_weak: "Ortak zayıflıklar",
            team_unresisted: "Kimsenin direnmediği",
            team_offense_gaps: "Kimsenin vuramadığı",
            team_all_clear: "yok — hepsi kapalı",
            abilities_label: "Yetenekler",
            abilities_title: " Yetenekler ",
            ability_hidden: "gizli",
            ability_close_hint: "Kapatmak için Esc / A",
            moves_title: " Hareketler ",
            moves_empty: "Kayıtlı hareket listesi yok",
            moves_close_hint: "↑ ↓ gezin · M / Esc kapat",
            col_learn: "Sv",
            col_move: "Hareket",
            col_type: "Tip",
            col_category: "Tür",
            col_power: "Güç",
            col_accuracy: "İsb",
            col_pp: "PP",
            learn_machine: "TM",
            learn_egg: "Yumurta",
            learn_tutor: "Öğretmen",
            class_physical: "Fiz",
            class_special: "Özel",
            class_status: "Durum",
            immune_by_ability: "Yetenekle bağışık",
            immunity_maybe: "olası",
            team_close_hint: "Kapatmak için Esc / P",
            loading_filter: "Filtre listesi getiriliyor",
            expand_hint: "Evrimlere göz atmak için E'ye basın",
            evo_nav_hint: "←/→ Seç · Enter Git · F Tam ekran · Esc Geri",
            evo_card_hint: "←/→ Seç · Enter Git · F / Esc Kapat",
            sprite_loading: "yükleniyor…",
            language_title: " Dil ",
            matchups_title: " Tip Etkinliği ",
            matchups_defense: "Alınan hasar",
            matchups_offense: "Karşı üstün olduğu tipler",
            matchups_none: "yok",
            close_hint: "Kapatmak için Esc / T",
            compare_title: " Karşılaştırma ",
            compare_best_hit: "En sert aynı tipten vuruş",
            compare_tie: "eşit",
            compare_hint: "Kapatmak için Esc / C",
            help_card: HelpStrings {
                title: " Yardım ",
                ctx_list: "Liste",
                ctx_search: "Arama kutusu",
                ctx_evolution: "Evrim paneli",
                ctx_cards: "Tüm kartlar",
                act_move: "Seçimi taşı",
                act_jump10: "On atla",
                act_load: "Yükle",
                act_search: "Ara",
                act_evolutions: "Evrimler",
                act_types: "Tip eşleşmeleri",
                act_abilities: "Yetenekler",
                act_moves: "Hareketler",
                act_shiny: "Parlak görsel",
                act_party_toggle: "Takıma ekle / çıkar",
                act_party_card: "Takım",
                act_sort: "Sıralama",
                act_language: "Dil",
                act_help: "Bu yardım",
                act_quit: "Çıkış",
                act_load_back: "Yükle ve dön",
                act_back: "Listeye dön",
                act_by_type: "Tipe göre süz",
                act_by_ability: "Yeteneğe göre süz",
                act_by_egg: "Yumurta grubuna göre süz",
                act_by_generation: "Jenerasyona göre süz",
                act_chain_move: "Aşamalar arasında gez",
                act_chain_jump: "Aşamaya atla",
                act_chain_expand: "Zinciri tam ekran aç",
                act_compare: "Sabitle / iki türü karşılaştır",
                act_close: "Kapat",
                close_hint: "Kapatmak için ? / Esc",
            },
            evo: EvoStrings {
                level: "Sv. {}",
                level_up: "Seviye atlayınca",
                trade: "Takas",
                trade_with: "{} ile takas",
                use_item: "{} kullan",
                held_item: "{} taşırken",
                knows_move: "{} bilir",
                knows_move_type: "{} tipi hamle bilir",
                happiness: "Mutluluk {}",
                affection: "Sevgi {}",
                beauty: "Güzellik {}",
                day: "Gündüz",
                night: "Gece",
                dusk: "Alacakaranlık",
                location: "{} bölgesinde",
                male: "Erkek",
                female: "Dişi",
                rain: "Yağmurda",
                upside_down: "Konsol ters çevrili",
                party_species: "Takımda {} varken",
                party_type: "Takımda {} tipi varken",
                shed: "Takımda boş yer",
            },
            legendary_label: "Efsanevi",
            mythical_label: "Mitik",
            baby_label: "Yavru",
            shiny_label: "Parlak",
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
            help: " ↑/↓ Navigieren · Enter Wählen · / Suche · ? Hilfe · Q Beenden ",
            search_hint: "Name · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Team ",
            team_empty: "Leertaste in der Liste fügt ein Pokemon hinzu",
            team_shared_weak: "Gemeinsame Schwächen",
            team_unresisted: "Von niemandem resistiert",
            team_offense_gaps: "Von niemandem hart getroffen",
            team_all_clear: "nichts — alles abgedeckt",
            abilities_label: "Fähigkeiten",
            abilities_title: " Fähigkeiten ",
            ability_hidden: "versteckt",
            ability_close_hint: "Esc / A zum Schließen",
            moves_title: " Attacken ",
            moves_empty: "Keine Attacken verzeichnet",
            moves_close_hint: "↑ ↓ blättern · M / Esc schließt",
            col_learn: "Lv",
            col_move: "Attacke",
            col_type: "Typ",
            col_category: "Kat.",
            col_power: "Str",
            col_accuracy: "Gen",
            col_pp: "AP",
            learn_machine: "TM",
            learn_egg: "Ei",
            learn_tutor: "Lehrer",
            class_physical: "Phys",
            class_special: "Spez",
            class_status: "Stat",
            immune_by_ability: "Immun durch Fähigkeit",
            immunity_maybe: "möglich",
            team_close_hint: "Esc / P zum Schließen",
            loading_filter: "Filterliste wird geladen",
            expand_hint: "Drücke E für die Entwicklungsreihe",
            evo_nav_hint: "←/→ Wählen · Enter Springen · F Vollbild · Esc Zurück",
            evo_card_hint: "←/→ Wählen · Enter Springen · F / Esc Schließen",
            sprite_loading: "lädt…",
            language_title: " Sprache ",
            matchups_title: " Typ-Effektivität ",
            matchups_defense: "Erlittener Schaden",
            matchups_offense: "Sehr effektiv gegen",
            matchups_none: "nichts",
            close_hint: "Esc / T zum Schließen",
            compare_title: " Direktvergleich ",
            compare_best_hit: "Bester Treffer eigenen Typs",
            compare_tie: "gleich",
            compare_hint: "Esc / C zum Schließen",
            help_card: HelpStrings {
                title: " Hilfe ",
                ctx_list: "Liste",
                ctx_search: "Suchfeld",
                ctx_evolution: "Entwicklungsfeld",
                ctx_cards: "Alle Karten",
                act_move: "Auswahl bewegen",
                act_jump10: "Zehn springen",
                act_load: "Laden",
                act_search: "Suche",
                act_evolutions: "Entwicklungen",
                act_types: "Typ-Matchups",
                act_abilities: "Fähigkeiten",
                act_moves: "Attacken",
                act_shiny: "Schillernde Grafik",
                act_party_toggle: "Team hinzu / entfernen",
                act_party_card: "Team",
                act_sort: "Sortierung",
                act_language: "Sprache",
                act_help: "Diese Hilfe",
                act_quit: "Beenden",
                act_load_back: "Laden und zurück",
                act_back: "Zurück zur Liste",
                act_by_type: "Nach Typ filtern",
                act_by_ability: "Nach Fähigkeit filtern",
                act_by_egg: "Nach Ei-Gruppe filtern",
                act_by_generation: "Nach Generation filtern",
                act_chain_move: "Zwischen Stufen",
                act_chain_jump: "Zur Stufe springen",
                act_chain_expand: "Reihe im Vollbild",
                act_compare: "Anheften / zwei vergleichen",
                act_close: "Schließen",
                close_hint: "? / Esc zum Schließen",
            },
            evo: EvoStrings {
                level: "Lv. {}",
                level_up: "Levelaufstieg",
                trade: "Tausch",
                trade_with: "Tausch gegen {}",
                use_item: "{} benutzen",
                held_item: "{} tragend",
                knows_move: "Kennt {}",
                knows_move_type: "Kennt {}-Attacke",
                happiness: "Freundschaft {}",
                affection: "Zuneigung {}",
                beauty: "Schönheit {}",
                day: "Tagsüber",
                night: "Nachts",
                dusk: "In der Dämmerung",
                location: "Bei {}",
                male: "Männlich",
                female: "Weiblich",
                rain: "Bei Regen",
                upside_down: "Konsole umgedreht",
                party_species: "Mit {} im Team",
                party_type: "Mit {}-Typ im Team",
                shed: "Freier Teamplatz",
            },
            legendary_label: "Legendär",
            mythical_label: "Mysteriös",
            baby_label: "Baby",
            shiny_label: "Schillernd",
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
            help: " ↑/↓ Naviguer · Entrée Choisir · / Recherche · ? Aide · Q Quitter ",
            search_hint: "nom · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Équipe ",
            team_empty: "Espace dans la liste pour ajouter un Pokemon",
            team_shared_weak: "Faiblesses communes",
            team_unresisted: "Résisté par personne",
            team_offense_gaps: "Frappé fort par personne",
            team_all_clear: "rien — tout est couvert",
            abilities_label: "Talents",
            abilities_title: " Talents ",
            ability_hidden: "caché",
            ability_close_hint: "Esc / A pour fermer",
            moves_title: " Capacités ",
            moves_empty: "Aucune capacité répertoriée",
            moves_close_hint: "↑ ↓ parcourir · M / Échap ferme",
            col_learn: "Niv",
            col_move: "Capacité",
            col_type: "Type",
            col_category: "Cat.",
            col_power: "Puis",
            col_accuracy: "Préc",
            col_pp: "PP",
            learn_machine: "CT",
            learn_egg: "Œuf",
            learn_tutor: "Tuteur",
            class_physical: "Phys",
            class_special: "Spé",
            class_status: "Statut",
            immune_by_ability: "Immunisé par talent",
            immunity_maybe: "possible",
            team_close_hint: "Esc / P pour fermer",
            loading_filter: "Chargement du filtre",
            expand_hint: "Appuie sur E pour les évolutions",
            evo_nav_hint: "←/→ Choisir · Entrée Aller · F Plein écran · Esc Retour",
            evo_card_hint: "←/→ Choisir · Entrée Aller · F / Esc Fermer",
            sprite_loading: "chargement…",
            language_title: " Langue ",
            matchups_title: " Efficacité des Types ",
            matchups_defense: "Dégâts subis",
            matchups_offense: "Super efficace contre",
            matchups_none: "rien",
            close_hint: "Esc / T pour fermer",
            compare_title: " Face à Face ",
            compare_best_hit: "Meilleure attaque de même type",
            compare_tie: "égalité",
            compare_hint: "Esc / C pour fermer",
            help_card: HelpStrings {
                title: " Aide ",
                ctx_list: "Liste",
                ctx_search: "Recherche",
                ctx_evolution: "Panneau d'évolution",
                ctx_cards: "Toute carte",
                act_move: "Déplacer la sélection",
                act_jump10: "Sauter dix",
                act_load: "Charger",
                act_search: "Rechercher",
                act_evolutions: "Évolutions",
                act_types: "Affinités de type",
                act_abilities: "Talents",
                act_moves: "Capacités",
                act_shiny: "Illustration chromatique",
                act_party_toggle: "Ajouter / retirer de l'équipe",
                act_party_card: "Équipe",
                act_sort: "Tri",
                act_language: "Langue",
                act_help: "Cette aide",
                act_quit: "Quitter",
                act_load_back: "Charger et revenir",
                act_back: "Retour à la liste",
                act_by_type: "Filtrer par type",
                act_by_ability: "Filtrer par talent",
                act_by_egg: "Filtrer par groupe d'œufs",
                act_by_generation: "Filtrer par génération",
                act_chain_move: "Entre les stades",
                act_chain_jump: "Aller au stade",
                act_chain_expand: "Chaîne en plein écran",
                act_compare: "Épingler / comparer deux espèces",
                act_close: "Fermer",
                close_hint: "? / Esc pour fermer",
            },
            evo: EvoStrings {
                level: "Niv. {}",
                level_up: "Montée de niveau",
                trade: "Échange",
                trade_with: "Échange contre {}",
                use_item: "Utiliser {}",
                held_item: "Tient {}",
                knows_move: "Connaît {}",
                knows_move_type: "Connaît une capacité {}",
                happiness: "Bonheur {}",
                affection: "Affection {}",
                beauty: "Beauté {}",
                day: "Le jour",
                night: "La nuit",
                dusk: "Au crépuscule",
                location: "À {}",
                male: "Mâle",
                female: "Femelle",
                rain: "Sous la pluie",
                upside_down: "Console retournée",
                party_species: "Avec {} dans l'équipe",
                party_type: "Avec un type {} dans l'équipe",
                shed: "Place libre dans l'équipe",
            },
            legendary_label: "Légendaire",
            mythical_label: "Fabuleux",
            baby_label: "Bébé",
            shiny_label: "Chromatique",
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
            help: " ↑/↓ Navegar · Enter Elegir · / Buscar · ? Ayuda · Q Salir ",
            search_hint: "nombre · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Equipo ",
            team_empty: "Espacio en la lista para añadir un Pokemon",
            team_shared_weak: "Debilidades compartidas",
            team_unresisted: "Nadie lo resiste",
            team_offense_gaps: "Nadie lo golpea fuerte",
            team_all_clear: "nada — todo cubierto",
            abilities_label: "Habilidades",
            abilities_title: " Habilidades ",
            ability_hidden: "oculta",
            ability_close_hint: "Esc / A para cerrar",
            moves_title: " Movimientos ",
            moves_empty: "Sin movimientos registrados",
            moves_close_hint: "↑ ↓ para navegar · M / Esc cierra",
            col_learn: "Niv",
            col_move: "Movimiento",
            col_type: "Tipo",
            col_category: "Cat.",
            col_power: "Pot",
            col_accuracy: "Prec",
            col_pp: "PP",
            learn_machine: "MT",
            learn_egg: "Huevo",
            learn_tutor: "Tutor",
            class_physical: "Fís",
            class_special: "Esp",
            class_status: "Estado",
            immune_by_ability: "Inmune por habilidad",
            immunity_maybe: "posible",
            team_close_hint: "Esc / P para cerrar",
            loading_filter: "Cargando la lista del filtro",
            expand_hint: "Pulsa E para ver las evoluciones",
            evo_nav_hint: "←/→ Elegir · Enter Ir · F Pantalla completa · Esc Volver",
            evo_card_hint: "←/→ Elegir · Enter Ir · F / Esc Cerrar",
            sprite_loading: "cargando…",
            language_title: " Idioma ",
            matchups_title: " Eficacia de Tipos ",
            matchups_defense: "Daño recibido",
            matchups_offense: "Muy eficaz contra",
            matchups_none: "nada",
            close_hint: "Esc / T para cerrar",
            compare_title: " Cara a Cara ",
            compare_best_hit: "Mejor golpe del mismo tipo",
            compare_tie: "empate",
            compare_hint: "Esc / C para cerrar",
            help_card: HelpStrings {
                title: " Ayuda ",
                ctx_list: "Lista",
                ctx_search: "Búsqueda",
                ctx_evolution: "Panel de evolución",
                ctx_cards: "Cualquier ficha",
                act_move: "Mover selección",
                act_jump10: "Saltar diez",
                act_load: "Cargar",
                act_search: "Buscar",
                act_evolutions: "Evoluciones",
                act_types: "Efectividad de tipos",
                act_abilities: "Habilidades",
                act_moves: "Movimientos",
                act_shiny: "Ilustración variocolor",
                act_party_toggle: "Añadir / quitar del equipo",
                act_party_card: "Equipo",
                act_sort: "Orden",
                act_language: "Idioma",
                act_help: "Esta ayuda",
                act_quit: "Salir",
                act_load_back: "Cargar y volver",
                act_back: "Volver a la lista",
                act_by_type: "Filtrar por tipo",
                act_by_ability: "Filtrar por habilidad",
                act_by_egg: "Filtrar por grupo huevo",
                act_by_generation: "Filtrar por generación",
                act_chain_move: "Entre etapas",
                act_chain_jump: "Ir a la etapa",
                act_chain_expand: "Cadena a pantalla completa",
                act_compare: "Fijar / comparar dos especies",
                act_close: "Cerrar",
                close_hint: "? / Esc para cerrar",
            },
            evo: EvoStrings {
                level: "Niv. {}",
                level_up: "Subir de nivel",
                trade: "Intercambio",
                trade_with: "Intercambiar por {}",
                use_item: "Usar {}",
                held_item: "Llevando {}",
                knows_move: "Conoce {}",
                knows_move_type: "Conoce un movimiento {}",
                happiness: "Felicidad {}",
                affection: "Afecto {}",
                beauty: "Belleza {}",
                day: "De día",
                night: "De noche",
                dusk: "Al anochecer",
                location: "En {}",
                male: "Macho",
                female: "Hembra",
                rain: "Bajo la lluvia",
                upside_down: "Consola boca abajo",
                party_species: "Con {} en el equipo",
                party_type: "Con un tipo {} en el equipo",
                shed: "Hueco libre en el equipo",
            },
            legendary_label: "Legendario",
            mythical_label: "Singular",
            baby_label: "Bebé",
            shiny_label: "Variocolor",
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
            help: " ↑/↓ Naviga · Invio Scegli · / Cerca · ? Aiuto · Q Esci ",
            search_hint: "nome · dex:25 · type:water · gen:1",
            sort_dex: "Dex",
            sort_name: "A–Z",
            team_title: " Squadra ",
            team_empty: "Spazio nella lista per aggiungere un Pokemon",
            team_shared_weak: "Debolezze condivise",
            team_unresisted: "Nessuno lo resiste",
            team_offense_gaps: "Nessuno lo colpisce forte",
            team_all_clear: "niente — tutto coperto",
            abilities_label: "Abilità",
            abilities_title: " Abilità ",
            ability_hidden: "nascosta",
            ability_close_hint: "Esc / A per chiudere",
            moves_title: " Mosse ",
            moves_empty: "Nessuna mossa registrata",
            moves_close_hint: "↑ ↓ per scorrere · M / Esc chiude",
            col_learn: "Liv",
            col_move: "Mossa",
            col_type: "Tipo",
            col_category: "Cat.",
            col_power: "Pot",
            col_accuracy: "Prec",
            col_pp: "PP",
            learn_machine: "MT",
            learn_egg: "Uovo",
            learn_tutor: "Tutor",
            class_physical: "Fis",
            class_special: "Spec",
            class_status: "Stato",
            immune_by_ability: "Immune per abilità",
            immunity_maybe: "possibile",
            team_close_hint: "Esc / P per chiudere",
            loading_filter: "Caricamento del filtro",
            expand_hint: "Premi E per le evoluzioni",
            evo_nav_hint: "←/→ Scegli · Invio Vai · F Schermo intero · Esc Indietro",
            evo_card_hint: "←/→ Scegli · Invio Vai · F / Esc Chiudi",
            sprite_loading: "caricamento…",
            language_title: " Lingua ",
            matchups_title: " Efficacia dei Tipi ",
            matchups_defense: "Danni subiti",
            matchups_offense: "Superefficace contro",
            matchups_none: "niente",
            close_hint: "Esc / T per chiudere",
            compare_title: " Testa a Testa ",
            compare_best_hit: "Miglior colpo dello stesso tipo",
            compare_tie: "pari",
            compare_hint: "Esc / C per chiudere",
            help_card: HelpStrings {
                title: " Aiuto ",
                ctx_list: "Lista",
                ctx_search: "Ricerca",
                ctx_evolution: "Pannello evoluzioni",
                ctx_cards: "Ogni scheda",
                act_move: "Sposta selezione",
                act_jump10: "Salta dieci",
                act_load: "Carica",
                act_search: "Cerca",
                act_evolutions: "Evoluzioni",
                act_types: "Efficacia dei tipi",
                act_abilities: "Abilità",
                act_moves: "Mosse",
                act_shiny: "Illustrazione cromatica",
                act_party_toggle: "Aggiungi / togli dalla squadra",
                act_party_card: "Squadra",
                act_sort: "Ordinamento",
                act_language: "Lingua",
                act_help: "Questo aiuto",
                act_quit: "Esci",
                act_load_back: "Carica e torna",
                act_back: "Torna alla lista",
                act_by_type: "Filtra per tipo",
                act_by_ability: "Filtra per abilità",
                act_by_egg: "Filtra per gruppo uova",
                act_by_generation: "Filtra per generazione",
                act_chain_move: "Tra gli stadi",
                act_chain_jump: "Vai allo stadio",
                act_chain_expand: "Catena a schermo intero",
                act_compare: "Fissa / confronta due specie",
                act_close: "Chiudi",
                close_hint: "? / Esc per chiudere",
            },
            evo: EvoStrings {
                level: "Liv. {}",
                level_up: "Aumento di livello",
                trade: "Scambio",
                trade_with: "Scambio con {}",
                use_item: "Usa {}",
                held_item: "Tenendo {}",
                knows_move: "Conosce {}",
                knows_move_type: "Conosce una mossa {}",
                happiness: "Felicità {}",
                affection: "Affetto {}",
                beauty: "Bellezza {}",
                day: "Di giorno",
                night: "Di notte",
                dusk: "Al tramonto",
                location: "A {}",
                male: "Maschio",
                female: "Femmina",
                rain: "Sotto la pioggia",
                upside_down: "Console capovolta",
                party_species: "Con {} in squadra",
                party_type: "Con un tipo {} in squadra",
                shed: "Posto libero in squadra",
            },
            legendary_label: "Leggendario",
            mythical_label: "Misterioso",
            baby_label: "Cucciolo",
            shiny_label: "Cromatico",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_reads_back_out_of_its_code() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.flavor_code()), Some(language));
        }
    }

    #[test]
    fn an_unknown_language_code_is_not_guessed_at() {
        assert_eq!(Language::from_code("xx"), None);
        assert_eq!(Language::from_code(""), None);
    }

    fn umbreon() -> EvolutionCondition {
        EvolutionCondition {
            trigger: Some(EvolutionTrigger::LevelUp),
            min_happiness: Some(160),
            time_of_day: Some("night".into()),
            ..Default::default()
        }
    }

    #[test]
    fn every_language_fills_its_placeholders() {
        // A condition touching every templated string, so a translation that
        // dropped its `{}` shows up here rather than in the UI.
        let kitchen_sink = EvolutionCondition {
            trigger: Some(EvolutionTrigger::Trade),
            min_level: Some(16),
            item: Some("water-stone".into()),
            held_item: Some("kings-rock".into()),
            known_move: Some("ancient-power".into()),
            known_move_type: Some("fairy".into()),
            min_happiness: Some(160),
            min_affection: Some(2),
            min_beauty: Some(170),
            location: Some("mount-coronet".into()),
            trade_species: Some("shelmet".into()),
            party_species: Some("remoraid".into()),
            party_type: Some("rock".into()),
            ..Default::default()
        };
        for language in Language::ALL {
            for part in language.strings().evo.parts(&kitchen_sink) {
                assert!(
                    !part.contains("{}"),
                    "{:?} left a placeholder unfilled: {part}",
                    language
                );
            }
        }
    }

    #[test]
    fn parts_are_ordered_headline_first() {
        let english = Language::English.strings().evo;
        assert_eq!(english.short(&umbreon()).as_deref(), Some("Happiness 160"));
        assert_eq!(english.summary(&umbreon()), "Happiness 160 · At night");
    }

    #[test]
    fn a_bare_trigger_still_reads_as_something() {
        let condition = EvolutionCondition {
            trigger: Some(EvolutionTrigger::Other("three-critical-hits".into())),
            ..Default::default()
        };
        let english = Language::English.strings().evo;
        assert_eq!(english.summary(&condition), "Three Critical Hits");
    }

    #[test]
    fn an_unknown_condition_yields_no_text() {
        let english = Language::English.strings().evo;
        assert!(english.short(&EvolutionCondition::default()).is_none());
    }

    #[test]
    fn item_names_are_humanised() {
        let english = Language::English.strings().evo;
        let condition = EvolutionCondition {
            trigger: Some(EvolutionTrigger::UseItem),
            item: Some("water-stone".into()),
            ..Default::default()
        };
        assert_eq!(english.summary(&condition), "Use Water Stone");
    }
}
