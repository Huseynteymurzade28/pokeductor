//! pokeductor — a terminal Pokedex & Evolution Analyzer.
//!
//! Module layout:
//! - `models`  : API-agnostic domain types (the data domain layer).
//! - `i18n`    : `Language` enum + translation tables (EN / TR / DE).
//! - `theme`   : Catppuccin Mocha palette and per-type colors.
//! - `api`     : async PokeAPI client and evolution-chain parser.
//! - `app`     : state machine + `tokio::select!` event loop.
//! - `ui`      : `ratatui` rendering.

mod api;
mod app;
mod i18n;
mod models;
mod theme;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (app, rx) = App::new()?;

    // `ratatui::init` enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores the terminal on the way out.
    let terminal = ratatui::init();
    let result = app.run(terminal, rx).await;
    ratatui::restore();

    result
}
