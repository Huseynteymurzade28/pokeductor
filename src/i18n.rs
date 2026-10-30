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

    /// A type's name in this language, for a PokeAPI type slug. A slug the
    /// table does not know (`stellar`, `shadow`) is title-cased as it comes.
    pub fn type_name(self, slug: &str) -> String {
        TYPE_NAMES
            .iter()
            .find(|(known, _)| *known == slug)
            .map(|(_, names)| names[self.index()].to_string())
            .unwrap_or_else(|| title_case(slug))
    }
}

/// Every type's name in each language, in [`Language::ALL`] order.
///
/// Written out here rather than fetched: PokeAPI has these for every language
/// but Turkish, but the type is on nearly every card, and eighteen fixed words
/// are not worth a request and a cache entry each. The Turkish names are the
/// ones Turkish fan communities settled on, since the games were never
/// released in Turkish.
const TYPE_NAMES: [(&str, [&str; 6]); 18] = [
    (
        "normal",
        ["Normal", "Normal", "Normal", "Normal", "Normal", "Normale"],
    ),
    ("fire", ["Fire", "Ateş", "Feuer", "Feu", "Fuego", "Fuoco"]),
    ("water", ["Water", "Su", "Wasser", "Eau", "Agua", "Acqua"]),
    (
        "electric",
        [
            "Electric",
            "Elektrik",
            "Elektro",
            "Électrik",
            "Eléctrico",
            "Elettro",
        ],
    ),
    (
        "grass",
        ["Grass", "Çimen", "Pflanze", "Plante", "Planta", "Erba"],
    ),
    ("ice", ["Ice", "Buz", "Eis", "Glace", "Hielo", "Ghiaccio"]),
    (
        "fighting",
        ["Fighting", "Dövüş", "Kampf", "Combat", "Lucha", "Lotta"],
    ),
    (
        "poison",
        ["Poison", "Zehir", "Gift", "Poison", "Veneno", "Veleno"],
    ),
    (
        "ground",
        ["Ground", "Yer", "Boden", "Sol", "Tierra", "Terra"],
    ),
    (
        "flying",
        ["Flying", "Uçan", "Flug", "Vol", "Volador", "Volante"],
    ),
    (
        "psychic",
        ["Psychic", "Psişik", "Psycho", "Psy", "Psíquico", "Psico"],
    ),
    (
        "bug",
        ["Bug", "Böcek", "Käfer", "Insecte", "Bicho", "Coleottero"],
    ),
    (
        "rock",
        ["Rock", "Kaya", "Gestein", "Roche", "Roca", "Roccia"],
    ),
    (
        "ghost",
        [
            "Ghost", "Hayalet", "Geist", "Spectre", "Fantasma", "Spettro",
        ],
    ),
    (
        "dragon",
        ["Dragon", "Ejderha", "Drache", "Dragon", "Dragón", "Drago"],
    ),
    (
        "dark",
        [
            "Dark",
            "Karanlık",
            "Unlicht",
            "Ténèbres",
            "Siniestro",
            "Buio",
        ],
    ),
    (
        "steel",
        ["Steel", "Çelik", "Stahl", "Acier", "Acero", "Acciaio"],
    ),
    ("fairy", ["Fairy", "Peri", "Fee", "Fée", "Hada", "Folletto"]),
];

/// The encounter methods most rows use, in each interface language, in the
/// order of [`Language::ALL`]. PokeAPI has sixty-odd methods, most of them for
/// one game's one gimmick; those are shown as their slug title-cased.
const METHOD_NAMES: [(&str, [&str; 6]); 15] = [
    (
        "walk",
        [
            "Walking",
            "Yürürken",
            "Laufen",
            "Marche",
            "Caminando",
            "Camminando",
        ],
    ),
    (
        "surf",
        ["Surfing", "Sörf", "Surfer", "Surf", "Surf", "Surf"],
    ),
    (
        "old-rod",
        [
            "Old Rod",
            "Eski Olta",
            "Angel",
            "Canne",
            "Caña Vieja",
            "Amo Vecchio",
        ],
    ),
    (
        "good-rod",
        [
            "Good Rod",
            "İyi Olta",
            "Profiangel",
            "Super Canne",
            "Caña Buena",
            "Amo Buono",
        ],
    ),
    (
        "super-rod",
        [
            "Super Rod",
            "Süper Olta",
            "Superangel",
            "Méga Canne",
            "Supercaña",
            "Super Amo",
        ],
    ),
    (
        "rock-smash",
        [
            "Rock Smash",
            "Kaya Kırma",
            "Zertrümmerer",
            "Éclate-Roc",
            "Golpe Roca",
            "Spaccaroccia",
        ],
    ),
    (
        "headbutt",
        [
            "Headbutt",
            "Kafa Atma",
            "Kopfnuss",
            "Coup d'Boule",
            "Cabezazo",
            "Bottintesta",
        ],
    ),
    (
        "gift",
        ["Gift", "Hediye", "Geschenk", "Cadeau", "Regalo", "Regalo"],
    ),
    (
        "gift-egg",
        [
            "Gift egg",
            "Hediye yumurta",
            "Geschenktes Ei",
            "Œuf offert",
            "Huevo regalado",
            "Uovo in regalo",
        ],
    ),
    (
        "npc-trade",
        [
            "Trade",
            "Takas",
            "Tausch",
            "Échange",
            "Intercambio",
            "Scambio",
        ],
    ),
    (
        "static",
        ["Static", "Sabit", "Statisch", "Fixe", "Fijo", "Fisso"],
    ),
    (
        "overworld",
        [
            "Overworld",
            "Açıkta",
            "Sichtbar",
            "Visible",
            "Visible",
            "Visibile",
        ],
    ),
    (
        "max-raid",
        [
            "Max Raid",
            "Maks Baskın",
            "Dyna-Raid",
            "Raid Dynamax",
            "Incursión Dinamax",
            "Raid Dynamax",
        ],
    ),
    (
        "sos",
        [
            "SOS call",
            "Yardım çağrısı",
            "Notruf",
            "Appel à l'aide",
            "Llamada SOS",
            "Richiamo SOS",
        ],
    ),
    (
        "wanderer",
        [
            "Wandering",
            "Dolaşan",
            "Umherstreifend",
            "Errant",
            "Errante",
            "Errante",
        ],
    ),
];

/// An encounter method's name in `language`, or its slug title-cased where the
/// table above has none.
pub fn method_name(language: Language, slug: &str) -> String {
    METHOD_NAMES
        .iter()
        .find(|(known, _)| *known == slug)
        .map(|(_, names)| names[language.index()].to_string())
        .unwrap_or_else(|| crate::models::title_case(slug))
}

/// The type slug a typed name stands for, in any of the six languages.
///
/// Any language rather than only the current one: someone who has learned
/// `type:geist` keeps it after switching the interface to English, and no
/// two languages use one word for different types, so nothing is ambiguous.
/// Accents are optional, because `type:electrik` is what a keyboard without
/// them produces.
pub fn type_slug(typed: &str) -> Option<&'static str> {
    let typed = fold(typed);
    TYPE_NAMES
        .iter()
        .find(|(slug, names)| *slug == typed || names.iter().any(|name| fold(name) == typed))
        .map(|(slug, _)| *slug)
}

/// Lowercases `text` and strips the accents the type names use, so a name
/// typed with or without them compares equal.
pub(crate) fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ı' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ğ' => 'g',
            'ñ' => 'n',
            'ş' => 's',
            other => other,
        })
        .collect()
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
    /// Labels for the field-guide rows on the info card.
    pub egg_groups_label: &'static str,
    /// A species with no gender. The gendered case is written in symbols and
    /// needs no label.
    pub genderless: &'static str,
    pub catch_rate_label: &'static str,
    /// The catch rate in words, shown beside the number so it says which way
    /// the scale runs.
    pub catch_hard: &'static str,
    pub catch_average: &'static str,
    pub catch_easy: &'static str,
    pub growth_label: &'static str,
    pub happiness_label: &'static str,
    pub habitat_label: &'static str,
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
    pub sort_total: &'static str,
    pub sort_speed: &'static str,
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
    /// Abbreviation for "generation", put before a roman numeral in the titles
    /// of the cards that read a generation's type rules: `Gen I`.
    pub generation_tag: &'static str,
    /// Said of a species that was not in the games of the generation a card
    /// is reading, in place of an answer for it.
    pub absent_from_generation: &'static str,
    /// Marker on an immunity the species might not actually have, because the
    /// ability granting it is only one of several it could carry.
    pub immunity_maybe: &'static str,
    /// Hint at the foot of the team card. Carries the card's own keys, since
    /// it is the one overlay with a cursor and a binding beyond closing.
    pub team_close_hint: &'static str,
    /// Replaces the hint for a moment once `E` has copied the party.
    pub team_copied: &'static str,
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
    /// Title of the stat calculator card.
    pub calc_title: &'static str,
    /// Row labels and column headings on the calculator. "IV" and "EV" are
    /// written the same in every language and are not here.
    pub calc_level: &'static str,
    pub calc_nature: &'static str,
    pub calc_base: &'static str,
    pub calc_stat: &'static str,
    /// Hint at the foot of the calculator card.
    pub calc_hint: &'static str,
    /// Label for the alternate-forms row on the info card.
    pub forms_label: &'static str,
    /// Title of the forms card.
    pub forms_title: &'static str,
    /// Hint at the foot of the forms card, which has a cursor and a jump of
    /// its own.
    pub forms_close_hint: &'static str,
    /// Title of the ability and item lookup card, and the names of its two
    /// sides.
    pub lookup_title: &'static str,
    pub lookup_abilities: &'static str,
    pub lookup_items: &'static str,
    /// Shown in place of the list when nothing matches the card's query.
    pub lookup_none: &'static str,
    /// Hint at the foot of the lookup card.
    pub lookup_hint: &'static str,
    /// Labels on an item's record.
    pub lookup_category: &'static str,
    pub lookup_fling: &'static str,
    /// Heading above the Pokemon found holding an item in the wild, and what
    /// stands in for it when there are none.
    pub lookup_holders: &'static str,
    pub lookup_no_holders: &'static str,
    /// Title of the encounters card.
    pub encounters_title: &'static str,
    /// Shown when PokeAPI records no wild encounters at all, followed by the
    /// note saying why that is not the same as "cannot be caught".
    pub encounters_none: &'static str,
    pub encounters_note: &'static str,
    /// Legend for the `*` on a row that only happens under some condition.
    pub encounters_conditional: &'static str,
    /// Hint at the foot of the encounters card.
    pub encounters_hint: &'static str,
    /// Title of the quiz card.
    pub quiz_title: &'static str,
    /// Label in front of the answer being typed.
    pub quiz_prompt: &'static str,
    /// Shown under a wrong answer until the next keystroke.
    pub quiz_missed: &'static str,
    /// Put in front of the revealed name, after a right answer and after
    /// giving up respectively.
    pub quiz_right: &'static str,
    pub quiz_gave_up: &'static str,
    /// Labels for the current and best streaks.
    pub quiz_streak: &'static str,
    pub quiz_best: &'static str,
    /// Hints at the foot of the quiz card, while asking and once answered.
    pub quiz_hint_asking: &'static str,
    pub quiz_hint_answered: &'static str,
    /// Why `N` does nothing in a terminal without colour.
    pub quiz_needs_colour: &'static str,
    /// Column headings on the encounters card.
    pub col_location: &'static str,
    pub col_method: &'static str,
    pub col_level: &'static str,
    pub col_chance: &'static str,
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
    /// The party card, which has a cursor and a binding of its own.
    pub ctx_party: &'static str,
    /// The forms card, which likewise has a cursor and a jump.
    pub ctx_forms: &'static str,
    pub ctx_cards: &'static str,
    pub act_move: &'static str,
    pub act_jump10: &'static str,
    pub act_load: &'static str,
    pub act_search: &'static str,
    pub act_evolutions: &'static str,
    pub act_types: &'static str,
    pub act_abilities: &'static str,
    pub act_moves: &'static str,
    /// `I`: the stat calculator.
    pub act_stat_calc: &'static str,
    /// `V`: the species' alternate forms.
    pub act_forms: &'static str,
    pub act_shiny: &'static str,
    /// `R`: a random species out of whatever the list is narrowed to.
    pub act_random: &'static str,
    pub act_party_toggle: &'static str,
    /// `B`: mark or unmark the highlighted species as a favourite.
    pub act_favourite: &'static str,
    pub act_party_card: &'static str,
    pub act_sort: &'static str,
    pub act_language: &'static str,
    /// `g` / `G`: step the generation the type cards read.
    pub act_generation: &'static str,
    pub act_help: &'static str,
    pub act_quit: &'static str,
    pub act_load_back: &'static str,
    pub act_back: &'static str,
    pub act_by_type: &'static str,
    pub act_by_ability: &'static str,
    pub act_by_egg: &'static str,
    pub act_by_generation: &'static str,
    /// `fav:`: only the species marked with `B`.
    pub act_by_favourite: &'static str,
    /// `spe>=100`: a comparison on a base stat.
    pub act_by_stat: &'static str,
    pub act_chain_move: &'static str,
    pub act_chain_jump: &'static str,
    pub act_form_jump: &'static str,
    pub ctx_lookup: &'static str,
    pub act_lookup: &'static str,
    pub act_lookup_switch: &'static str,
    pub act_lookup_apply: &'static str,
    pub act_by_held: &'static str,
    pub act_encounters: &'static str,
    pub act_quiz: &'static str,
    pub act_chain_expand: &'static str,
    pub act_compare: &'static str,
    /// `E` on the party card: copy it as a Showdown team.
    pub act_copy_team: &'static str,
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
            egg_groups_label: "Egg groups",
            genderless: "Genderless",
            catch_rate_label: "Catch rate",
            catch_hard: "hard",
            catch_average: "average",
            catch_easy: "easy",
            growth_label: "Growth",
            happiness_label: "Happiness",
            habitat_label: "Habitat",
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
            sort_total: "BST",
            sort_speed: "Speed",
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
            calc_title: " Stat calculator ",
            calc_level: "Level",
            calc_nature: "Nature",
            calc_base: "Base",
            calc_stat: "Stat",
            calc_hint: "↑ ↓ ← → Move · 0-9 + - Edit · R Reset · Esc Close",
            forms_label: "Forms",
            forms_title: " Forms ",
            forms_close_hint: "↑ ↓ Select · Enter Open · Esc / V to close",
            lookup_title: " Look Up ",
            lookup_abilities: "Abilities",
            lookup_items: "Items",
            lookup_none: "Nothing matches",
            lookup_hint: "Tab Abilities ↔ Items · ↑ ↓ Select · Enter Show Pokémon · Esc Close",
            lookup_category: "Category",
            lookup_fling: "Fling power",
            lookup_holders: "Held in the wild by",
            lookup_no_holders: "Not found held in the wild",
            encounters_title: " Where to Find ",
            encounters_none: "No wild encounters recorded",
            encounters_note: "PokeAPI's encounter data is incomplete for recent games, so this does not mean it cannot be caught there.",
            encounters_conditional: "* only at some times, in some weather or at some point in the story",
            encounters_hint: "← → Game · ↑ ↓ Scroll · Esc / W Close",
            quiz_title: " Who's That Pokémon? ",
            quiz_prompt: "Name:",
            quiz_missed: "Not quite, try again",
            quiz_right: "Right! It's",
            quiz_gave_up: "It's",
            quiz_streak: "Streak",
            quiz_best: "Best",
            quiz_hint_asking: "Type a name · Enter Answer · ? Give up · Esc Close",
            quiz_hint_answered: "Enter / N Next · Esc Close",
            quiz_needs_colour: "The quiz needs a terminal with colour",
            col_location: "Location",
            col_method: "Method",
            col_level: "Level",
            col_chance: "Chance",
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
            generation_tag: "Gen",
            absent_from_generation: "Not in these games",
            immunity_maybe: "possible",
            team_close_hint: "↑ ↓ Select · C Pin / compare · E Copy · Esc Close",
            team_copied: "Copied as a Showdown team",
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
            close_hint: "g G Gen · Esc / T to close",
            compare_title: " Head to Head ",
            compare_best_hit: "Best same-type hit",
            compare_tie: "level",
            compare_hint: "g G Gen · Esc / C to close",
            help_card: HelpStrings {
                title: " Help ",
                ctx_list: "List",
                ctx_search: "Search box",
                ctx_evolution: "Evolution panel",
                ctx_party: "Party card",
                ctx_forms: "Forms card",
                ctx_cards: "Any card",
                act_move: "Move selection",
                act_jump10: "Jump ten",
                act_load: "Load",
                act_search: "Search",
                act_evolutions: "Evolutions",
                act_types: "Type matchups",
                act_abilities: "Abilities",
                act_moves: "Moves",
                act_stat_calc: "Stat calculator",
                act_forms: "Alternate forms",
                act_shiny: "Shiny artwork",
                act_random: "Random from the list",
                act_party_toggle: "Add / remove from party",
                act_favourite: "Mark / unmark favourite",
                act_party_card: "Party",
                act_sort: "Sort order",
                act_language: "Language",
                act_generation: "Generation of the type chart",
                act_help: "This help",
                act_quit: "Quit",
                act_load_back: "Load and return",
                act_back: "Back to list",
                act_by_type: "Filter by type",
                act_by_ability: "Filter by ability",
                act_by_egg: "Filter by egg group",
                act_by_generation: "Filter by generation",
                act_by_favourite: "Only favourites",
                act_by_stat: "Filter by base stat",
                act_chain_move: "Move between stages",
                act_chain_jump: "Jump to stage",
                act_form_jump: "Open form",
                ctx_lookup: "Lookup card",
                act_lookup: "Look up abilities and items",
                act_lookup_switch: "Abilities ↔ items",
                act_lookup_apply: "List the Pokémon with it",
                act_by_held: "Filter by held item",
                act_encounters: "Where to find it",
                act_quiz: "Who's that Pokémon? quiz",
                act_chain_expand: "Full-screen chain",
                act_compare: "Pin / compare two species",
                act_copy_team: "Copy for Showdown",
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
            egg_groups_label: "Yumurta grupları",
            genderless: "Cinsiyetsiz",
            catch_rate_label: "Yakalama oranı",
            catch_hard: "zor",
            catch_average: "orta",
            catch_easy: "kolay",
            growth_label: "Gelişim",
            happiness_label: "Mutluluk",
            habitat_label: "Yaşam alanı",
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
            sort_total: "BST",
            sort_speed: "Hız",
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
            calc_title: " Değer hesaplayıcı ",
            calc_level: "Seviye",
            calc_nature: "Doğa",
            calc_base: "Temel",
            calc_stat: "Sonuç",
            calc_hint: "↑ ↓ ← → Gezin · 0-9 + - Düzenle · R Sıfırla · Esc Kapat",
            forms_label: "Formlar",
            forms_title: " Formlar ",
            forms_close_hint: "↑ ↓ Seç · Enter Aç · Esc / V Kapat",
            lookup_title: " Dizin ",
            lookup_abilities: "Yetenekler",
            lookup_items: "Eşyalar",
            lookup_none: "Eşleşen yok",
            lookup_hint: "Tab Yetenek ↔ Eşya · ↑ ↓ Seç · Enter Pokémonları göster · Esc Kapat",
            lookup_category: "Kategori",
            lookup_fling: "Fırlatma gücü",
            lookup_holders: "Doğada taşıyanlar",
            lookup_no_holders: "Doğada taşıyan yok",
            encounters_title: " Nerede Bulunur ",
            encounters_none: "Kayıtlı yabani karşılaşma yok",
            encounters_note: "PokeAPI'nin karşılaşma verisi yeni oyunlar için eksik; bu, orada yakalanamayacağı anlamına gelmez.",
            encounters_conditional: "* yalnızca belirli saat, hava ya da hikâye aşamasında",
            encounters_hint: "← → Oyun · ↑ ↓ Kaydır · Esc / W Kapat",
            quiz_title: " Bu Pokémon Kim? ",
            quiz_prompt: "Adı:",
            quiz_missed: "Olmadı, tekrar dene",
            quiz_right: "Doğru! Bu",
            quiz_gave_up: "Bu",
            quiz_streak: "Seri",
            quiz_best: "En iyi",
            quiz_hint_asking: "Bir ad yaz · Enter Cevapla · ? Pes et · Esc Kapat",
            quiz_hint_answered: "Enter / N Sıradaki · Esc Kapat",
            quiz_needs_colour: "Quiz için renkli bir terminal gerekir",
            col_location: "Konum",
            col_method: "Yöntem",
            col_level: "Seviye",
            col_chance: "Şans",
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
            generation_tag: "Jen.",
            absent_from_generation: "Bu oyunlarda yok",
            immunity_maybe: "olası",
            team_close_hint: "↑ ↓ Seç · C Karşılaştır · E Kopyala · Esc Kapat",
            team_copied: "Showdown takımı olarak kopyalandı",
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
            close_hint: "g G Jen. · Kapatmak için Esc / T",
            compare_title: " Karşılaştırma ",
            compare_best_hit: "En sert aynı tipten vuruş",
            compare_tie: "eşit",
            compare_hint: "g G Jen. · Kapatmak için Esc / C",
            help_card: HelpStrings {
                title: " Yardım ",
                ctx_list: "Liste",
                ctx_search: "Arama kutusu",
                ctx_evolution: "Evrim paneli",
                ctx_party: "Takım kartı",
                ctx_forms: "Form kartı",
                ctx_cards: "Tüm kartlar",
                act_move: "Seçimi taşı",
                act_jump10: "On atla",
                act_load: "Yükle",
                act_search: "Ara",
                act_evolutions: "Evrimler",
                act_types: "Tip eşleşmeleri",
                act_abilities: "Yetenekler",
                act_moves: "Hareketler",
                act_stat_calc: "Değer hesaplayıcı",
                act_forms: "Alternatif formlar",
                act_shiny: "Parlak görsel",
                act_random: "Listeden rastgele",
                act_party_toggle: "Takıma ekle / çıkar",
                act_favourite: "Favorilere ekle / çıkar",
                act_party_card: "Takım",
                act_sort: "Sıralama",
                act_language: "Dil",
                act_generation: "Tip tablosunun jenerasyonu",
                act_help: "Bu yardım",
                act_quit: "Çıkış",
                act_load_back: "Yükle ve dön",
                act_back: "Listeye dön",
                act_by_type: "Tipe göre süz",
                act_by_ability: "Yeteneğe göre süz",
                act_by_egg: "Yumurta grubuna göre süz",
                act_by_generation: "Jenerasyona göre süz",
                act_by_favourite: "Sadece favoriler",
                act_by_stat: "Temel değere göre süz",
                act_chain_move: "Aşamalar arasında gez",
                act_chain_jump: "Aşamaya atla",
                act_form_jump: "Formu aç",
                ctx_lookup: "Dizin kartı",
                act_lookup: "Yetenek ve eşya ara",
                act_lookup_switch: "Yetenek ↔ eşya",
                act_lookup_apply: "Sahip Pokémonları listele",
                act_by_held: "Taşınan eşyaya göre süz",
                act_encounters: "Nerede bulunur",
                act_quiz: "Bu Pokémon kim? oyunu",
                act_chain_expand: "Zinciri tam ekran aç",
                act_compare: "Sabitle / iki türü karşılaştır",
                act_copy_team: "Showdown'a kopyala",
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
            egg_groups_label: "Ei-Gruppen",
            genderless: "Geschlechtslos",
            catch_rate_label: "Fangrate",
            catch_hard: "schwer",
            catch_average: "mittel",
            catch_easy: "leicht",
            growth_label: "Wachstum",
            happiness_label: "Freundschaft",
            habitat_label: "Lebensraum",
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
            sort_total: "BST",
            sort_speed: "Tempo",
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
            calc_title: " Werte-Rechner ",
            calc_level: "Level",
            calc_nature: "Wesen",
            calc_base: "Basis",
            calc_stat: "Wert",
            calc_hint: "↑ ↓ ← → Wählen · 0-9 + - Ändern · R Neu · Esc Schließen",
            forms_label: "Formen",
            forms_title: " Formen ",
            forms_close_hint: "↑ ↓ Wählen · Enter Öffnen · Esc / V Schließen",
            lookup_title: " Nachschlagen ",
            lookup_abilities: "Fähigkeiten",
            lookup_items: "Items",
            lookup_none: "Keine Treffer",
            lookup_hint:
                "Tab Fähigkeiten ↔ Items · ↑ ↓ Wählen · Enter Pokémon zeigen · Esc Schließen",
            lookup_category: "Kategorie",
            lookup_fling: "Schleuder-Stärke",
            lookup_holders: "In freier Wildbahn getragen von",
            lookup_no_holders: "In freier Wildbahn von keinem getragen",
            encounters_title: " Fundorte ",
            encounters_none: "Keine wilden Begegnungen erfasst",
            encounters_note: "Die Begegnungsdaten von PokeAPI sind für neuere Spiele unvollständig; das heißt nicht, dass es dort nicht gefangen werden kann.",
            encounters_conditional: "* nur zu bestimmten Zeiten, bei bestimmtem Wetter oder an bestimmten Punkten der Geschichte",
            encounters_hint: "← → Spiel · ↑ ↓ Blättern · Esc / W Schließen",
            quiz_title: " Wer ist das Pokémon? ",
            quiz_prompt: "Name:",
            quiz_missed: "Nicht ganz, versuch's noch mal",
            quiz_right: "Richtig! Es ist",
            quiz_gave_up: "Es ist",
            quiz_streak: "Serie",
            quiz_best: "Rekord",
            quiz_hint_asking: "Namen tippen · Enter Antworten · ? Aufgeben · Esc Schließen",
            quiz_hint_answered: "Enter / N Weiter · Esc Schließen",
            quiz_needs_colour: "Das Quiz braucht ein Terminal mit Farben",
            col_location: "Ort",
            col_method: "Methode",
            col_level: "Level",
            col_chance: "Chance",
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
            generation_tag: "Gen.",
            absent_from_generation: "In diesen Spielen nicht dabei",
            immunity_maybe: "möglich",
            team_close_hint: "↑ ↓ Wählen · C Anheften · E Kopieren · Esc Schließen",
            team_copied: "Als Showdown-Team kopiert",
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
            close_hint: "g G Gen. · Esc / T zum Schließen",
            compare_title: " Direktvergleich ",
            compare_best_hit: "Bester Treffer eigenen Typs",
            compare_tie: "gleich",
            compare_hint: "g G Gen. · Esc / C zum Schließen",
            help_card: HelpStrings {
                title: " Hilfe ",
                ctx_list: "Liste",
                ctx_search: "Suchfeld",
                ctx_evolution: "Entwicklungsfeld",
                ctx_party: "Teamkarte",
                ctx_forms: "Formenkarte",
                ctx_cards: "Alle Karten",
                act_move: "Auswahl bewegen",
                act_jump10: "Zehn springen",
                act_load: "Laden",
                act_search: "Suche",
                act_evolutions: "Entwicklungen",
                act_types: "Typ-Matchups",
                act_abilities: "Fähigkeiten",
                act_moves: "Attacken",
                act_stat_calc: "Werte-Rechner",
                act_forms: "Andere Formen",
                act_shiny: "Schillernde Grafik",
                act_random: "Zufällig aus der Liste",
                act_party_toggle: "Team hinzu / entfernen",
                act_favourite: "Favorit an / aus",
                act_party_card: "Team",
                act_sort: "Sortierung",
                act_language: "Sprache",
                act_generation: "Generation der Typentabelle",
                act_help: "Diese Hilfe",
                act_quit: "Beenden",
                act_load_back: "Laden und zurück",
                act_back: "Zurück zur Liste",
                act_by_type: "Nach Typ filtern",
                act_by_ability: "Nach Fähigkeit filtern",
                act_by_egg: "Nach Ei-Gruppe filtern",
                act_by_generation: "Nach Generation filtern",
                act_by_favourite: "Nur Favoriten",
                act_by_stat: "Nach Werten filtern",
                act_chain_move: "Zwischen Stufen",
                act_chain_jump: "Zur Stufe springen",
                act_form_jump: "Form öffnen",
                ctx_lookup: "Nachschlagekarte",
                act_lookup: "Fähigkeiten und Items nachschlagen",
                act_lookup_switch: "Fähigkeiten ↔ Items",
                act_lookup_apply: "Pokémon damit auflisten",
                act_by_held: "Nach getragenem Item filtern",
                act_encounters: "Fundorte",
                act_quiz: "Quiz: Wer ist das Pokémon?",
                act_chain_expand: "Reihe im Vollbild",
                act_compare: "Anheften / zwei vergleichen",
                act_copy_team: "Showdown-Kopie",
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
            egg_groups_label: "Groupes d'Œuf",
            genderless: "Asexué",
            catch_rate_label: "Taux de capture",
            catch_hard: "difficile",
            catch_average: "moyen",
            catch_easy: "facile",
            growth_label: "Croissance",
            happiness_label: "Bonheur",
            habitat_label: "Habitat",
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
            sort_total: "Total",
            sort_speed: "Vitesse",
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
            calc_title: " Calcul des stats ",
            calc_level: "Niveau",
            calc_nature: "Nature",
            calc_base: "Base",
            calc_stat: "Stat",
            calc_hint: "↑ ↓ ← → Choisir · 0-9 + - Modifier · R Effacer · Échap Fermer",
            forms_label: "Formes",
            forms_title: " Formes ",
            forms_close_hint: "↑ ↓ Choisir · Entrée Ouvrir · Esc / V Fermer",
            lookup_title: " Recherche ",
            lookup_abilities: "Talents",
            lookup_items: "Objets",
            lookup_none: "Aucun résultat",
            lookup_hint:
                "Tab Talents ↔ Objets · ↑ ↓ Choisir · Entrée Voir les Pokémon · Esc Fermer",
            lookup_category: "Catégorie",
            lookup_fling: "Puissance de Dégommage",
            lookup_holders: "Porté à l'état sauvage par",
            lookup_no_holders: "Porté par aucun Pokémon sauvage",
            encounters_title: " Où le trouver ",
            encounters_none: "Aucune rencontre sauvage enregistrée",
            encounters_note: "Les données de rencontre de PokeAPI sont incomplètes pour les jeux récents ; cela ne veut pas dire qu'il ne peut pas y être capturé.",
            encounters_conditional: "* seulement à certaines heures, météos ou étapes de l'histoire",
            encounters_hint: "← → Jeu · ↑ ↓ Défiler · Esc / W Fermer",
            quiz_title: " Quel est ce Pokémon ? ",
            quiz_prompt: "Nom :",
            quiz_missed: "Pas tout à fait, réessaie",
            quiz_right: "Bravo ! C'est",
            quiz_gave_up: "C'est",
            quiz_streak: "Série",
            quiz_best: "Record",
            quiz_hint_asking: "Tape un nom · Entrée Répondre · ? Abandonner · Esc Fermer",
            quiz_hint_answered: "Entrée / N Suivant · Esc Fermer",
            quiz_needs_colour: "Le quiz demande un terminal en couleurs",
            col_location: "Lieu",
            col_method: "Méthode",
            col_level: "Niveau",
            col_chance: "Chance",
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
            generation_tag: "Gén.",
            absent_from_generation: "Absent de ces jeux",
            immunity_maybe: "possible",
            team_close_hint: "↑ ↓ Choisir · C Comparer · E Copier · Esc Fermer",
            team_copied: "Copiée comme équipe Showdown",
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
            close_hint: "g G Gén. · Esc / T pour fermer",
            compare_title: " Face à Face ",
            compare_best_hit: "Meilleure attaque de même type",
            compare_tie: "égalité",
            compare_hint: "g G Gén. · Esc / C pour fermer",
            help_card: HelpStrings {
                title: " Aide ",
                ctx_list: "Liste",
                ctx_search: "Recherche",
                ctx_evolution: "Panneau d'évolution",
                ctx_party: "Carte d'équipe",
                ctx_forms: "Carte des formes",
                ctx_cards: "Toute carte",
                act_move: "Déplacer la sélection",
                act_jump10: "Sauter dix",
                act_load: "Charger",
                act_search: "Rechercher",
                act_evolutions: "Évolutions",
                act_types: "Affinités de type",
                act_abilities: "Talents",
                act_moves: "Capacités",
                act_stat_calc: "Calcul des stats",
                act_forms: "Autres formes",
                act_shiny: "Illustration chromatique",
                act_random: "Au hasard dans la liste",
                act_party_toggle: "Ajouter / retirer de l'équipe",
                act_favourite: "Ajouter / retirer des favoris",
                act_party_card: "Équipe",
                act_sort: "Tri",
                act_language: "Langue",
                act_generation: "Génération de la table des types",
                act_help: "Cette aide",
                act_quit: "Quitter",
                act_load_back: "Charger et revenir",
                act_back: "Retour à la liste",
                act_by_type: "Filtrer par type",
                act_by_ability: "Filtrer par talent",
                act_by_egg: "Filtrer par groupe d'œufs",
                act_by_generation: "Filtrer par génération",
                act_by_favourite: "Favoris seulement",
                act_by_stat: "Filtrer par stat",
                act_chain_move: "Entre les stades",
                act_chain_jump: "Aller au stade",
                act_form_jump: "Ouvrir la forme",
                ctx_lookup: "Carte de recherche",
                act_lookup: "Chercher talents et objets",
                act_lookup_switch: "Talents ↔ objets",
                act_lookup_apply: "Lister les Pokémon concernés",
                act_by_held: "Filtrer par objet tenu",
                act_encounters: "Où le trouver",
                act_quiz: "Quiz : Quel est ce Pokémon ?",
                act_chain_expand: "Chaîne en plein écran",
                act_compare: "Épingler / comparer deux espèces",
                act_copy_team: "Copier pour Showdown",
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
            egg_groups_label: "Grupos huevo",
            genderless: "Sin género",
            catch_rate_label: "Ratio de captura",
            catch_hard: "difícil",
            catch_average: "media",
            catch_easy: "fácil",
            growth_label: "Crecimiento",
            happiness_label: "Amistad",
            habitat_label: "Hábitat",
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
            sort_total: "Total",
            sort_speed: "Velocid.",
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
            calc_title: " Calculadora de stats ",
            calc_level: "Nivel",
            calc_nature: "Naturaleza",
            calc_base: "Base",
            calc_stat: "Valor",
            calc_hint: "↑ ↓ ← → Mover · 0-9 + - Editar · R Reiniciar · Esc Cerrar",
            forms_label: "Formas",
            forms_title: " Formas ",
            forms_close_hint: "↑ ↓ Elegir · Enter Abrir · Esc / V Cerrar",
            lookup_title: " Consulta ",
            lookup_abilities: "Habilidades",
            lookup_items: "Objetos",
            lookup_none: "Sin resultados",
            lookup_hint: "Tab Habilidades ↔ Objetos · ↑ ↓ Elegir · Enter Ver Pokémon · Esc Cerrar",
            lookup_category: "Categoría",
            lookup_fling: "Potencia de Lanzamiento",
            lookup_holders: "Llevado en estado salvaje por",
            lookup_no_holders: "Ningún Pokémon salvaje lo lleva",
            encounters_title: " Dónde encontrarlo ",
            encounters_none: "No hay encuentros salvajes registrados",
            encounters_note: "Los datos de encuentros de PokeAPI están incompletos para los juegos recientes; eso no significa que no se pueda capturar en ellos.",
            encounters_conditional: "* solo a ciertas horas, con cierto clima o en cierto punto de la historia",
            encounters_hint: "← → Juego · ↑ ↓ Desplazar · Esc / W Cerrar",
            quiz_title: " ¿Quién es ese Pokémon? ",
            quiz_prompt: "Nombre:",
            quiz_missed: "Casi, inténtalo otra vez",
            quiz_right: "¡Correcto! Es",
            quiz_gave_up: "Es",
            quiz_streak: "Racha",
            quiz_best: "Récord",
            quiz_hint_asking: "Escribe un nombre · Enter Responder · ? Rendirse · Esc Cerrar",
            quiz_hint_answered: "Enter / N Siguiente · Esc Cerrar",
            quiz_needs_colour: "El quiz necesita una terminal con color",
            col_location: "Lugar",
            col_method: "Método",
            col_level: "Nivel",
            col_chance: "Prob.",
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
            generation_tag: "Gen.",
            absent_from_generation: "No está en estos juegos",
            immunity_maybe: "posible",
            team_close_hint: "↑ ↓ Elegir · C Comparar · E Copiar · Esc Cerrar",
            team_copied: "Copiado como equipo de Showdown",
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
            close_hint: "g G Gen. · Esc / T para cerrar",
            compare_title: " Cara a Cara ",
            compare_best_hit: "Mejor golpe del mismo tipo",
            compare_tie: "empate",
            compare_hint: "g G Gen. · Esc / C para cerrar",
            help_card: HelpStrings {
                title: " Ayuda ",
                ctx_list: "Lista",
                ctx_search: "Búsqueda",
                ctx_evolution: "Panel de evolución",
                ctx_party: "Tarjeta de equipo",
                ctx_forms: "Tarjeta de formas",
                ctx_cards: "Cualquier ficha",
                act_move: "Mover selección",
                act_jump10: "Saltar diez",
                act_load: "Cargar",
                act_search: "Buscar",
                act_evolutions: "Evoluciones",
                act_types: "Efectividad de tipos",
                act_abilities: "Habilidades",
                act_moves: "Movimientos",
                act_stat_calc: "Calculadora de stats",
                act_forms: "Otras formas",
                act_shiny: "Ilustración variocolor",
                act_random: "Al azar de la lista",
                act_party_toggle: "Añadir / quitar del equipo",
                act_favourite: "Marcar / quitar favorito",
                act_party_card: "Equipo",
                act_sort: "Orden",
                act_language: "Idioma",
                act_generation: "Generación de la tabla de tipos",
                act_help: "Esta ayuda",
                act_quit: "Salir",
                act_load_back: "Cargar y volver",
                act_back: "Volver a la lista",
                act_by_type: "Filtrar por tipo",
                act_by_ability: "Filtrar por habilidad",
                act_by_egg: "Filtrar por grupo huevo",
                act_by_generation: "Filtrar por generación",
                act_by_favourite: "Solo favoritos",
                act_by_stat: "Por estadística base",
                act_chain_move: "Entre etapas",
                act_chain_jump: "Ir a la etapa",
                act_form_jump: "Abrir forma",
                ctx_lookup: "Tarjeta de consulta",
                act_lookup: "Consultar habilidades y objetos",
                act_lookup_switch: "Habilidades ↔ objetos",
                act_lookup_apply: "Listar los Pokémon con ello",
                act_by_held: "Filtrar por objeto equipado",
                act_encounters: "Dónde encontrarlo",
                act_quiz: "Quiz: ¿Quién es ese Pokémon?",
                act_chain_expand: "Cadena a pantalla completa",
                act_compare: "Fijar / comparar dos especies",
                act_copy_team: "Copiar para Showdown",
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
            egg_groups_label: "Gruppi uova",
            genderless: "Senza sesso",
            catch_rate_label: "Tasso di cattura",
            catch_hard: "difficile",
            catch_average: "medio",
            catch_easy: "facile",
            growth_label: "Crescita",
            happiness_label: "Felicità",
            habitat_label: "Habitat",
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
            sort_total: "Totale",
            sort_speed: "Velocità",
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
            calc_title: " Calcolo statistiche ",
            calc_level: "Livello",
            calc_nature: "Natura",
            calc_base: "Base",
            calc_stat: "Valore",
            calc_hint: "↑ ↓ ← → Muovi · 0-9 + - Modifica · R Azzera · Esc Chiudi",
            forms_label: "Forme",
            forms_title: " Forme ",
            forms_close_hint: "↑ ↓ Scegli · Invio Apri · Esc / V Chiudi",
            lookup_title: " Consulta ",
            lookup_abilities: "Abilità",
            lookup_items: "Strumenti",
            lookup_none: "Nessun risultato",
            lookup_hint: "Tab Abilità ↔ Strumenti · ↑ ↓ Scegli · Invio Mostra Pokémon · Esc Chiudi",
            lookup_category: "Categoria",
            lookup_fling: "Potenza di Lancio",
            lookup_holders: "Tenuto allo stato selvatico da",
            lookup_no_holders: "Nessun Pokémon selvatico lo tiene",
            encounters_title: " Dove trovarlo ",
            encounters_none: "Nessun incontro selvatico registrato",
            encounters_note: "I dati sugli incontri di PokeAPI sono incompleti per i giochi recenti: non significa che non si possa catturare lì.",
            encounters_conditional: "* solo in certi orari, con certo meteo o in certi punti della storia",
            encounters_hint: "← → Gioco · ↑ ↓ Scorri · Esc / W Chiudi",
            quiz_title: " Chi è quel Pokémon? ",
            quiz_prompt: "Nome:",
            quiz_missed: "Non proprio, riprova",
            quiz_right: "Giusto! È",
            quiz_gave_up: "È",
            quiz_streak: "Serie",
            quiz_best: "Record",
            quiz_hint_asking: "Scrivi un nome · Invio Rispondi · ? Arrenditi · Esc Chiudi",
            quiz_hint_answered: "Invio / N Avanti · Esc Chiudi",
            quiz_needs_colour: "Il quiz richiede un terminale a colori",
            col_location: "Luogo",
            col_method: "Metodo",
            col_level: "Livello",
            col_chance: "Prob.",
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
            generation_tag: "Gen.",
            absent_from_generation: "Assente in questi giochi",
            immunity_maybe: "possibile",
            team_close_hint: "↑ ↓ Scegli · C Confronta · E Copia · Esc Chiudi",
            team_copied: "Copiata come squadra Showdown",
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
            close_hint: "g G Gen. · Esc / T per chiudere",
            compare_title: " Testa a Testa ",
            compare_best_hit: "Miglior colpo dello stesso tipo",
            compare_tie: "pari",
            compare_hint: "g G Gen. · Esc / C per chiudere",
            help_card: HelpStrings {
                title: " Aiuto ",
                ctx_list: "Lista",
                ctx_search: "Ricerca",
                ctx_evolution: "Pannello evoluzioni",
                ctx_party: "Scheda squadra",
                ctx_forms: "Scheda forme",
                ctx_cards: "Ogni scheda",
                act_move: "Sposta selezione",
                act_jump10: "Salta dieci",
                act_load: "Carica",
                act_search: "Cerca",
                act_evolutions: "Evoluzioni",
                act_types: "Efficacia dei tipi",
                act_abilities: "Abilità",
                act_moves: "Mosse",
                act_stat_calc: "Calcolo statistiche",
                act_forms: "Altre forme",
                act_shiny: "Illustrazione cromatica",
                act_random: "A caso dalla lista",
                act_party_toggle: "Metti / togli in squadra",
                act_favourite: "Metti / togli nei preferiti",
                act_party_card: "Squadra",
                act_sort: "Ordinamento",
                act_language: "Lingua",
                act_generation: "Generazione della tabella dei tipi",
                act_help: "Questo aiuto",
                act_quit: "Esci",
                act_load_back: "Carica e torna",
                act_back: "Torna alla lista",
                act_by_type: "Filtra per tipo",
                act_by_ability: "Filtra per abilità",
                act_by_egg: "Filtra per gruppo uova",
                act_by_generation: "Filtra per generazione",
                act_by_favourite: "Solo preferiti",
                act_by_stat: "Filtra per stat base",
                act_chain_move: "Tra gli stadi",
                act_chain_jump: "Vai allo stadio",
                act_form_jump: "Apri forma",
                ctx_lookup: "Scheda consulta",
                act_lookup: "Consulta abilità e strumenti",
                act_lookup_switch: "Abilità ↔ strumenti",
                act_lookup_apply: "Elenca i Pokémon che ce l'hanno",
                act_by_held: "Filtra per strumento tenuto",
                act_encounters: "Dove trovarlo",
                act_quiz: "Quiz: Chi è quel Pokémon?",
                act_chain_expand: "Catena a schermo intero",
                act_compare: "Fissa / confronta due specie",
                act_copy_team: "Copia per Showdown",
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
    use std::collections::HashMap;

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

    #[test]
    fn every_type_in_the_chart_has_a_name_in_every_language() {
        for slug in crate::typechart::TYPES {
            for language in Language::ALL {
                let name = language.type_name(slug);
                assert!(!name.is_empty(), "{slug} has no {language:?} name");
                assert_eq!(type_slug(&name), Some(slug), "{name} does not read back");
            }
        }
    }

    #[test]
    fn no_word_names_two_different_types_across_languages() {
        let mut seen: HashMap<String, &str> = HashMap::new();
        for (slug, names) in TYPE_NAMES {
            for name in names.iter().copied().chain([slug]) {
                if let Some(other) = seen.insert(fold(name), slug) {
                    assert_eq!(other, slug, "{name} names both {other} and {slug}");
                }
            }
        }
    }

    #[test]
    fn a_type_name_is_read_with_or_without_its_accents() {
        assert_eq!(type_slug("Électrik"), Some("electric"));
        assert_eq!(type_slug("electrik"), Some("electric"));
        assert_eq!(type_slug("KARANLIK"), Some("dark"));
        assert_eq!(type_slug("celik"), Some("steel"));
        assert_eq!(type_slug("ghost"), Some("ghost"));
        assert_eq!(type_slug("gho"), None);
    }

    #[test]
    fn a_type_the_table_does_not_know_is_title_cased() {
        assert_eq!(Language::German.type_name("stellar"), "Stellar");
        assert_eq!(Language::German.type_name("ghost"), "Geist");
    }
}
