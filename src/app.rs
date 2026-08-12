//! Application state machine and async orchestration.
//!
//! The UI never blocks: network work is performed in detached `tokio` tasks
//! that report back over an `mpsc` channel. Each spawned task is a *producer*;
//! the main loop in [`App::run`] is the single *consumer*, draining the channel
//! alongside terminal input and a steady animation tick via `tokio::select!`.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::api;
use crate::cache;
use crate::i18n::Language;
use crate::models::{AbilityInfo, EvolutionTree, PokemonDetail, PokemonEntry, Sprite};
use crate::query::Query;
use crate::team;

/// Messages sent from background fetch tasks to the UI loop. The payloads are
/// large but short-lived and low-frequency, so the size difference between
/// variants isn't worth boxing around.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Message {
    /// The master Pokemon list finished loading.
    ListLoaded(Vec<PokemonEntry>),
    /// A Pokemon's details and evolution chain finished loading.
    PokemonLoaded {
        detail: PokemonDetail,
        evolution: EvolutionTree,
        /// Decoded artwork, if the species had a sprite we could fetch.
        sprite: Option<Sprite>,
    },
    /// A standalone sprite (for an evolution-chain member) finished loading.
    SpriteLoaded {
        name: String,
        sprite: Option<Sprite>,
    },
    /// An ability's localized text finished loading.
    AbilityLoaded(AbilityInfo),
    /// The roster for a `type:` filter finished loading.
    TypeMembersLoaded {
        type_name: String,
        members: Vec<String>,
    },
    /// A machine-translated flavor blurb finished loading.
    FlavorTranslated {
        name: String,
        lang: String,
        text: String,
    },
    /// A background task failed.
    Error(String),
}

/// Which panel currently receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Search,
    List,
    /// The evolution panel: arrow keys move between chain members and Enter
    /// jumps to the highlighted one.
    Evolution,
}

/// How the sidebar orders whatever survived the filter.
///
/// Both keys are derived from data the list response already carries, so
/// sorting never costs a request. Ordering by base-stat total would: it needs
/// every species' stats, which is 1300 fetches for one keypress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// National Pokedex order — PokeAPI's own, and the default.
    Dex,
    /// Alphabetical by name.
    Name,
}

impl SortKey {
    /// The next key in the cycle, for the sort hotkey.
    pub fn next(self) -> Self {
        match self {
            SortKey::Dex => SortKey::Name,
            SortKey::Name => SortKey::Dex,
        }
    }
}

/// The complete, observable state of the running application.
pub struct App {
    pub language: Language,
    pub all_pokemon: Vec<PokemonEntry>,
    /// Indices into `all_pokemon` that match the current search query.
    pub filtered: Vec<usize>,
    pub list_state: ListState,
    /// Raw contents of the search box, exactly as typed.
    pub query: String,
    /// `query` after parsing, kept so the renderer can describe the active
    /// filter without re-parsing on every frame.
    pub parsed_query: Query,
    pub sort: SortKey,
    /// Rosters for `type:` filters, keyed by type name. An entry that is
    /// present but empty means "we asked and got nothing back".
    pub type_members: HashMap<String, HashSet<String>>,
    /// Type rosters currently in flight, so a filter is requested only once.
    pub type_loading: HashSet<String>,
    pub focus: Focus,
    /// In-memory cache so each Pokemon is fetched at most once per session.
    pub details: HashMap<String, PokemonDetail>,
    pub evolutions: HashMap<String, EvolutionTree>,
    /// Decoded sprites, keyed by Pokemon name. Absent if a species has no art.
    pub sprites: HashMap<String, Sprite>,
    /// Names whose sprite is being fetched on demand for the evolution panel,
    /// so we never queue the same request twice.
    pub sprite_loading: HashSet<String>,
    /// Cursor into the evolution chain (depth-first order) while the evolution
    /// panel is focused.
    pub evo_cursor: usize,
    /// Whether the language-picker card is open, and which row it highlights.
    pub language_picker: bool,
    pub lang_cursor: usize,
    /// Whether the type-matchup card is open for the current selection.
    pub matchups: bool,
    /// The party being assembled, in the order members were added. Holds names
    /// only; the analysis reads their typings out of `details`, so a member
    /// whose record is still in flight simply does not contribute yet.
    pub team: Vec<String>,
    /// Team members whose details are being fetched, so each is requested once.
    pub team_loading: HashSet<String>,
    /// Whether the team card is open.
    pub team_card: bool,
    /// Localized ability text, keyed by ability slug.
    pub abilities: HashMap<String, AbilityInfo>,
    /// Ability lookups in flight, so each is requested only once.
    pub ability_loading: HashSet<String>,
    /// Whether the ability card is open for the current selection.
    pub ability_card: bool,
    /// Machine-translated flavor blurbs, keyed by `(pokemon name, lang code)`.
    pub translations: HashMap<(String, String), String>,
    /// Translation requests currently in flight, to avoid duplicating work.
    pub translating: HashSet<(String, String)>,
    /// Name of the Pokemon currently shown in the detail panel.
    pub selected_name: Option<String>,
    /// Name currently being fetched, if any (drives the detail spinner).
    pub loading_detail: Option<String>,
    pub list_loading: bool,
    pub error: Option<String>,
    /// Monotonic counter used to animate the loading spinner.
    pub spinner: usize,
    pub should_quit: bool,

    client: reqwest::Client,
    tx: mpsc::Sender<Message>,
}

impl App {
    /// Builds the app and returns it alongside the receiver half of the
    /// message channel (handed back to [`App::run`]).
    pub fn new() -> anyhow::Result<(Self, mpsc::Receiver<Message>)> {
        let client = api::build_client()?;
        let (tx, rx) = mpsc::channel(64);
        let app = App {
            language: Language::English,
            all_pokemon: Vec::new(),
            filtered: Vec::new(),
            list_state: ListState::default(),
            query: String::new(),
            parsed_query: Query::default(),
            sort: SortKey::Dex,
            type_members: HashMap::new(),
            type_loading: HashSet::new(),
            focus: Focus::List,
            details: HashMap::new(),
            evolutions: HashMap::new(),
            sprites: HashMap::new(),
            sprite_loading: HashSet::new(),
            evo_cursor: 0,
            language_picker: false,
            lang_cursor: 0,
            matchups: false,
            team: Vec::new(),
            team_loading: HashSet::new(),
            team_card: false,
            abilities: HashMap::new(),
            ability_loading: HashSet::new(),
            ability_card: false,
            translations: HashMap::new(),
            translating: HashSet::new(),
            selected_name: None,
            loading_detail: None,
            list_loading: false,
            error: None,
            spinner: 0,
            should_quit: false,
            client,
            tx,
        };
        Ok((app, rx))
    }

    /// The main event loop. Owns the terminal and runs until the user quits.
    pub async fn run(
        mut self,
        mut terminal: DefaultTerminal,
        mut rx: mpsc::Receiver<Message>,
    ) -> anyhow::Result<()> {
        self.fetch_list();

        let mut events = EventStream::new();
        let mut ticker = tokio::time::interval(Duration::from_millis(120));

        while !self.should_quit {
            // Cheap, idempotent: requests a translation only when the current
            // selection+language needs one and none is cached or in flight.
            self.ensure_translation();
            self.ensure_ability_info();
            terminal.draw(|frame| crate::ui::render(frame, &mut self))?;

            tokio::select! {
                maybe_msg = rx.recv() => {
                    if let Some(msg) = maybe_msg {
                        self.handle_message(msg);
                    }
                }
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(event)) => self.handle_event(event),
                        Some(Err(_)) => {} // transient read error: ignore and redraw
                        None => self.should_quit = true,
                    }
                }
                _ = ticker.tick() => {
                    self.spinner = self.spinner.wrapping_add(1);
                }
            }
        }
        Ok(())
    }

    // --- Async fetch dispatch --------------------------------------------
    //
    // Every dispatcher below reads through `cache` before it touches the
    // network, and writes back whatever it had to fetch. The functions doing
    // that live at the bottom of this file: they run on spawned tasks and so
    // cannot borrow `self`.

    /// Loads the sidebar list, preferring the cache so the app has something to
    /// show immediately. A cached-but-expired list is displayed first and then
    /// refreshed in place; if that refresh fails (offline, say) the stale copy
    /// simply stays up, which is far more useful than an error banner.
    fn fetch_list(&mut self) {
        self.list_loading = true;
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            if let Some(cached) = cache::load_list().await {
                let fresh = cached.fresh;
                let _ = tx.send(Message::ListLoaded(cached.entries)).await;
                if fresh {
                    return;
                }
                if let Ok(list) = api::fetch_pokemon_list(&client).await {
                    cache::store_list(&list).await;
                    let _ = tx.send(Message::ListLoaded(list)).await;
                }
                return;
            }
            let msg = match api::fetch_pokemon_list(&client).await {
                Ok(list) => {
                    cache::store_list(&list).await;
                    Message::ListLoaded(list)
                }
                Err(err) => Message::Error(err.to_string()),
            };
            let _ = tx.send(msg).await;
        });
    }

    /// Kicks off a roster fetch for every `type:` term we have not resolved
    /// yet. One request answers a whole type, and the answer is cached on disk,
    /// so this fires at most once per type per install.
    fn request_missing_type_rosters(&mut self, query: &Query) {
        let missing: Vec<String> = query
            .types
            .iter()
            .filter(|t| !self.type_members.contains_key(*t) && !self.type_loading.contains(*t))
            .cloned()
            .collect();

        for type_name in missing {
            self.type_loading.insert(type_name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                let members = resolve_type_members(&client, &type_name).await;
                let _ = tx.send(Message::TypeMembersLoaded { type_name, members }).await;
            });
        }
    }

    /// Loads (or reveals from cache) the currently highlighted Pokemon.
    fn request_selected(&mut self) {
        let Some(name) = self.current_name() else {
            return;
        };
        self.error = None;
        self.selected_name = Some(name.clone());

        // Cache hit: nothing to fetch, but make sure the chain sprites are on
        // their way (they may not have been requested yet).
        if self.details.contains_key(&name) {
            self.loading_detail = None;
            self.ensure_chain_sprites();
            return;
        }

        self.loading_detail = Some(name.clone());
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = tx.send(resolve_bundle(&client, &name).await).await;
        });
    }

    /// Requests a machine translation of the selected Pokemon's flavor text when
    /// the active language has no native PokeAPI entry (e.g. Turkish) and we
    /// haven't already translated or queued it.
    fn ensure_translation(&mut self) {
        let code = self.language.flavor_code();
        if code == "en" {
            return; // English is always the source; nothing to translate
        }
        // Gather what we need under a short immutable borrow, then release it.
        let (name, source) = {
            let Some(detail) = self.selected_detail() else {
                return;
            };
            if detail.flavors.contains_key(code) {
                return; // PokeAPI already has this language natively
            }
            match detail.flavors.get("en") {
                Some(src) => (detail.name.clone(), src.clone()),
                None => return, // no English source to translate from
            }
        };

        let key = (name.clone(), code.to_string());
        if self.translations.contains_key(&key) || self.translating.contains(&key) {
            return;
        }
        self.translating.insert(key);

        let tx = self.tx.clone();
        let client = self.client.clone();
        let lang = code.to_string();
        tokio::spawn(async move {
            // Translations cost a rate-limited third-party request, so a cached
            // one is worth reaching for before we ask again.
            if let Some(text) = cache::load_translation(&name, &lang).await {
                let _ = tx.send(Message::FlavorTranslated { name, lang, text }).await;
                return;
            }
            // On failure we simply never send: the UI keeps the English text and
            // the in-flight flag stops us from hammering a rate-limited service.
            if let Ok(text) = api::translate_text(&client, &source, "en", &lang).await {
                cache::store_translation(&name, &lang, &text).await;
                let _ = tx.send(Message::FlavorTranslated { name, lang, text }).await;
            }
        });
    }

    /// A cached machine translation for `name` in `code`, if one exists.
    pub fn translation_for(&self, name: &str, code: &str) -> Option<&str> {
        self.translations
            .get(&(name.to_string(), code.to_string()))
            .map(String::as_str)
    }

    /// The names in the current evolution chain, depth-first. Empty if no
    /// evolution data is loaded for the selection.
    pub fn chain_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(tree) = self.selected_evolution() {
            tree.collect_names(&mut names);
        }
        names
    }

    /// Kicks off sprite fetches for every member of the current chain that isn't
    /// already cached or in flight, so the evolution panel can show its artwork.
    fn ensure_chain_sprites(&mut self) {
        for name in self.chain_names() {
            if self.sprites.contains_key(&name) || self.sprite_loading.contains(&name) {
                continue;
            }
            self.sprite_loading.insert(name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                // A failed chain sprite is non-fatal: `resolve_named_sprite`
                // reports no art, so the panel shows a placeholder instead of
                // an error banner.
                let sprite = resolve_named_sprite(&client, &name).await;
                let _ = tx.send(Message::SpriteLoaded { name, sprite }).await;
            });
        }
    }

    /// Loads the chain member currently under the evolution cursor — the quick
    /// "jump to my next evolution" action.
    fn jump_to_evolution_member(&mut self) {
        let names = self.chain_names();
        let Some(name) = names.get(self.evo_cursor).cloned() else {
            return;
        };
        // Make sure the target is visible in the list and selected there, so the
        // sidebar stays in sync with the detail panel.
        self.query.clear();
        self.recompute_filter();
        if let Some(abs) = self.all_pokemon.iter().position(|p| p.name == name) {
            if let Some(pos) = self.filtered.iter().position(|&i| i == abs) {
                self.list_state.select(Some(pos));
            }
        }
        self.request_selected();
    }

    // --- Message handling ------------------------------------------------

    fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::ListLoaded(list) => {
                self.all_pokemon = list;
                self.list_loading = false;
                self.recompute_filter();
                // Open on the first species (Bulbasaur) instead of an empty
                // panel, so there is something to look at before any keypress.
                if self.selected_name.is_none() {
                    self.request_selected();
                }
            }
            Message::PokemonLoaded { detail, evolution, sprite } => {
                let name = detail.name.clone();
                if self.loading_detail.as_deref() == Some(name.as_str()) {
                    self.loading_detail = None;
                }
                self.evolutions.insert(name.clone(), evolution);
                if let Some(sprite) = sprite {
                    self.sprites.insert(name.clone(), sprite);
                }
                let is_selected = self.selected_name.as_deref() == Some(name.as_str());
                self.team_loading.remove(&name);
                self.details.insert(name, detail);
                // Now that the chain is known, fetch its members' sprites for
                // the evolution panel.
                if is_selected {
                    self.ensure_chain_sprites();
                }
            }
            Message::SpriteLoaded { name, sprite } => {
                self.sprite_loading.remove(&name);
                if let Some(sprite) = sprite {
                    self.sprites.insert(name, sprite);
                }
            }
            Message::AbilityLoaded(info) => {
                self.ability_loading.remove(&info.name);
                self.abilities.insert(info.name.clone(), info);
            }
            Message::TypeMembersLoaded { type_name, members } => {
                self.type_loading.remove(&type_name);
                // Recorded even when empty — a mistyped type must settle on
                // "no results" instead of being requested again every frame.
                self.type_members.insert(type_name, members.into_iter().collect());
                self.recompute_filter();
            }
            Message::FlavorTranslated { name, lang, text } => {
                let key = (name, lang);
                self.translating.remove(&key);
                self.translations.insert(key, text);
            }
            Message::Error(err) => {
                self.error = Some(err);
                self.loading_detail = None;
                self.list_loading = false;
            }
        }
    }

    // --- Input handling --------------------------------------------------

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else {
            return; // resize/mouse: the next draw already adapts
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        // Ctrl-C always quits, regardless of focus.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        // The overlay cards are modal: whichever is open grabs all input.
        if self.language_picker {
            self.handle_language_key(key);
            return;
        }
        if self.ability_card {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('a' | 'A' | 'q' | 'Q')
            ) {
                self.ability_card = false;
            }
            return;
        }
        if self.team_card {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('p' | 'P' | 'q' | 'Q')
            ) {
                self.team_card = false;
            }
            return;
        }
        if self.matchups {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('t' | 'T' | 'q' | 'Q')
            ) {
                self.matchups = false;
            }
            return;
        }
        match self.focus {
            Focus::List => self.handle_list_key(key),
            Focus::Search => self.handle_search_key(key),
            Focus::Evolution => self.handle_evolution_key(key),
        }
    }

    /// Opens the type-matchup card. It reads the selection's types, so there is
    /// nothing to show until a Pokemon has actually loaded.
    /// Adds the highlighted species to the party, or drops it if it is already
    /// there. A member whose record is not loaded yet is fetched in the
    /// background: the party is picked from the list, where nothing but the
    /// name is known until something asks for more.
    fn toggle_team_membership(&mut self) {
        let Some(name) = self.current_name() else {
            return;
        };
        if let Some(position) = self.team.iter().position(|member| *member == name) {
            self.team.remove(position);
            return;
        }
        if self.team.len() >= team::MAX_MEMBERS {
            return; // party is full; drop someone first
        }
        self.team.push(name.clone());

        if self.details.contains_key(&name) || self.team_loading.contains(&name) {
            return;
        }
        self.team_loading.insert(name.clone());
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = tx.send(resolve_bundle(&client, &name).await).await;
        });
    }

    /// The loaded records for the current party, in party order. Members still
    /// in flight are skipped, so the analysis always describes exactly what is
    /// listed as loaded on the card.
    pub fn team_details(&self) -> Vec<&PokemonDetail> {
        self.team
            .iter()
            .filter_map(|name| self.details.get(name))
            .collect()
    }

    /// Whether the highlighted list entry is in the party, for the list marker.
    pub fn is_in_team(&self, name: &str) -> bool {
        self.team.iter().any(|member| member == name)
    }

    /// Opens the ability card. The text it shows is pulled in by
    /// [`App::ensure_ability_info`], which the loop is already running.
    fn open_abilities(&mut self) {
        if self.selected_detail().is_some() {
            self.ability_card = true;
        }
    }

    /// Requests the localized text for any ability on the current selection or
    /// in the party that we do not have yet.
    ///
    /// Ability *names* are localized too, and they show on the info card and
    /// the party card, not just behind `A` — so waiting for the card to open
    /// would leave those reading as raw English slugs in every other language.
    /// Cheap and idempotent: it only ever covers species the user has actually
    /// opened, each name is requested once, and every answer is cached on disk.
    fn ensure_ability_info(&mut self) {
        // Gather under a short immutable borrow, then release it.
        let missing: Vec<String> = {
            let selection = self.selected_detail().into_iter();
            selection
                .chain(self.team_details())
                .flat_map(|detail| detail.abilities.iter())
                .map(|ability| ability.name.clone())
                .filter(|name| {
                    !self.abilities.contains_key(name) && !self.ability_loading.contains(name)
                })
                .collect()
        };

        for name in missing {
            self.ability_loading.insert(name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                // Text we cannot fetch simply never arrives: everything keeps
                // showing the ability's slug, which is the useful half.
                if let Some(info) = resolve_ability(&client, &name).await {
                    let _ = tx.send(Message::AbilityLoaded(info)).await;
                }
            });
        }
    }

    fn open_matchups(&mut self) {
        if self.selected_detail().is_some() {
            self.matchups = true;
        }
    }

    /// Opens the language picker, parking the cursor on the active language.
    fn open_language_picker(&mut self) {
        self.lang_cursor = self.language.index();
        self.language_picker = true;
    }

    fn handle_language_key(&mut self, key: KeyEvent) {
        let len = Language::ALL.len();
        match key.code {
            KeyCode::Esc => self.language_picker = false,
            KeyCode::Up | KeyCode::Char('k') => {
                self.lang_cursor = (self.lang_cursor + len - 1) % len;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.lang_cursor = (self.lang_cursor + 1) % len;
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.language = Language::ALL[self.lang_cursor];
                self.language_picker = false;
            }
            _ => {}
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Enter => self.request_selected(),
            KeyCode::Char('e') | KeyCode::Char('E') => self.focus_evolution(),
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_matchups(),
            KeyCode::Tab | KeyCode::Char('/') => self.focus = Focus::Search,
            KeyCode::Char('l') | KeyCode::Char('L') => self.open_language_picker(),
            KeyCode::Char('s') | KeyCode::Char('S') => self.cycle_sort(),
            KeyCode::Char(' ') => self.toggle_team_membership(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.team_card = true,
            KeyCode::Char('a') | KeyCode::Char('A') => self.open_abilities(),
            _ => {}
        }
    }

    /// Moves focus into the evolution panel, parking the cursor on the species
    /// currently shown in the detail panel.
    fn focus_evolution(&mut self) {
        let names = self.chain_names();
        if names.is_empty() {
            return; // no chain to navigate yet
        }
        self.evo_cursor = self
            .selected_name
            .as_ref()
            .and_then(|sel| names.iter().position(|n| n == sel))
            .unwrap_or(0);
        self.focus = Focus::Evolution;
    }

    fn handle_evolution_key(&mut self, key: KeyEvent) {
        let len = self.chain_names().len();
        match key.code {
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::List,
            KeyCode::Char('q') | KeyCode::Char('Q') => self.should_quit = true,
            KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k')
                if self.evo_cursor > 0 =>
            {
                self.evo_cursor -= 1;
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Char('l') | KeyCode::Char('j')
                if self.evo_cursor + 1 < len =>
            {
                self.evo_cursor += 1;
            }
            KeyCode::Enter => self.jump_to_evolution_member(),
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_matchups(),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::List,
            KeyCode::Enter => {
                self.request_selected();
                self.focus = Focus::List;
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Backspace => {
                self.query.pop();
                self.recompute_filter();
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.recompute_filter();
            }
            _ => {}
        }
    }

    // --- List / filter helpers -------------------------------------------

    /// Rebuilds the visible list from the search box and the sort key.
    ///
    /// Called after anything that can change either, and cheap enough to run on
    /// every keystroke: the work is one pass over ~1300 entries plus a sort.
    fn recompute_filter(&mut self) {
        let query = Query::parse(&self.query);
        self.request_missing_type_rosters(&query);

        // Remember what was highlighted so the same Pokemon stays under the
        // cursor when the list is merely re-sorted, or when it survives a
        // narrowing search. Losing the highlight on every keystroke is the
        // main thing that makes a filtered list annoying to use.
        let anchor = self.current_name();

        let mut filtered: Vec<usize> = self
            .all_pokemon
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                query.matches_name_and_generation(&p.name, p.generation())
                    && self.has_every_type(&query, &p.name)
            })
            .map(|(idx, _)| idx)
            .collect();

        match self.sort {
            SortKey::Dex => filtered.sort_unstable_by_key(|&idx| self.all_pokemon[idx].id),
            SortKey::Name => {
                filtered.sort_unstable_by(|&a, &b| {
                    self.all_pokemon[a].name.cmp(&self.all_pokemon[b].name)
                });
            }
        }

        self.filtered = filtered;
        self.parsed_query = query;
        self.restore_highlight(anchor);
    }

    /// Whether `name` is in the roster of every type the query asks for.
    /// A roster we do not have yet matches nothing, which leaves the list empty
    /// until it lands — the sidebar says as much while that is true.
    fn has_every_type(&self, query: &Query, name: &str) -> bool {
        query.types.iter().all(|type_name| {
            self.type_members
                .get(type_name)
                .is_some_and(|members| members.contains(name))
        })
    }

    /// Puts the cursor back on `anchor` if it is still visible, and on the
    /// first row otherwise.
    fn restore_highlight(&mut self, anchor: Option<String>) {
        if self.filtered.is_empty() {
            self.list_state.select(None);
            return;
        }
        let restored = anchor
            .and_then(|name| self.all_pokemon.iter().position(|p| p.name == name))
            .and_then(|abs| self.filtered.iter().position(|&idx| idx == abs));
        self.list_state.select(Some(restored.unwrap_or(0)));
    }

    /// True while a `type:` filter is still waiting on its roster, so the
    /// sidebar can say "loading" rather than "no results".
    pub fn awaiting_type_roster(&self) -> bool {
        self.parsed_query
            .types
            .iter()
            .any(|type_name| !self.type_members.contains_key(type_name))
    }

    fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.recompute_filter();
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len() as i32;
        let current = self.list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len);
        self.list_state.select(Some(next as usize));
    }

    /// Raw API name of the highlighted list entry, if any.
    pub fn current_name(&self) -> Option<String> {
        let selected = self.list_state.selected()?;
        let idx = *self.filtered.get(selected)?;
        self.all_pokemon.get(idx).map(|p| p.name.clone())
    }

    /// Detail record for the panel, if the selection is loaded.
    pub fn selected_detail(&self) -> Option<&PokemonDetail> {
        let name = self.selected_name.as_ref()?;
        self.details.get(name)
    }

    /// Evolution tree for the selected Pokemon, if loaded.
    pub fn selected_evolution(&self) -> Option<&EvolutionTree> {
        let name = self.selected_name.as_ref()?;
        self.evolutions.get(name)
    }

    /// Decoded sprite for the selected Pokemon, if one was loaded.
    pub fn selected_sprite(&self) -> Option<&Sprite> {
        let name = self.selected_name.as_ref()?;
        self.sprites.get(name)
    }

    /// True while the detail panel is awaiting its current selection.
    pub fn detail_is_loading(&self) -> bool {
        match (&self.loading_detail, &self.selected_name) {
            (Some(loading), Some(selected)) => loading == selected,
            _ => false,
        }
    }
}

// --- Cache-first resolvers -----------------------------------------------
//
// These run on spawned tasks, so they take everything they need by value or
// shared reference rather than borrowing `App`.

/// Resolves a species from the cache, falling back to the network and storing
/// whatever it had to fetch.
async fn resolve_bundle(client: &reqwest::Client, name: &str) -> Message {
    if let Some(bundle) = cache::load_bundle(name).await {
        let sprite = resolve_sprite(client, name, bundle.detail.sprite_url.as_deref()).await;
        return Message::PokemonLoaded {
            detail: bundle.detail,
            evolution: bundle.evolution,
            sprite,
        };
    }
    match api::fetch_pokemon_bundle(client, name).await {
        Ok((detail, evolution, sprite)) => {
            cache::store_bundle(name, &detail, &evolution).await;
            record_sprite(name, sprite.as_ref()).await;
            Message::PokemonLoaded { detail, evolution, sprite }
        }
        Err(err) => Message::Error(err.to_string()),
    }
}

/// Cache-first sprite lookup for a species whose artwork URL we already know
/// (because its details came out of the cache alongside it).
async fn resolve_sprite(
    client: &reqwest::Client,
    name: &str,
    url: Option<&str>,
) -> Option<Sprite> {
    if let Some(sprite) = cache::load_sprite(name).await {
        return Some(sprite);
    }
    if cache::has_sprite_answer(name).await {
        return None; // asked before: this species genuinely has no artwork
    }
    let sprite = api::fetch_sprite(client, url?).await.ok();
    record_sprite(name, sprite.as_ref()).await;
    sprite
}

/// Cache-first sprite lookup for a chain member we know nothing else about.
/// Only the network path has to resolve the artwork URL first.
async fn resolve_named_sprite(client: &reqwest::Client, name: &str) -> Option<Sprite> {
    if let Some(sprite) = cache::load_sprite(name).await {
        return Some(sprite);
    }
    if cache::has_sprite_answer(name).await {
        return None;
    }
    match api::fetch_named_sprite(client, name).await {
        Ok(sprite) => {
            record_sprite(name, sprite.as_ref()).await;
            sprite
        }
        // A transient failure must not be written down as "no artwork", or the
        // species would stay blank for as long as the cache lives.
        Err(_) => None,
    }
}

/// Resolves one ability's localized text, cache first.
async fn resolve_ability(client: &reqwest::Client, name: &str) -> Option<AbilityInfo> {
    if let Some(info) = cache::load_ability(name).await {
        return Some(info);
    }
    let info = api::fetch_ability(client, name).await.ok()?;
    cache::store_ability(name, &info).await;
    Some(info)
}

/// Resolves a type's roster from the cache, falling back to the network.
///
/// A failure yields an empty roster rather than an error: the only way to get
/// here is a `type:` term, and the honest answer to a type we cannot resolve
/// is that nothing matches it.
async fn resolve_type_members(client: &reqwest::Client, type_name: &str) -> Vec<String> {
    if let Some(members) = cache::load_type_members(type_name).await {
        return members;
    }
    match api::fetch_type_members(client, type_name).await {
        Ok(members) => {
            cache::store_type_members(type_name, &members).await;
            members
        }
        Err(_) => Vec::new(),
    }
}

/// Stores a freshly fetched sprite, or the fact that there wasn't one, so the
/// next run does not repeat the request either way.
async fn record_sprite(name: &str, sprite: Option<&Sprite>) {
    match sprite {
        Some(sprite) => cache::store_sprite(name, sprite).await,
        None => cache::store_missing_sprite(name).await,
    }
}
