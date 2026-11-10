//! pokeductor — a terminal Pokedex & Evolution Analyzer.
//!
//! Module layout:
//! - `models`  : API-agnostic domain types (the data domain layer).
//! - `mouse`   : what the last frame drew where, for hit-testing clicks.
//! - `cache`   : on-disk cache of everything fetched, for instant/offline starts.
//! - `cli`     : argument parsing and the commands that need no terminal.
//! - `clipboard`: copying text through the terminal with OSC 52.
//! - `json`    : the `--json` output shape and the name rule behind it.
//! - `color`   : terminal colour-depth detection and per-frame degradation.
//! - `i18n`    : `Language` enum + translation tables (EN / TR / DE).
//! - `theme`   : Catppuccin Mocha palette and per-type colors.
//! - `api`     : async PokeAPI client and evolution-chain parser.
//! - `query`   : search-box syntax (`type:`, `gen:`) parsing.
//! - `retry`   : retry classification and backoff policy for network requests.
//! - `session`: party and preferences carried over from the previous run.
//! - `showdown`: the party in Pokemon Showdown's team format, both ways.
//! - `statcalc`: final stats from level, nature, IVs and EVs.
//! - `typechart`: offline type-effectiveness chart.
//! - `team`    : team-level type analysis built on top of it.
//! - `app`     : state machine + `tokio::select!` event loop.
//! - `ui`      : `ratatui` rendering.

mod api;
mod app;
mod base_stats;
mod browser;
mod cache;
mod cli;
mod clipboard;
mod color;
mod compare;
mod i18n;
mod json;
mod lookup;
mod models;
mod mouse;
mod query;
mod quiz;
mod retry;
mod session;
mod showdown;
mod statcalc;
mod team;
mod theme;
mod typechart;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Argument handling comes first: `--help`, `--version` and the cache
    // commands all answer without a terminal, and none of them should flicker
    // through the alternate screen on the way.
    let startup = match cli::run().await? {
        cli::Outcome::Handled => return Ok(()),
        cli::Outcome::Launch(startup) => startup,
    };

    let (app, rx) = App::new(startup)?;

    // `ratatui::init` enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores the terminal on the way out.
    let terminal = ratatui::init();
    // `ratatui::init` restores the screen on a panic but knows nothing about
    // the mouse, which `App::run` may have asked the terminal to report. Left
    // on, every movement after the crash would print escape codes into the
    // shell. Releasing a mouse that was never taken is harmless, so the hook
    // does not need to know which it was.
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        mouse::capture(false);
        restore(info);
    }));
    let result = app.run(terminal, rx).await;
    ratatui::restore();

    result
}
