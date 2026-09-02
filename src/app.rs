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
use tokio::time::MissedTickBehavior;

use crate::api;
use crate::api::ApiError;
use crate::cache;
use crate::cli::Startup;
use crate::color::{self, Depth};
use crate::i18n::Language;
use crate::models::{
    AbilityInfo, EvolutionTree, LearnedMove, MoveInfo, PokemonDetail, PokemonEntry, RosterTerm,
    Sprite, SpriteVariant,
};
use crate::query::Query;
use crate::session::{self, Session};
use crate::team;

/// How many learnset rows around the cursor the moves card fetches records for.
/// Sized to cover the card on a tall terminal, so the visible table fills in
/// together rather than a row at a time.
const MOVE_BAND: usize = 36;
/// How much of [`MOVE_BAND`] sits above the cursor rather than below it.
const MOVE_LOOKBEHIND: usize = 4;

/// How often the loading spinner advances, while there is one to advance.
const SPINNER_TICK: Duration = Duration::from_millis(120);

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
        /// Which palette `sprite` was fetched in. Carried along because the
        /// shiny toggle can flip while the request is in flight.
        variant: SpriteVariant,
    },
    /// A standalone sprite (for an evolution-chain member) finished loading.
    SpriteLoaded {
        name: String,
        variant: SpriteVariant,
        sprite: Option<Sprite>,
    },
    /// An ability's localized text finished loading.
    AbilityLoaded(AbilityInfo),
    /// One move's record finished loading.
    MoveLoaded(MoveInfo),
    /// The roster behind a `type:`, `ability:` or `egg:` filter finished
    /// loading.
    RosterLoaded {
        term: RosterTerm,
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

    /// Stable name used to record the ordering in a session file, so that
    /// reordering this enum can never change what a stored session means.
    pub fn code(self) -> &'static str {
        match self {
            SortKey::Dex => "dex",
            SortKey::Name => "name",
        }
    }

    /// The inverse of [`code`](Self::code). An unrecognised value is `None`,
    /// and the caller keeps the default ordering.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "dex" => Some(SortKey::Dex),
            "name" => Some(SortKey::Name),
            _ => None,
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
    /// Membership lists for the filter terms asked for so far. An entry that is
    /// present but empty means "we asked and got nothing back".
    pub rosters: HashMap<RosterTerm, HashSet<String>>,
    /// Rosters currently in flight, so a filter is requested only once.
    pub roster_loading: HashSet<RosterTerm>,
    pub focus: Focus,
    /// In-memory cache so each Pokemon is fetched at most once per session.
    pub details: HashMap<String, PokemonDetail>,
    pub evolutions: HashMap<String, EvolutionTree>,
    /// Decoded sprites, keyed by palette and then by Pokemon name. Absent if a
    /// species has no art, or if that palette has not been asked for yet.
    pub sprites: HashMap<SpriteVariant, HashMap<String, Sprite>>,
    /// Names whose sprite is being fetched on demand, per palette, so we never
    /// queue the same request twice.
    pub sprite_loading: HashMap<SpriteVariant, HashSet<String>>,
    /// Which palette every sprite on screen is shown in. App-wide rather than
    /// per-species: moving through the list keeps showing shinies until the
    /// toggle is switched off again.
    pub sprite_variant: SpriteVariant,
    /// Cursor into the evolution chain (depth-first order) while the evolution
    /// panel is focused.
    pub evo_cursor: usize,
    /// Whether the chain is expanded to the full-screen evolution view. Wide
    /// branching chains never fit the panel, so this is where they get drawn as
    /// sprite cards rather than as a text tree.
    pub evo_card: bool,
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
    /// Move records, keyed by move slug. Filled in one move at a time as the
    /// cursor reaches each row, rather than eighty at a time when the card
    /// opens.
    pub moves: HashMap<String, MoveInfo>,
    /// Move lookups in flight, so each is requested only once.
    pub move_loading: HashSet<String>,
    /// Whether the moves card is open for the current selection.
    pub moves_card: bool,
    /// Row the moves card highlights, as an index into the selection's
    /// learnset.
    pub move_cursor: usize,
    /// Whether the help overlay is open.
    pub help_card: bool,
    /// Machine-translated flavor blurbs, keyed by `(pokemon name, lang code)`.
    pub translations: HashMap<(String, String), String>,
    /// Translation requests currently in flight, to avoid duplicating work.
    pub translating: HashSet<(String, String)>,
    /// Name of the Pokemon currently shown in the detail panel.
    pub selected_name: Option<String>,
    /// What the terminal can show, resolved once at startup from `--color` and
    /// the environment. Every frame is rewritten into it on the way out, and
    /// sprites are skipped entirely when it is [`Depth::None`].
    pub color_depth: Depth,
    /// Language named on the command line, if any. Kept rather than merely
    /// applied because the restored session carries a language too, and this
    /// one has to outrank it.
    cli_language: Option<Language>,
    /// Species named on the command line, opened once the list arrives — the
    /// first moment there is anything to resolve a name against.
    startup_species: Option<String>,
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
    ///
    /// `startup` carries the command-line overrides. Each is optional, so a
    /// bare invocation is the same call with nothing to override.
    pub fn new(startup: Startup) -> anyhow::Result<(Self, mpsc::Receiver<Message>)> {
        let client = api::build_client()?;
        let color_depth = color::resolve(startup.color, &color::Env::from_process());
        color::enforce(startup.color, color_depth);
        let (tx, rx) = mpsc::channel(64);
        let app = App {
            language: startup.language.unwrap_or(Language::English),
            all_pokemon: Vec::new(),
            filtered: Vec::new(),
            list_state: ListState::default(),
            query: String::new(),
            parsed_query: Query::default(),
            sort: SortKey::Dex,
            rosters: HashMap::new(),
            roster_loading: HashSet::new(),
            focus: Focus::List,
            details: HashMap::new(),
            evolutions: HashMap::new(),
            sprites: HashMap::new(),
            sprite_loading: HashMap::new(),
            sprite_variant: SpriteVariant::Normal,
            evo_cursor: 0,
            evo_card: false,
            language_picker: false,
            lang_cursor: 0,
            matchups: false,
            team: Vec::new(),
            team_loading: HashSet::new(),
            team_card: false,
            abilities: HashMap::new(),
            ability_loading: HashSet::new(),
            ability_card: false,
            moves: HashMap::new(),
            move_loading: HashSet::new(),
            moves_card: false,
            move_cursor: 0,
            help_card: false,
            translations: HashMap::new(),
            translating: HashSet::new(),
            selected_name: None,
            color_depth,
            cli_language: startup.language,
            startup_species: startup.species,
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
        // Before the first frame, so the restored language and palette are
        // already in place by the time anything is drawn or fetched.
        self.restore(session::load().await);
        self.fetch_list();

        let mut events = EventStream::new();
        let mut ticker = tokio::time::interval(SPINNER_TICK);
        // Nothing polls the ticker while the app is idle, so by the time it is
        // wanted again its deadline is far in the past. `Burst` — the default —
        // would answer that by firing every tick it missed back to back,
        // spinning the wheel through a whole sleep's worth of frames the moment
        // a request starts. `Delay` fires once and schedules the next a full
        // period out, which is what makes waking up look like starting rather
        // than catching up.
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        while !self.should_quit {
            // Cheap, idempotent: requests a translation only when the current
            // selection+language needs one and none is cached or in flight.
            self.ensure_translation();
            self.ensure_ability_info();
            self.ensure_move_info();
            // Rendering always writes 24-bit colour; this is where the frame
            // is rewritten into what the terminal can actually show. Doing it
            // over the finished buffer keeps every widget — and every sprite
            // pixel — degrading by one rule instead of each checking for
            // itself.
            let depth = self.color_depth;
            terminal.draw(|frame| {
                crate::ui::render(frame, &mut self);
                color::degrade(frame.buffer_mut(), depth);
            })?;

            // The ticker exists to animate the spinner, and the spinner is
            // only on screen while something is in flight. Selecting on it
            // unconditionally is what made an idle Pokedex — the kind of thing
            // left open in a split for hours — redraw itself eight times a
            // second forever. Idle, the loop now blocks on input and messages
            // alone, and draws when one of them says something changed.
            let animating = self.is_busy();

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
                _ = ticker.tick(), if animating => {
                    self.spinner = self.spinner.wrapping_add(1);
                }
            }
        }

        session::store(&self.snapshot()).await;
        Ok(())
    }

    /// Whether anything is in flight — which is exactly when a spinner is on
    /// screen, and so exactly when there is any reason to redraw on a timer.
    ///
    /// Every field here is the pending set of one kind of request, so "is
    /// anything loading" is the union of them being non-empty. `team_loading`
    /// counts too: a party member's record is fetched without the detail panel
    /// waiting on it, but the party card draws a spinner for it just the same.
    pub fn is_busy(&self) -> bool {
        self.list_loading
            || self.loading_detail.is_some()
            || !self.roster_loading.is_empty()
            || !self.ability_loading.is_empty()
            || !self.move_loading.is_empty()
            || !self.translating.is_empty()
            || !self.team_loading.is_empty()
            || self
                .sprite_loading
                .values()
                .any(|pending| !pending.is_empty())
    }

    // --- Session persistence ---------------------------------------------

    /// What this run hands to the next one.
    fn snapshot(&self) -> Session {
        Session {
            team: self.team.clone(),
            language: Some(self.language.flavor_code().to_string()),
            sort: Some(self.sort.code().to_string()),
            shiny: self.sprite_variant.is_shiny(),
        }
    }

    /// Applies a restored session over the defaults [`App::new`] set.
    ///
    /// Anything the file leaves out, or records in terms this build no longer
    /// recognises, keeps its default rather than rejecting the file: a session
    /// is a convenience, and a partly understood one still beats starting over.
    fn restore(&mut self, session: Session) {
        // `--lang` is an explicit choice made for this run, so it outranks the
        // one carried over from the last. What the run *ends* in is still what
        // gets stored on the way out, which makes the flag behave exactly like
        // opening the picker and choosing that language would.
        if self.cli_language.is_none() {
            if let Some(language) = session.language.as_deref().and_then(Language::from_code) {
                self.language = language;
            }
        }
        if let Some(sort) = session.sort.as_deref().and_then(SortKey::from_code) {
            self.sort = sort;
        }
        if session.shiny {
            self.sprite_variant = SpriteVariant::Shiny;
        }
        self.team = session.team;

        // The party card reads typings out of `details`, which is empty on a
        // cold start, so a restored member contributes nothing to the analysis
        // until its record is back. These are cache hits on any install that
        // has seen the member before, which is every install that stored it.
        for name in self.team.clone() {
            self.request_team_member(name);
        }
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

    /// Kicks off a roster fetch for every filter term we have not resolved yet.
    /// One request answers a whole term, and the answer is cached on disk, so
    /// this fires at most once per term per install.
    fn request_missing_rosters(&mut self, query: &Query) {
        let missing: Vec<RosterTerm> = query
            .rosters
            .iter()
            .filter(|t| !self.rosters.contains_key(*t) && !self.roster_loading.contains(*t))
            .cloned()
            .collect();

        for term in missing {
            self.roster_loading.insert(term.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                let members = resolve_roster(&client, &term).await;
                let _ = tx.send(Message::RosterLoaded { term, members }).await;
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
            self.ensure_visible_sprites();
            return;
        }

        self.loading_detail = Some(name.clone());
        let tx = self.tx.clone();
        let client = self.client.clone();
        let variant = self.sprite_variant;
        tokio::spawn(async move {
            let _ = tx.send(resolve_bundle(&client, &name, variant).await).await;
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
                let _ = tx
                    .send(Message::FlavorTranslated { name, lang, text })
                    .await;
                return;
            }
            // On failure we simply never send: the UI keeps the English text and
            // the in-flight flag stops us from hammering a rate-limited service.
            if let Ok(text) = api::translate_text(&client, &source, "en", &lang).await {
                cache::store_translation(&name, &lang, &text).await;
                let _ = tx
                    .send(Message::FlavorTranslated { name, lang, text })
                    .await;
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

    /// Kicks off sprite fetches for everything on screen — the selected species
    /// and every member of its chain — that isn't already cached or in flight.
    ///
    /// Only the palette currently on display is ever requested, so flipping the
    /// shiny toggle costs the artwork in front of you rather than pre-fetching
    /// two full sets. The selection is listed separately from the chain because
    /// an alternate form (`raichu-alola`) does not appear in it under its own
    /// name — the chain carries the base species.
    fn ensure_visible_sprites(&mut self) {
        // Nothing on screen will show them, and `sprite_for` says as much, so
        // without this the loop below would re-request every chain member on
        // every frame and never be satisfied. The artwork arriving inside a
        // species bundle is still kept: it costs no request of its own, and it
        // means a later run with colour opens instantly.
        if self.color_depth == Depth::None {
            return;
        }
        let variant = self.sprite_variant;
        let names: Vec<String> = self
            .selected_name
            .iter()
            .cloned()
            .chain(self.chain_names())
            .collect();

        for name in names {
            if self.sprite_for(&name).is_some() || self.sprite_is_loading(&name) {
                continue;
            }
            // A species whose record is already loaded carries both artwork
            // URLs, which saves the resolver a `/pokemon` request. `Some(None)`
            // means we know it has no art at all.
            let known_url = self
                .details
                .get(&name)
                .map(|detail| detail.sprite_url_for(variant).map(str::to_string));

            self.sprite_loading
                .entry(variant)
                .or_default()
                .insert(name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                // A failed sprite is non-fatal: the resolvers report no art, so
                // the panel shows a placeholder instead of an error banner.
                let sprite = match known_url {
                    Some(url) => resolve_sprite(&client, &name, url.as_deref(), variant).await,
                    None => resolve_named_sprite(&client, &name, variant).await,
                };
                let _ = tx
                    .send(Message::SpriteLoaded {
                        name,
                        variant,
                        sprite,
                    })
                    .await;
            });
        }
    }

    /// Flips between the normal and shiny palettes, then pulls in whatever
    /// artwork the new one is missing.
    fn toggle_shiny(&mut self) {
        self.sprite_variant = self.sprite_variant.toggled();
        self.ensure_visible_sprites();
    }

    /// Decoded artwork for `name` in the palette currently on display.
    pub fn sprite_for(&self, name: &str) -> Option<&Sprite> {
        // A sprite drawn without colour is a rectangle of identical blocks,
        // which says less than the placeholder the panels already fall back
        // to. Answering `None` here is what routes both of them to it.
        if self.color_depth == Depth::None {
            return None;
        }
        self.sprites.get(&self.sprite_variant)?.get(name)
    }

    /// Whether `name`'s artwork in the current palette is still in flight.
    pub fn sprite_is_loading(&self, name: &str) -> bool {
        self.sprite_loading
            .get(&self.sprite_variant)
            .is_some_and(|pending| pending.contains(name))
    }

    fn remember_sprite(&mut self, name: String, variant: SpriteVariant, sprite: Sprite) {
        self.sprites
            .entry(variant)
            .or_default()
            .insert(name, sprite);
    }

    /// Puts the species named on the command line under the cursor, down the
    /// same path a search-box query takes: the name goes into the box and the
    /// list narrows to it. That is what makes `pokeductor 25` and
    /// `pokeductor type:ghost` work without a second parser, and what keeps an
    /// unknown name on the one "no results" path the TUI already has rather
    /// than inventing a command-line error beside it. The query stays in the
    /// box, so a list that narrowed to nothing says why it did.
    ///
    /// The one thing the search box cannot do here is prefer an exact match:
    /// `mew` narrows to Mew and Mewtwo, and dex order puts Mewtwo first. That
    /// is right when a human is about to press `↓`, and wrong as the answer to
    /// `pokeductor mew`, so an exact name takes the cursor.
    fn select_named_species(&mut self, name: String) {
        self.query = name.trim().to_lowercase();
        self.recompute_filter();
        let exact = self
            .filtered
            .iter()
            .position(|&idx| self.all_pokemon[idx].name == self.query);
        if let Some(pos) = exact {
            self.list_state.select(Some(pos));
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
                // A species named on the command line goes through the
                // search box, which is what narrows the list to it. With no
                // argument the box is empty and the filter is the whole list.
                if let Some(name) = self.startup_species.take() {
                    self.select_named_species(name);
                } else {
                    self.recompute_filter();
                }
                // Open on whatever ended up under the cursor — the argument's
                // species, or the first entry (Bulbasaur) — instead of an empty
                // panel, so there is something to look at before any keypress.
                if self.selected_name.is_none() {
                    self.request_selected();
                }
            }
            Message::PokemonLoaded {
                detail,
                evolution,
                sprite,
                variant,
            } => {
                let name = detail.name.clone();
                if self.loading_detail.as_deref() == Some(name.as_str()) {
                    self.loading_detail = None;
                }
                self.evolutions.insert(name.clone(), evolution);
                if let Some(sprite) = sprite {
                    self.remember_sprite(name.clone(), variant, sprite);
                }
                let is_selected = self.selected_name.as_deref() == Some(name.as_str());
                self.team_loading.remove(&name);
                self.details.insert(name, detail);
                // Now that the chain is known, fetch its members' sprites for
                // the evolution panel. This also covers a toggle that happened
                // while the bundle was in flight: the palette it arrived in may
                // no longer be the one on screen.
                if is_selected {
                    self.ensure_visible_sprites();
                }
            }
            Message::SpriteLoaded {
                name,
                variant,
                sprite,
            } => {
                if let Some(pending) = self.sprite_loading.get_mut(&variant) {
                    pending.remove(&name);
                }
                if let Some(sprite) = sprite {
                    self.remember_sprite(name, variant, sprite);
                }
            }
            Message::AbilityLoaded(info) => {
                self.ability_loading.remove(&info.name);
                self.abilities.insert(info.name.clone(), info);
            }
            Message::MoveLoaded(info) => {
                self.move_loading.remove(&info.name);
                self.moves.insert(info.name.clone(), info);
            }
            Message::RosterLoaded { term, members } => {
                self.roster_loading.remove(&term);
                // Recorded even when empty — a mistyped term must settle on
                // "no results" instead of being requested again every frame.
                self.rosters.insert(term, members.into_iter().collect());
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
        if self.help_card {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?' | 'q' | 'Q')
            ) {
                self.help_card = false;
            }
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
        if self.moves_card {
            self.handle_moves_key(key);
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
        if self.evo_card {
            self.handle_evo_card_key(key);
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
        self.request_team_member(name);
    }

    /// Pulls in a party member's record in the background, unless it is
    /// already loaded or in flight.
    fn request_team_member(&mut self, name: String) {
        if self.details.contains_key(&name) || self.team_loading.contains(&name) {
            return;
        }
        self.team_loading.insert(name.clone());
        let tx = self.tx.clone();
        let client = self.client.clone();
        let variant = self.sprite_variant;
        tokio::spawn(async move {
            let _ = tx.send(resolve_bundle(&client, &name, variant).await).await;
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

    /// Opens the moves card. The learnset came with the species record, so
    /// there is nothing to wait for; the per-move numbers are pulled in by
    /// [`App::ensure_move_info`] as the cursor reaches each row.
    fn open_moves(&mut self) {
        if self.selected_learnset().is_some_and(|set| !set.is_empty()) {
            self.moves_card = true;
            self.move_cursor = 0;
        }
    }

    /// The learnset of the species currently in the detail panel.
    pub fn selected_learnset(&self) -> Option<&[LearnedMove]> {
        self.selected_detail().map(|detail| detail.moves.as_slice())
    }

    /// The move the card highlights, if the card has anything to highlight.
    pub fn highlighted_move(&self) -> Option<&LearnedMove> {
        self.selected_learnset()?.get(self.move_cursor)
    }

    fn handle_moves_key(&mut self, key: KeyEvent) {
        let len = self.selected_learnset().map_or(0, <[LearnedMove]>::len);
        match key.code {
            KeyCode::Esc | KeyCode::Char('m' | 'M' | 'q' | 'Q') => self.moves_card = false,
            KeyCode::Up | KeyCode::Char('k') => self.move_move_cursor(-1, len),
            KeyCode::Down | KeyCode::Char('j') => self.move_move_cursor(1, len),
            KeyCode::PageUp => self.move_move_cursor(-10, len),
            KeyCode::PageDown => self.move_move_cursor(10, len),
            KeyCode::Home => self.move_cursor = 0,
            KeyCode::End => self.move_cursor = len.saturating_sub(1),
            _ => {}
        }
    }

    /// Moves the card's cursor, clamping at both ends rather than wrapping —
    /// a learnset is one long list, and wrapping off the end of eighty rows
    /// loses the reader's place.
    fn move_move_cursor(&mut self, delta: i32, len: usize) {
        if len == 0 {
            return;
        }
        let next = self.move_cursor as i32 + delta;
        self.move_cursor = next.clamp(0, len as i32 - 1) as usize;
    }

    /// Requests the records for the moves around the card's cursor.
    ///
    /// Only while the card is open, and only for a band around what is on
    /// screen: a full learnset runs past a hundred entries, and fetching all of
    /// them the moment the card opens would spend a hundred requests on rows
    /// most readers never scroll to. A band wide enough to cover the card
    /// leaves the visible table filled in rather than showing a column of
    /// names with the numbers still arriving one row at a time.
    fn ensure_move_info(&mut self) {
        if !self.moves_card {
            return;
        }
        let Some(learnset) = self.selected_learnset() else {
            return;
        };
        // A few rows back as well as forward, so scrolling up finds the same
        // band already warm.
        let start = self.move_cursor.saturating_sub(MOVE_LOOKBEHIND);
        let missing: Vec<String> = learnset
            .iter()
            .skip(start)
            .take(MOVE_BAND)
            .map(|learned| learned.name.clone())
            .filter(|name| !self.moves.contains_key(name) && !self.move_loading.contains(name))
            .collect();

        for name in missing {
            self.move_loading.insert(name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                // A record we cannot fetch simply never arrives: the row keeps
                // showing the move's name and how it is learned, which is the
                // half that came free with the species.
                if let Some(info) = resolve_move(&client, &name).await {
                    let _ = tx.send(Message::MoveLoaded(info)).await;
                }
            });
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
            KeyCode::Char('f') | KeyCode::Char('F') => self.open_evolution_card(),
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_matchups(),
            KeyCode::Tab | KeyCode::Char('/') => self.focus = Focus::Search,
            KeyCode::Char('l') | KeyCode::Char('L') => self.open_language_picker(),
            KeyCode::Char('s') | KeyCode::Char('S') => self.cycle_sort(),
            KeyCode::Char(' ') => self.toggle_team_membership(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.team_card = true,
            KeyCode::Char('a') | KeyCode::Char('A') => self.open_abilities(),
            KeyCode::Char('m') | KeyCode::Char('M') => self.open_moves(),
            KeyCode::Char('x') | KeyCode::Char('X') => self.toggle_shiny(),
            KeyCode::Char('?') => self.help_card = true,
            _ => {}
        }
    }

    /// Moves focus into the evolution panel, parking the cursor on the species
    /// currently shown in the detail panel.
    fn focus_evolution(&mut self) {
        if self.park_evo_cursor() {
            self.focus = Focus::Evolution;
        }
    }

    /// Opens the full-screen evolution view on the current chain. Same cursor
    /// as the panel, so expanding and collapsing never loses the reader's place.
    fn open_evolution_card(&mut self) {
        if self.park_evo_cursor() {
            self.evo_card = true;
        }
    }

    /// Parks the chain cursor on the species in the detail panel, reporting
    /// whether there is a chain to navigate at all.
    fn park_evo_cursor(&mut self) -> bool {
        let names = self.chain_names();
        if names.is_empty() {
            return false; // no chain to navigate yet
        }
        self.evo_cursor = self
            .selected_name
            .as_ref()
            .and_then(|sel| names.iter().position(|n| n == sel))
            .unwrap_or(0);
        true
    }

    /// The full-screen view is the evolution panel with room to breathe, so it
    /// answers to the same keys — `Esc` (or `F` again) collapses it back.
    fn handle_evo_card_key(&mut self, key: KeyEvent) {
        let len = self.chain_names().len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('f' | 'F' | 'q' | 'Q') => self.evo_card = false,
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
            // Taking a stage collapses the view: what you picked is loaded on
            // the panels behind it, and leaving the chain spread over them is
            // the one thing the jump was for.
            KeyCode::Enter => {
                self.jump_to_evolution_member();
                self.evo_card = false;
            }
            KeyCode::Char('x') | KeyCode::Char('X') => self.toggle_shiny(),
            KeyCode::Char('?') => self.help_card = true,
            _ => {}
        }
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
            KeyCode::Char('f') | KeyCode::Char('F') => self.evo_card = true,
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_matchups(),
            KeyCode::Char('x') | KeyCode::Char('X') => self.toggle_shiny(),
            KeyCode::Char('?') => self.help_card = true,
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
        self.request_missing_rosters(&query);

        // Remember what was highlighted so the same Pokemon stays under the
        // cursor when the list is merely re-sorted, or when it survives a
        // narrowing search. Losing the highlight on every keystroke is the
        // main thing that makes a filtered list annoying to use.
        let anchor = self.current_name();

        let mut filtered: Vec<usize> = self
            .all_pokemon
            .iter()
            .enumerate()
            .filter(|(_, p)| query.matches_entry(p) && self.is_in_every_roster(&query, &p.name))
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

    /// Whether `name` is in the roster of every filter term the query asks for.
    /// A roster we do not have yet matches nothing, which leaves the list empty
    /// until it lands — the sidebar says as much while that is true.
    fn is_in_every_roster(&self, query: &Query, name: &str) -> bool {
        query.rosters.iter().all(|term| {
            self.rosters
                .get(term)
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

    /// True while a filter term is still waiting on its roster, so the sidebar
    /// can say "loading" rather than "no results".
    pub fn awaiting_roster(&self) -> bool {
        self.parsed_query
            .rosters
            .iter()
            .any(|term| !self.rosters.contains_key(term))
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

    /// Decoded sprite for the selected Pokemon in the current palette, if one
    /// was loaded.
    pub fn selected_sprite(&self) -> Option<&Sprite> {
        self.sprite_for(self.selected_name.as_deref()?)
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
async fn resolve_bundle(client: &reqwest::Client, name: &str, variant: SpriteVariant) -> Message {
    if let Some(bundle) = cache::load_bundle(name).await {
        let sprite =
            resolve_sprite(client, name, bundle.detail.sprite_url_for(variant), variant).await;
        return Message::PokemonLoaded {
            detail: bundle.detail,
            evolution: bundle.evolution,
            sprite,
            variant,
        };
    }
    match api::fetch_pokemon_bundle(client, name, variant).await {
        Ok((detail, evolution, sprite)) => {
            cache::store_bundle(name, &detail, &evolution).await;
            record_sprite(name, sprite.as_ref(), variant).await;
            Message::PokemonLoaded {
                detail,
                evolution,
                sprite,
                variant,
            }
        }
        Err(err) => Message::Error(err.to_string()),
    }
}

/// Cache-first sprite lookup for a species whose artwork URL we already know
/// (because its details came out of the cache alongside it). `url` is the one
/// for `variant`, so a species with no shiny art resolves its normal sprite —
/// stored under the shiny name, since that is the question we asked.
async fn resolve_sprite(
    client: &reqwest::Client,
    name: &str,
    url: Option<&str>,
    variant: SpriteVariant,
) -> Option<Sprite> {
    if let Some(sprite) = cache::load_sprite(name, variant).await {
        return Some(sprite);
    }
    if cache::has_sprite_answer(name, variant).await {
        return None; // asked before: this species genuinely has no artwork
    }
    let Some(url) = url else {
        // The record itself says there is no artwork in either palette. Write
        // that down so the question is not re-asked on every toggle.
        record_sprite(name, None, variant).await;
        return None;
    };
    let sprite = api::fetch_sprite(client, url).await.ok();
    record_sprite(name, sprite.as_ref(), variant).await;
    sprite
}

/// Cache-first sprite lookup for a chain member we know nothing else about.
/// Only the network path has to resolve the artwork URL first.
async fn resolve_named_sprite(
    client: &reqwest::Client,
    name: &str,
    variant: SpriteVariant,
) -> Option<Sprite> {
    if let Some(sprite) = cache::load_sprite(name, variant).await {
        return Some(sprite);
    }
    if cache::has_sprite_answer(name, variant).await {
        return None;
    }
    // Chain members arrive as species names, which are not always valid
    // `/pokemon` keys — see `resolve_default_variety`. The answer is cached
    // under the species name either way, since that is what the UI asks for.
    let variety = resolve_default_variety(client, name).await?;

    match api::fetch_named_sprite(client, &variety, variant).await {
        Ok(sprite) => {
            record_sprite(name, sprite.as_ref(), variant).await;
            sprite
        }
        // A 404 is a permanent answer about the name, so it is worth writing
        // down rather than re-asking on every run.
        Err(ApiError::NotFound(_)) => {
            record_sprite(name, None, variant).await;
            None
        }
        // A transient failure must not be written down as "no artwork", or the
        // species would stay blank for as long as the cache lives.
        Err(_) => None,
    }
}

/// The `/pokemon` key a species' artwork is filed under, cache first.
///
/// Most of the time this is the species name itself, but a species whose
/// default form has its own name (`giratina` -> `giratina-altered`) has no
/// `/pokemon` entry under the bare name at all, and its card would otherwise
/// stay blank forever.
///
/// `name` can also arrive already being a variety (`raichu-alola`, straight out
/// of the master list), which has no species record of its own. That 404 means
/// "the name is its own key" — cached like any other answer, so the failing
/// request happens once per install rather than once per view.
async fn resolve_default_variety(client: &reqwest::Client, name: &str) -> Option<String> {
    if let Some(variety) = cache::load_default_variety(name).await {
        return Some(variety);
    }
    let variety = match api::fetch_default_variety(client, name).await {
        Ok(variety) => variety,
        Err(ApiError::NotFound(_)) => name.to_string(),
        // Nothing was learned, so nothing is written down; the next run asks
        // again rather than filing a network hiccup as a fact.
        Err(_) => return None,
    };
    cache::store_default_variety(name, &variety).await;
    Some(variety)
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

/// Resolves one move's record from the cache, falling back to the network.
async fn resolve_move(client: &reqwest::Client, name: &str) -> Option<MoveInfo> {
    if let Some(info) = cache::load_move(name).await {
        return Some(info);
    }
    let info = api::fetch_move(client, name).await.ok()?;
    cache::store_move(name, &info).await;
    Some(info)
}

/// Resolves one filter term's roster from the cache, falling back to the
/// network.
///
/// A failure yields an empty roster rather than an error: the only way to get
/// here is something typed in the search box, and the honest answer to a term
/// we cannot resolve is that nothing matches it.
async fn resolve_roster(client: &reqwest::Client, term: &RosterTerm) -> Vec<String> {
    if let Some(members) = cache::load_roster(term).await {
        return members;
    }
    match api::fetch_roster(client, term).await {
        Ok(members) => {
            cache::store_roster(term, &members).await;
            members
        }
        Err(_) => Vec::new(),
    }
}

/// Stores a freshly fetched sprite, or the fact that there wasn't one, so the
/// next run does not repeat the request either way. Recorded per palette: a
/// species can be cached shiny and unknown normal, or the other way round.
async fn record_sprite(name: &str, sprite: Option<&Sprite>, variant: SpriteVariant) {
    match sprite {
        Some(sprite) => cache::store_sprite(name, sprite, variant).await,
        None => cache::store_missing_sprite(name, variant).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RosterKind;

    #[test]
    fn every_sort_key_reads_back_out_of_its_code() {
        for sort in [SortKey::Dex, SortKey::Name] {
            assert_eq!(SortKey::from_code(sort.code()), Some(sort));
        }
    }

    #[test]
    fn an_unknown_sort_code_is_not_guessed_at() {
        assert_eq!(SortKey::from_code("stat-total"), None);
        assert_eq!(SortKey::from_code(""), None);
    }

    /// An app with a list already in it and nothing in flight. `App::new`
    /// touches no network of its own — it only builds the client the fetch
    /// tasks would use — so this stays a plain unit test.
    fn app_listing(entries: &[(u32, &str)]) -> App {
        let (mut app, _rx) = App::new(Startup::default()).expect("client builds");
        app.all_pokemon = entries
            .iter()
            .map(|&(id, name)| PokemonEntry {
                name: name.to_string(),
                id,
            })
            .collect();
        app
    }

    /// The membership set a roster resolves to.
    fn members(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// The names currently visible in the sidebar, in order.
    fn visible(app: &App) -> Vec<&str> {
        app.filtered
            .iter()
            .map(|&idx| app.all_pokemon[idx].name.as_str())
            .collect()
    }

    /// A chain of `names`, each stage evolving into the next.
    fn line(names: &[&str]) -> EvolutionTree {
        let mut node = EvolutionTree {
            name: names[names.len() - 1].to_string(),
            condition: None,
            children: Vec::new(),
        };
        for name in names.iter().rev().skip(1) {
            node = EvolutionTree {
                name: (*name).to_string(),
                condition: None,
                children: vec![node],
            };
        }
        node
    }

    /// Enough of a record to keep [`App::request_selected`] on its cache-hit
    /// path: the fetch it would otherwise spawn wants a runtime these tests
    /// have no reason to build.
    fn loaded(name: &str) -> PokemonDetail {
        PokemonDetail {
            name: name.to_string(),
            species: name.to_string(),
            dex_number: 0,
            is_legendary: false,
            is_mythical: false,
            is_baby: false,
            types: Vec::new(),
            abilities: Vec::new(),
            stats: Vec::new(),
            height: 0,
            weight: 0,
            sprite_url: None,
            shiny_sprite_url: None,
            genera: HashMap::new(),
            flavors: HashMap::new(),
            moves: Vec::new(),
            learnset_games: None,
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn taking_a_stage_collapses_the_full_screen_chain() {
        let mut app = app_listing(&[(1, "bulbasaur"), (2, "ivysaur")]);
        // Artwork is the other thing a selection spawns fetches for.
        app.color_depth = Depth::None;
        app.recompute_filter();
        app.details
            .insert("bulbasaur".to_string(), loaded("bulbasaur"));
        app.details.insert("ivysaur".to_string(), loaded("ivysaur"));
        app.selected_name = Some("bulbasaur".to_string());
        app.evolutions
            .insert("bulbasaur".to_string(), line(&["bulbasaur", "ivysaur"]));

        app.open_evolution_card();
        assert!(app.evo_card);
        // The cursor opens on the species already in the detail panel.
        assert_eq!(app.evo_cursor, 0);

        app.handle_evo_card_key(press(KeyCode::Right));
        app.handle_evo_card_key(press(KeyCode::Enter));

        // Ivysaur is loaded, and the chain is out of the way of it.
        assert_eq!(app.selected_name.as_deref(), Some("ivysaur"));
        assert!(!app.evo_card);
    }

    #[test]
    fn the_full_screen_chain_does_not_open_on_a_species_with_no_chain_loaded() {
        let mut app = app_listing(&[(1, "bulbasaur")]);
        app.recompute_filter();
        app.selected_name = Some("bulbasaur".to_string());

        app.open_evolution_card();
        assert!(!app.evo_card);
    }

    #[test]
    fn roster_terms_of_different_kinds_narrow_together() {
        // `egg:grass` also has to survive the trip through the alias table on
        // the way to the group PokeAPI files as `plant`.
        let mut app = app_listing(&[(1, "bulbasaur"), (43, "oddish"), (92, "gastly")]);
        app.rosters.insert(
            RosterTerm::new(RosterKind::Type, "poison"),
            members(&["bulbasaur", "oddish", "gastly"]),
        );
        app.rosters.insert(
            RosterTerm::new(RosterKind::EggGroup, "plant"),
            members(&["bulbasaur", "oddish"]),
        );

        app.query = "type:poison".to_string();
        app.recompute_filter();
        assert_eq!(visible(&app), ["bulbasaur", "oddish", "gastly"]);

        app.query = "type:poison egg:grass".to_string();
        app.recompute_filter();
        assert_eq!(visible(&app), ["bulbasaur", "oddish"]);
    }

    #[test]
    fn the_list_waits_until_every_roster_it_needs_has_landed() {
        // Each roster arrives on its own message, and until the last one does
        // the sidebar has to say "loading" rather than "no results" — an
        // unresolved term matches nothing, so the two look identical from the
        // list alone.
        let mut app = app_listing(&[]);
        app.parsed_query = Query::parse("type:poison ability:levitate");
        assert!(app.awaiting_roster());

        app.rosters
            .insert(RosterTerm::new(RosterKind::Type, "poison"), HashSet::new());
        assert!(app.awaiting_roster());

        app.rosters.insert(
            RosterTerm::new(RosterKind::Ability, "levitate"),
            HashSet::new(),
        );
        assert!(!app.awaiting_roster());
    }

    #[test]
    fn the_moves_cursor_stops_at_both_ends_of_the_learnset() {
        // Wrapping would be worse than stopping here: a learnset is one long
        // list, and jumping from the last tutor move back to level one loses
        // the reader's place rather than saving them a keypress.
        let mut app = app_listing(&[]);
        app.move_move_cursor(-1, 3);
        assert_eq!(app.move_cursor, 0);
        app.move_move_cursor(10, 3);
        assert_eq!(app.move_cursor, 2);
        // An empty learnset has no row to land on.
        app.move_cursor = 0;
        app.move_move_cursor(1, 0);
        assert_eq!(app.move_cursor, 0);
    }

    #[test]
    fn an_app_with_nothing_in_flight_is_idle() {
        assert!(!app_listing(&[]).is_busy());
    }

    #[test]
    fn any_one_pending_request_is_enough_to_keep_the_spinner_turning() {
        // One arm per kind of request. All of them put a spinner on screen, so
        // all of them have to keep the ticker alive; a kind missing from
        // `is_busy` would show as a frozen spinner, which is the failure this
        // pins down.
        /// What kind of request to start, and how to start it.
        type Pending = (&'static str, fn(&mut App));

        let cases: [Pending; 7] = [
            ("list", |app| app.list_loading = true),
            ("detail", |app| app.loading_detail = Some("mew".to_string())),
            ("roster", |app| {
                app.roster_loading
                    .insert(RosterTerm::new(RosterKind::Type, "ghost"));
            }),
            ("ability", |app| {
                app.ability_loading.insert("levitate".to_string());
            }),
            ("translation", |app| {
                app.translating
                    .insert(("mew".to_string(), "tr".to_string()));
            }),
            ("party member", |app| {
                app.team_loading.insert("mew".to_string());
            }),
            ("sprite", |app| {
                app.sprite_loading
                    .entry(SpriteVariant::Normal)
                    .or_default()
                    .insert("mew".to_string());
            }),
        ];

        for (kind, begin) in cases {
            let mut app = app_listing(&[]);
            begin(&mut app);
            assert!(
                app.is_busy(),
                "a pending {kind} request should read as busy"
            );
        }
    }

    #[test]
    fn a_pending_set_that_has_drained_stops_counting() {
        // `sprite_loading` keeps one set per palette and the sets outlive their
        // contents. Testing the map for emptiness rather than its sets would
        // leave the app busy — and redrawing on a timer — forever after the
        // first sprite it ever fetched.
        let mut app = app_listing(&[]);
        app.sprite_loading.entry(SpriteVariant::Normal).or_default();
        assert!(!app.is_busy());
    }

    #[test]
    fn a_named_species_wins_the_cursor_over_the_longer_name_that_sorts_first() {
        let mut app = app_listing(&[(150, "mewtwo"), (151, "mew")]);
        app.select_named_species("Mew".to_string());

        assert_eq!(app.query, "mew", "the box shows what narrowed the list");
        assert_eq!(app.filtered.len(), 2, "Mewtwo still matches the text");
        assert_eq!(app.current_name().as_deref(), Some("mew"));
    }

    #[test]
    fn a_name_reaching_the_box_carries_its_search_syntax_with_it() {
        let mut app = app_listing(&[(25, "pikachu"), (26, "raichu")]);
        app.select_named_species("dex:26".to_string());

        assert_eq!(app.current_name().as_deref(), Some("raichu"));
    }

    #[test]
    fn a_name_matching_nothing_lands_on_the_empty_list_rather_than_an_error() {
        let mut app = app_listing(&[(1, "bulbasaur")]);
        app.select_named_species("gengr".to_string());

        assert!(app.filtered.is_empty());
        assert_eq!(app.current_name(), None, "nothing to load, nothing loaded");
        assert_eq!(app.query, "gengr", "and the box says why the list is empty");
    }

    #[test]
    fn a_lang_flag_outranks_the_language_the_last_run_left_behind() {
        let (mut app, _rx) = App::new(Startup {
            language: Some(Language::Turkish),
            ..Startup::default()
        })
        .expect("client builds");
        app.restore(Session {
            language: Some("de".to_string()),
            ..Session::default()
        });

        assert_eq!(app.language, Language::Turkish);
    }

    #[test]
    fn without_the_flag_the_stored_language_is_still_what_comes_back() {
        let (mut app, _rx) = App::new(Startup::default()).expect("client builds");
        app.restore(Session {
            language: Some("de".to_string()),
            ..Session::default()
        });

        assert_eq!(app.language, Language::German);
    }
}
