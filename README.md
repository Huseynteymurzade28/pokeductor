# Pokeductor

**A retro terminal Pokédex & Evolution Analyzer, powered by [PokeAPI](https://pokeapi.co/).**

Browse every species, read localized Pokédex entries, study branching evolution
chains as connected sprite cards — all rendered with Unicode half-blocks in a
warm, PICO-8-inspired palette. Built in Rust with [ratatui](https://ratatui.rs/).

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)
![ratatui](https://img.shields.io/badge/TUI-ratatui%200.29-blueviolet)
![Async](https://img.shields.io/badge/runtime-tokio-green)
![License](https://img.shields.io/badge/license-MIT-yellow)

</div>

---

## ✨ Features

- **Full Pokédex** — every species from PokeAPI, searchable and filterable in real time.
  The sidebar shows National Pokédex numbers and sorts by dex order or A–Z (`S`).
- **Search that narrows by more than the name** — combine plain text with
  `type:` and `gen:` terms: `type:ghost gen:1` finds Gastly, Haunter and Gengar;
  `type:water type:flying` finds the dual-typed ones. Generations are matched
  offline from dex ranges, and each type's roster costs a single request that is
  then cached forever.
- **Sprite rendering in the terminal** — official artwork drawn with Unicode
  upper-half-blocks (two pixels per character cell), cropped to its opaque bounds,
  area-averaged on downscale and alpha-blended over the panel so edges stay clean.
- **Interactive evolution graph** — focus the evolution panel and the chain is laid
  out as connected sprite cards (branching chains like Eevee included). Hop to any
  stage with a keypress to instantly inspect a Pokémon's next evolution.
- **Evolution requirements** — every stage is labelled with what it takes to get
  there: `Lv. 16`, `Use Water Stone`, `Trade for Shelmet`, `Happiness 160`,
  `At night`, `Knows a Fairy move`, and the rest. Move the cursor onto a stage
  and the full set of conditions is spelled out under the panel.
- **Info cards** — national Pokédex number, genus, flavor-text blurb, and
  **Legendary / Mythical / Baby** badges for each species.
- **Abilities** — every ability a species can have is listed on the info card,
  with its Hidden Ability marked. Press `A` for what each one actually does,
  localized like the rest of the Pokédex text. The names ride along in the
  payload the app already fetches, so that row costs nothing; only the
  descriptions need a request, and each is cached for good.
- **Type matchup analysis** — press `T` for a card showing exactly what the
  species takes ×4 / ×2 / ×½ / ×¼ / ×0 damage from, plus what its own same-type
  moves are super effective against. Computed offline from a built-in
  Generation VI+ type chart, so it opens instantly.
- **Party builder** — press `Space` in the list to put up to six Pokémon on a
  team, then `P` for the verdict on their combined typings: attacking types
  that hit **several** members hard, types **nobody** resists, and types
  **nobody** hits back hard. That overlap — not any one member's weaknesses —
  is what decides whether a team holds together, and it is computed from the
  same offline chart, so the card is instant.
- **6 interface languages** — English, Türkçe, Deutsch, Français, Español, Italiano,
  switchable live from a little picker card.
- **Localized Pokédex text** — flavor/genus come straight from PokeAPI in EN/DE/FR/ES/IT;
  for languages PokeAPI doesn't carry (Turkish), the English entry is machine-translated
  on demand and cached.
- **Responsive layout** — every panel reflows to the terminal size; sprites scale to fit.
- **Snappy & async** — all network I/O runs on `tokio` tasks; the UI never blocks,
  and each species is fetched at most once per session.
- **Works offline** — every response is cached under `~/.cache/pokeductor`
  (`$XDG_CACHE_HOME` is honoured), so species you have already looked at open
  instantly on the next run and keep working with no connection at all. The
  cache is safe to delete at any time; it just refills itself.

---

## 📸 Screenshots


<table>
  <tr>  
    <td align="center">
      <img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/screenshot-main.png" alt="Main view" width="420"><br>
      <sub>Main view — list, details & info card</sub>
    </td>
    <td align="center">
      <img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/screenshot-search.png" alt="Evolution graph" width="420"><br>
      <sub>Search Menu</sub>
    </td>
  </tr>
  <tr>
    <td align="center" colspan="2">
      <img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/demo.gif" alt="Demo" width="420"><br>
      <sub>Quick demo</sub>
    </td>
  </tr>
</table>
---

## Easy Install with Cargo
```bash
cargo install pokeductor
```

## 🚀 Getting started

### Requirements

- **Rust** (stable, 2021 edition) — install via [rustup](https://rustup.rs/).
- A **truecolor (24-bit) terminal** for the sprite colors (most modern terminals qualify).
- A font with **Unicode block + box-drawing glyphs** (e.g. any Nerd Font, Fira Code, JetBrains Mono).
- An **internet connection** for anything not already cached — see
  [Works offline](#-features).

### Build & run

```bash
# Clone, then from the project root:
cargo run --release
```

The first launch fetches the species list and opens on Bulbasaur; moving through
the list loads each Pokémon's details, evolution chain and artwork on demand.
Everything fetched is cached under `~/.cache/pokeductor`, so later launches are
instant and work offline.

---

## ⌨️ Controls

| Context | Key | Action |
|---|---|---|
| **List** | `↑` / `↓` · `j` / `k` | Move selection |
| | `PgUp` / `PgDn` | Jump 10 |
| | `Enter` | Load the highlighted Pokémon |
| | `/` or `Tab` | Focus the search box |
| | `E` | Focus the evolution panel |
| | `T` | Open the type matchup card |
| | `A` | Open the ability card |
| | `Space` | Add / remove the highlighted Pokémon from the party |
| | `P` | Open the party card |
| | `S` | Cycle sort: dex order ↔ A–Z |
| | `L` | Open the language picker |
| | `Q` / `Esc` | Quit |
| **Search** | *type* | Filter the list (see syntax below) |
| | `Enter` | Load result & return to list |
| | `Esc` / `Tab` | Back to list |
| **Evolution** | `←` `→` `↑` `↓` · `h` `j` `k` `l` | Move between chain members |
| | `Enter` | Jump to the highlighted evolution |
| | `Esc` / `Tab` | Back to list |
| **Language card** | `↑` / `↓` | Move selection |
| | `Enter` / `Space` | Apply |
| | `Esc` | Cancel |
| **Matchup card** | `Esc` / `T` | Close |
| **Party card** | `Esc` / `P` | Close |
| **Ability card** | `Esc` / `A` | Close |
| **Anywhere** | `Ctrl-C` | Quit |

### Search syntax

| Query | Matches |
|---|---|
| `char` | names containing "char" |
| `type:water` | every Water Pokémon (`t:` also works) |
| `type:water type:flying` | Water **and** Flying — Gyarados, Mantine, … |
| `gen:1` | introduced in Generation I (`g:` also works) |
| `gen:1 gen:2` | either generation |
| `gen:1 type:ghost ga` | all three at once |

Terms combine with the name search, so anything that isn't a recognised term is
treated as ordinary text. A `gen:` filter skips alternate forms such as
`raichu-alola`, whose ids carry no dex number to derive a generation from.

---

## 🏗️ Architecture

A small, layered design — the rendering layer is pure functions of application
state, and all network work happens off the UI thread.

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point; sets up the terminal and the `tokio` runtime. |
| `models.rs` | API-agnostic domain types (`PokemonDetail`, `EvolutionTree`, `Sprite`). |
| `api.rs` | Async PokeAPI client, evolution-chain parser, sprite decode, translation. |
| `cache.rs` | On-disk cache of every fetched response, for instant and offline starts. |
| `query.rs` | Search-box syntax (`type:`, `gen:`) parsing. |
| `app.rs` | State machine + `tokio::select!` event loop (input · messages · animation tick). |
| `ui.rs` | All `ratatui` rendering, including the sprite & evolution-graph drawing. |
| `i18n.rs` | `Language` enum + translation tables for the 6 UI languages. |
| `typechart.rs` | Offline Generation VI+ type-effectiveness chart & matchup analysis. |
| `team.rs` | Team-level type analysis built on top of the chart. |
| `theme.rs` | PICO-8-inspired palette and per-type accent colors. |

### How it works

- **Concurrency model.** Background fetch tasks are *producers* that send
  [`Message`]s over an `mpsc` channel; the main loop is the single *consumer*,
  draining the channel alongside terminal input and a steady animation tick via
  `tokio::select!`. The UI thread never blocks on I/O.
- **Caching.** Two layers, both keyed by name. In memory, a given Pokémon is
  fetched at most once per session; on disk, details, evolution chains, sprites,
  type rosters and machine translations survive between runs, so a species you
  have already seen needs no request at all. Every fetch reads through the disk
  cache first and writes back what it had to fetch. Writes are atomic
  (temp file + rename) and version-stamped, and the whole layer is best-effort:
  a cache that cannot be read or written is a miss, never a visible error.
- **Sprite pipeline.** PNG → decode to RGBA (`image`) → crop to opaque bounds →
  box-average downscale (keeping aspect, accounting for ~2:1 cell height) →
  alpha-blend over the panel → emit `▀` half-block cells (fg = top pixel,
  bg = bottom pixel).
- **Alternate forms.** Forms like `raichu-alola` resolve their species/evolution
  data via the base species name from the Pokémon payload, so they don't 404.
  Their ids sit above 10000 and carry no dex number, so they show a blank dex
  column and are skipped by `gen:` filters.
- **Abilities.** A species' abilities arrive inside the `/pokemon` payload that
  is fetched anyway, so listing them is free. Their *descriptions* live behind
  one request each, which is why they are fetched when the card is opened
  rather than for every species browsed past. The text comes from the game
  flavor entries rather than the effect entries: PokeAPI carries flavor in all
  five languages the info card uses, while effect text exists only in English,
  German and French.
- **Party analysis.** A member is added by name; its typing is fetched in the
  background if it isn't loaded yet, and the card lists it greyed out until it
  arrives, so the analysis always describes exactly the members it can see. A
  weakness only counts as *shared* once two members have it — one member's
  weakness is that member's problem, not the team's.
- **Filtering.** Generations come from a fixed table of dex ranges — released
  generations never change — so `gen:` costs nothing. `type:` needs a roster,
  but `/type/{name}` answers a whole filter in one request that is then cached
  forever. Sorting sticks to keys the list response already carries; ordering by
  base-stat total would mean fetching all ~1300 species for a single keypress.
- **Evolution requirements.** PokeAPI's `evolution_details` are parsed into a
  structured `EvolutionCondition` and phrased through per-language templates, so
  each translation decides where the value lands (`Use {}` vs. `{} kullan`).
  Where a stage can be reached more than one way, the first (current-generation)
  route is shown.

### Built with

[`ratatui`](https://crates.io/crates/ratatui) ·
[`crossterm`](https://crates.io/crates/crossterm) ·
[`tokio`](https://crates.io/crates/tokio) ·
[`reqwest`](https://crates.io/crates/reqwest) ·
[`serde`](https://crates.io/crates/serde) ·
[`serde_json`](https://crates.io/crates/serde_json) ·
[`image`](https://crates.io/crates/image) ·
[`anyhow`](https://crates.io/crates/anyhow) ·
[`thiserror`](https://crates.io/crates/thiserror)

---

## 🌍 Localization & translation

- UI strings live in `i18n.rs` — add a language by extending the `Language` enum,
  `Language::ALL`, and adding a `Strings` table.
- Pokédex **flavor text and genus** come from PokeAPI in `en`, `de`, `fr`, `es`, `it`.
- **Evolution requirements** are phrased from the `EvoStrings` templates in
  `i18n.rs`. Item, move and location names inside them stay in English: they
  arrive as PokeAPI slugs and localizing each one would cost an extra request.
- For UI languages PokeAPI has **no** text for (Turkish), the English blurb is
  translated on demand through the free, key-less
  [MyMemory](https://mymemory.translated.net/) API and cached. Translation is
  best-effort: if the service errors or is rate-limited, the original English text
  is shown.

---

## 🙏 Credits

- Data & sprites: [**PokeAPI**](https://pokeapi.co/) (please respect their
  [fair-use policy](https://pokeapi.co/docs/v2)).
- Translations fallback: [**MyMemory**](https://mymemory.translated.net/).
- Pokémon is © Nintendo / Game Freak / The Pokémon Company. This is a
  non-commercial, educational project.

## 📄 License

MIT — see `LICENSE` (add one if you haven't yet).
