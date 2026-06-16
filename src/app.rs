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
use crate::i18n::Language;
use crate::models::{EvolutionTree, PokemonDetail, PokemonEntry, Sprite};

/// Messages sent from background fetch tasks to the UI loop.
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
    /// A background task failed.
    Error(String),
}

/// Which panel currently receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Search,
    List,
}

/// The complete, observable state of the running application.
pub struct App {
    pub language: Language,
    pub all_pokemon: Vec<PokemonEntry>,
    /// Indices into `all_pokemon` that match the current search query.
    pub filtered: Vec<usize>,
    pub list_state: ListState,
    pub query: String,
    pub focus: Focus,
    /// In-memory cache so each Pokemon is fetched at most once per session.
    pub details: HashMap<String, PokemonDetail>,
    pub evolutions: HashMap<String, EvolutionTree>,
    /// Decoded sprites, keyed by Pokemon name. Absent if a species has no art.
    pub sprites: HashMap<String, Sprite>,
    /// Names whose sprite is being fetched on demand for the evolution overlay,
    /// so we never queue the same request twice.
    pub sprite_loading: HashSet<String>,
    /// Whether the full-screen evolution chain overlay is open (toggled by `E`).
    pub show_evolution: bool,
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
            focus: Focus::List,
            details: HashMap::new(),
            evolutions: HashMap::new(),
            sprites: HashMap::new(),
            sprite_loading: HashSet::new(),
            show_evolution: false,
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

    fn fetch_list(&mut self) {
        self.list_loading = true;
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let msg = match api::fetch_pokemon_list(&client).await {
                Ok(list) => Message::ListLoaded(list),
                Err(err) => Message::Error(err.to_string()),
            };
            let _ = tx.send(msg).await;
        });
    }

    /// Loads (or reveals from cache) the currently highlighted Pokemon.
    fn request_selected(&mut self) {
        let Some(name) = self.current_name() else {
            return;
        };
        self.error = None;
        self.selected_name = Some(name.clone());

        // Cache hit: nothing to fetch.
        if self.details.contains_key(&name) {
            self.loading_detail = None;
            return;
        }

        self.loading_detail = Some(name.clone());
        let tx = self.tx.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let msg = match api::fetch_pokemon_bundle(&client, &name).await {
                Ok((detail, evolution, sprite)) => {
                    Message::PokemonLoaded { detail, evolution, sprite }
                }
                Err(err) => Message::Error(err.to_string()),
            };
            let _ = tx.send(msg).await;
        });
    }

    /// Opens the evolution overlay and kicks off sprite fetches for every
    /// member of the current chain that isn't already cached or in flight.
    fn open_evolution_overlay(&mut self) {
        let Some(tree) = self.selected_evolution() else {
            return; // nothing to show yet
        };

        let mut names = Vec::new();
        tree.collect_names(&mut names);
        self.show_evolution = true;

        for name in names {
            if self.sprites.contains_key(&name) || self.sprite_loading.contains(&name) {
                continue;
            }
            self.sprite_loading.insert(name.clone());
            let tx = self.tx.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                let msg = match api::fetch_named_sprite(&client, &name).await {
                    Ok(sprite) => Message::SpriteLoaded { name, sprite },
                    // A failed chain sprite is non-fatal: report no art so the
                    // overlay shows a placeholder instead of an error banner.
                    Err(_) => Message::SpriteLoaded { name, sprite: None },
                };
                let _ = tx.send(msg).await;
            });
        }
    }

    // --- Message handling ------------------------------------------------

    fn handle_message(&mut self, msg: Message) {
        match msg {
            Message::ListLoaded(list) => {
                self.all_pokemon = list;
                self.list_loading = false;
                self.recompute_filter();
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
                self.details.insert(name, detail);
            }
            Message::SpriteLoaded { name, sprite } => {
                self.sprite_loading.remove(&name);
                if let Some(sprite) = sprite {
                    self.sprites.insert(name, sprite);
                }
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
        // While the evolution overlay is open it captures input: any of Esc/E/Q
        // dismisses it and nothing else gets through.
        if self.show_evolution {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Char('q')
            ) {
                self.show_evolution = false;
            }
            return;
        }
        match self.focus {
            Focus::List => self.handle_list_key(key),
            Focus::Search => self.handle_search_key(key),
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
            KeyCode::Char('e') | KeyCode::Char('E') => self.open_evolution_overlay(),
            KeyCode::Tab | KeyCode::Char('/') => self.focus = Focus::Search,
            KeyCode::Char('l') | KeyCode::Char('L') => self.language = self.language.next(),
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

    fn recompute_filter(&mut self) {
        let query = self.query.to_lowercase();
        self.filtered = self
            .all_pokemon
            .iter()
            .enumerate()
            .filter(|(_, p)| query.is_empty() || p.name.to_lowercase().contains(&query))
            .map(|(idx, _)| idx)
            .collect();

        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            let clamped = self
                .list_state
                .selected()
                .unwrap_or(0)
                .min(self.filtered.len() - 1);
            self.list_state.select(Some(clamped));
        }
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
