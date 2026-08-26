# Pokeductor

A terminal Pokédex and evolution analyzer, powered by [PokeAPI](https://pokeapi.co/).

Browse every species, read localized Pokédex entries, study branching evolution
chains as connected sprite cards, and analyse type coverage for a single species
or a whole party — rendered with Unicode half-blocks in a PICO-8-inspired
palette. Built in Rust with [ratatui](https://ratatui.rs/).

[![CI](https://github.com/Huseynteymurzade28/pokeductor/actions/workflows/ci.yml/badge.svg)](https://github.com/Huseynteymurzade28/pokeductor/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)
![MSRV](https://img.shields.io/badge/MSRV-1.88-orange?logo=rust)
![ratatui](https://img.shields.io/badge/TUI-ratatui%200.29-blueviolet)
![Async](https://img.shields.io/badge/runtime-tokio-green)
![License](https://img.shields.io/badge/license-MIT-yellow)

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-main.png" alt="Pokeductor main view: species list, details panel with sprite and base stats, and the evolution chain" width="900">

A short tour — filtering by type and generation, type matchups, abilities,
building a party, and the help overlay:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/demo.gif" alt="Animated tour: filtering with type:ghost gen:1, opening the matchup and ability cards for Gengar, adding three Pokemon to a party, and the help overlay" width="860">

---

## Install

```bash
cargo install pokeductor
```

Or from a clone:

```bash
cargo run --release
```

### Arch Linux

Packaged in the [AUR](https://aur.archlinux.org/packages/pokeductor), built from
the crates.io release:

```bash
yay -S pokeductor
```

### Requirements

- **Rust 1.88 or newer** (2021 edition) — via [rustup](https://rustup.rs/). Not
  needed for the AUR package, which builds it for you.
- A **truecolor (24-bit) terminal** for sprites at their best. Not a
  requirement: a 256-colour terminal gets the artwork quantized to its palette,
  and one with no colour at all gets the interface without sprites rather than a
  field of blank blocks. See [Colour](#colour).
- A font with **Unicode block and box-drawing glyphs** — any Nerd Font, Fira
  Code, JetBrains Mono, and most modern monospace fonts qualify.
- An **internet connection** for anything not already cached. After a species
  has been viewed once it opens with no network at all; see
  [Caching](#caching).

The first launch fetches the species list and opens on Bulbasaur — or on
whatever you name, `pokeductor gengar`. Moving through the list loads each
Pokémon's details, evolution chain and artwork on demand. See [Command
line](#command-line) for the full set of flags.

---

## The interface

### Browsing and filtering

The sidebar lists all 1302 entries PokeAPI serves, with National Pokédex
numbers. A bare number looks up that Pokédex number — `25` finds Pikachu —
and beyond plain name matching the search box takes `dex:`, `type:` and `gen:`
terms:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-search.png" alt="Searching for type:ghost gen:1, narrowing the list to Gastly, Haunter and Gengar" width="900">

`type:` terms combine with **AND**, so `type:water type:flying` finds the
dual-typed ones. `gen:` and `dex:` terms combine with **OR**, since a species
belongs to exactly one generation and carries exactly one number, so requiring
two at once could only ever match nothing.
Anything that is not a recognised term is treated as ordinary text, so a stray
colon degrades to a name search instead of an error. `S` cycles the sort between
Pokédex order and alphabetical; the highlighted species stays under the cursor
across a re-sort or a narrowing search.

### Species details and abilities

The info panel carries the dex number, genus, typing, abilities, physical
measurements and base stats, with a flavour blurb when the panel has room for
one. `A` opens the abilities card:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-abilities.png" alt="The abilities card for Snorlax, showing Immunity, Thick Fat and the hidden ability Gluttony with descriptions" width="900">

Ability *names* arrive inside the payload every species fetch pulls down
already, so listing them on the info card costs nothing. Their descriptions live
behind one request each and are cached permanently. Hidden abilities are marked:
they are only obtainable by special means, which is worth knowing at a glance.

### Evolution chains

Chains are laid out as connected sprite cards, each stage labelled with what it
takes to reach it. Focus the panel with `E` to move between stages; `Enter`
jumps to the highlighted one, and the full set of conditions for the stage under
the cursor is spelled out beneath the panel.

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-evolution.png" alt="The evolution panel focused on the Charmander line, with the cursor on Charmeleon and its Lv. 16 requirement" width="900">

Branching chains are handled as an n-ary tree, so every route is represented.
When there are too many branches to draw as cards in the space available, the
panel falls back to a labelled list rather than truncating:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-branching.png" alt="Eevee's eight-way evolution chain listed with the requirement for each branch" width="900">

### Shiny artwork

`X` flips every sprite on screen — the info panel and each card in the
evolution chain — into its shiny palette, so a whole line can be inspected in
the colours it is hunted for. The toggle is app-wide rather than per-species:
it stays on as you move through the list, and the info panel carries a
`✦ Shiny` badge while it does, since an unfamiliar palette otherwise reads as a
rendering bug.

Only the palette on screen is fetched, so flipping the toggle never pulls down
two full sets of artwork, and both are cached separately on disk. A species
PokeAPI ships no shiny sprite for falls back to its normal one.

### Colour

Sprites are RGB half-blocks, so how they land depends on what the terminal can
show. Rather than requiring 24-bit colour, Pokeductor works out what it has and
renders to it:

| Terminal | What you get |
|---|---|
| Truecolor (`COLORTERM=truecolor`) | The artwork as decoded, 24-bit. |
| 256 colours | Sprites quantized to the xterm palette — the 6×6×6 cube plus the greyscale ramp — so they stay readable instead of being dropped by a terminal that cannot parse the escape. |
| No colour (`NO_COLOR`, `TERM=dumb`, `--color=never`) | The full interface without sprites. A sprite with no colour is a rectangle of identical blocks, which says less than the placeholder the panels already fall back to. |

Detection is deliberately conservative: `COLORTERM` is the only positive claim
of 24-bit support that gets taken at its word, so a terminal announcing itself
through `TERM` alone gets the 256-colour path even when it would in fact have
managed truecolor. A coarser sprite is a much smaller cost than an unreadable
one, and `--color=truecolor` is there for anyone who knows better.

`NO_COLOR` is honoured, and `--color` outranks it — naming a depth on the
command line answers the question the environment is being consulted about.
With no colour to work with, the list cursor and the evolution highlight fall
back to reverse video, so there is still always something showing where you are.

### Type matchups

`T` opens the defensive and offensive breakdown for the current species:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-matchups.png" alt="Type matchup card for Charizard showing x4 Rock, x2 Water and Electric, and immunity to Ground" width="900">

Computed offline from a built-in Generation VI+ chart, so it opens instantly and
works with no connection. Neutral matchups are omitted — they are the default,
and listing them would bury the rows worth reading.

### Party analysis

`Space` puts up to six Pokémon on a team; `P` shows the verdict on their
combined typings:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-party.png" alt="Party card for Rotom, Charizard, Gyarados and Snorlax showing shared weaknesses, unresisted types, an ability immunity and offensive gaps" width="900">

A party asks a different question than a single species does, and the answer is
not the union of its members' weaknesses — it is the overlap:

| Section | What it means |
|---|---|
| **Shared weaknesses** | Attacking types hitting **two or more** members super-effectively, labelled with how many. A type that hits one member is that member's problem, not the team's. |
| **Resisted by nobody** | Attacking types **no** member resists or is immune to. There is no safe switch-in against them. |
| **Immune by ability** | Immunities the type chart cannot see (Levitate, Water Absorb, Flash Fire, …). |
| **Hit hard by nobody** | Defending types **no** member hits super-effectively with a same-type move. These wall the team. |

Note the differing thresholds: the first section asks "how many at once", the
others ask "is there any answer at all". An empty section is good news, and the
card says so rather than leaving a blank.

Ability immunities are deliberately kept *out* of the numbers. A species carries
one of its listed abilities, not all of them, so an immunity is only a certainty
when the species had no other ability it could have had — anything else is
flagged as merely possible. Folding that uncertainty into the counts would make
the card claim something the data does not support.

The party is picked up where it was left: it is written out on exit and
restored on the next run, along with the language, the palette and the sort
order. See [Session state](#session-state).

### Help

Every binding in one place, grouped by where it applies:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-help.png" alt="The help overlay listing all key bindings for the list, search box, evolution panel and cards" width="900">

### Localization

Six interface languages, switchable live from a picker card with no restart and
no refetch:

<img src="https://raw.githubusercontent.com/Huseynteymurzade28/pokeductor/main/assets/ui-turkish.png" alt="The same Gengar view with the interface in Turkish" width="900">

---

## Key bindings

| Context | Key | Action |
|---|---|---|
| **List** | `↑` `↓` · `j` `k` | Move selection |
| | `PgUp` `PgDn` | Jump ten |
| | `Enter` | Load the highlighted Pokémon |
| | `/` · `Tab` | Focus the search box |
| | `E` | Focus the evolution panel |
| | `T` | Type matchup card |
| | `A` | Ability card |
| | `X` | Toggle shiny artwork |
| | `Space` | Add / remove from the party |
| | `P` | Party card |
| | `S` | Cycle sort: Pokédex order ↔ A–Z |
| | `L` | Language picker |
| | `?` | Help overlay |
| | `Q` · `Esc` | Quit |
| **Search box** | *type* | Filter the list |
| | `Enter` | Load the result and return to the list |
| | `Esc` · `Tab` | Back to the list |
| **Evolution panel** | `←` `→` `↑` `↓` · `h` `j` `k` `l` | Move between stages |
| | `Enter` | Jump to the highlighted stage |
| | `X` | Toggle shiny artwork |
| | `Esc` · `Tab` | Back to the list |
| **Any card** | `Esc` | Close |
| **Anywhere** | `Ctrl-C` | Quit |

### Search syntax

| Query | Matches |
|---|---|
| `char` | names containing "char" |
| `25` | Pokédex number 25 — Pikachu — **or** a name containing "25" |
| `dex:25` | Pokédex number 25, without the name fallback (`d:` also works) |
| `dex:1-151` | every number in that range — the Kanto dex |
| `type:water` | every Water Pokémon (`t:` also works) |
| `type:water type:flying` | Water **and** Flying — Gyarados, Mantine, … |
| `gen:1` | introduced in Generation I (`g:` also works) |
| `gen:1 gen:2` | either generation |
| `gen:1 type:ghost ga` | all three at once |

`dex:` and `gen:` filters skip alternate forms such as `raichu-alola`: their
ids sit above 10000 and are not dex numbers, so there is nothing to test a range
or derive a generation from. A bare number still reaches them by name.

---

## Command line

```
Usage: pokeductor [OPTIONS] [NAME]

Arguments:
  [NAME]  Open directly on this species, e.g. `pokeductor gengar`

Options:
      --lang <LANG>   Start in this UI language [possible values: en, tr, de, fr, es, it]
      --color <WHEN>  How much colour the terminal can show [default: auto] [possible values: auto, truecolor, 256, never]
      --clear-cache   Delete the on-disk cache and exit
      --cache-dir     Print the cache directory and exit
  -h, --help          Print help (see more with '--help')
  -V, --version       Print version
```

`NAME` goes into the search box rather than through a parser of its own, so
everything [the search syntax](#search-syntax) understands works here too:

```bash
pokeductor gengar          # straight to Gengar
pokeductor 25              # Pokedex number 25 — Pikachu
pokeductor type:ghost      # open with the list already filtered
```

An exact name wins the cursor even when something longer sorts ahead of it —
`pokeductor mew` opens Mew, not Mewtwo — and a name that matches nothing leaves
you on the same empty list typing it would have, with the query still in the
box saying why.

`--lang` outranks the language [the previous run left
behind](#session-state) for this run, and being an ordinary choice like any
made from the picker, it is what gets stored on the way out.

`--color` overrides what [colour detection](#colour) concluded, in either
direction: `--color=truecolor` on a terminal that never advertised it, or
`--color=never` on one that did.

The two cache commands answer the question this README used to answer with a
path and a `rm -rf`. Both print what they touched:

```console
$ pokeductor --cache-dir
/home/you/.cache/pokeductor
$ pokeductor --clear-cache
Removed /home/you/.cache/pokeductor
```

---

## Architecture

A layered design. The rendering layer is a pure function of application state,
all network work happens off the UI thread, and everything fetched is written
through to disk.

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point; argument handling, terminal setup and the `tokio` runtime. |
| `models.rs` | API-agnostic domain types (`PokemonDetail`, `EvolutionTree`, `Sprite`, `Ability`). |
| `api.rs` | Async PokeAPI client, evolution-chain parser, sprite decode, translation. |
| `cache.rs` | On-disk cache of every fetched response, for instant and offline starts. |
| `cli.rs` | Argument parsing, and the commands that answer without a terminal. |
| `color.rs` | Terminal colour-depth detection, and the per-frame degradation pass. |
| `session.rs` | Party and preferences carried over from the previous run. |
| `query.rs` | Search-box syntax (`dex:`, `type:`, `gen:`) parsing. |
| `app.rs` | State machine and `tokio::select!` event loop (input · messages · animation tick). |
| `ui.rs` | All `ratatui` rendering, including the sprite and evolution-graph drawing. |
| `typechart.rs` | Offline Generation VI+ type-effectiveness chart and matchup analysis. |
| `team.rs` | Team-level type analysis built on top of the chart. |
| `i18n.rs` | `Language` enum and translation tables for the six UI languages. |
| `theme.rs` | PICO-8-inspired palette and per-type accent colours. |

### Concurrency

Background fetch tasks are *producers* that send `Message`s over an `mpsc`
channel; the main loop is the single *consumer*, draining that channel alongside
terminal input and an animation tick via `tokio::select!`. The UI thread never
blocks on I/O, and no state is shared across tasks — a task owns what it needs
and hands the result back as a message.

The tick is not steady: it is selected on only while something is in flight,
which is the only time a spinner is on screen to animate. Idle, the loop blocks
on input and messages alone and draws when one of them says something changed,
so a Pokédex left open in a split costs nothing at all — an idle measurement
goes from ~1.1% of a core to 0.00%. Waking up uses `MissedTickBehavior::Delay`,
so a ticker whose deadline went by during a long sleep fires once and schedules
the next a full period out, rather than bursting through every frame it missed.

### Caching

Two layers, both keyed by name.

In memory, a given Pokémon is fetched at most once per session. On disk, under
`$XDG_CACHE_HOME/pokeductor` (falling back to `~/.cache/pokeductor`):

```
list.json                 master species list, 30-day TTL
species/<name>.json       details + parsed evolution tree
sprites/<name>.png        decoded artwork, re-encoded as PNG
types/<type>.json         roster backing a type: filter
abilities/<slug>.json     localized ability name and description
translations/<name>.<lang>.txt
```

Every fetch reads through the disk cache first and writes back only what it had
to fetch, so a species already seen needs no request at all and the app keeps
working with no connection. PokeAPI is effectively an append-only archive — a
species' stats, typing and evolution chain do not change once published — so
only the master list carries a TTL, and a stale list is still shown while a
refresh is attempted in the background rather than replaced by an error.

Writes go through a temporary file and a rename, so an interrupted run cannot
leave a half-written entry for the next one to read back as valid. Every entry
is version-stamped: a build whose cached representation has changed shape treats
older files as misses instead of mis-parsing them. The whole layer is
best-effort — a cache that cannot be read or written is a miss, never an error
the user sees. It is safe to delete at any time; it refills itself, and
`--clear-cache` does it without anyone having to work out the path first.

### Session state

What the cache holds is a second copy of something PokeAPI already knows, so
deleting it costs nothing but a refetch. The choices made during a run are the
opposite — nothing can reconstruct the party someone assembled — so they are
kept apart from it, under `$XDG_STATE_HOME/pokeductor` (falling back to
`~/.local/state/pokeductor`):

```
session.json              party, language, sort order, shiny toggle
```

Written once, as the app exits, and read once, before the first frame. A run
that is killed rather than quit therefore leaves the previous session in place,
and of two instances quitting in turn the last one wins — both acceptable for a
convenience that never holds anything the user cannot rebuild in a few
keystrokes.

The file is version-stamped and pretty-printed, and read back defensively: it
sits in a directory users are invited to look inside, so a party longer than
the six-member limit is trimmed rather than rejected, and a setting recorded in
terms this build does not recognise — a language it no longer ships, say —
leaves that setting at its default instead of discarding the whole file.
Preferences are stored as codes (`"tr"`, `"name"`) rather than as enum indices,
so reordering an enum in Rust can never silently switch somebody's language.

### Sprite pipeline

PokeAPI's `front_default` PNG (96×96) → decode to RGBA via `image` → crop to
opaque bounds → box-average downscale, keeping aspect and accounting for the
roughly 2:1 cell aspect ratio → alpha-blend over the panel colour → emit `▀`
half-block cells, foreground being the top pixel and background the bottom.

Area averaging rather than nearest-neighbour sampling is what keeps downscaled
sprites smooth instead of leaving the hard outline pixels as ragged lines.

Sprites are cached re-encoded as PNG rather than as raw RGBA: a few kilobytes
compressed against ~36 KB flattened, and the decoder is already a dependency.

### Colour depth

Detection resolves once at startup — `COLORTERM`, then `TERM`, with `NO_COLOR`
and `--color` on top — into a single `Depth` carried on the app. The renderer
never sees it: every widget writes 24-bit colour as before, and the finished
buffer is rewritten on its way out of `terminal.draw`, mapping each cell's
foreground and background to a palette index or to nothing at all.

Doing it over the buffer rather than at each call site is what makes it one rule
instead of a condition threaded through nineteen hundred lines of rendering —
and it catches the sprite cells for free, since by then they are ordinary
coloured cells like any other.

Quantization compares two candidates: the nearest colour in the xterm 6×6×6 cube
and the nearest step on the 24-entry greyscale ramp. Both are needed because the
ramp is far finer than the cube's diagonal, so a mid-grey has a near-exact match
on one and a visible cast on the other. Every palette entry quantizes to itself,
which is the invariant the tests pin down.

One thing survives the loss of colour deliberately. The list cursor and the
evolution highlight are written as a foreground/background pair *plus*
`REVERSED` — which a coloured terminal simply swaps back, drawing exactly what
it drew before, and a colourless one renders as reverse video. Written the
usual way round the highlight bar would vanish under `--color=never`, and with
it any way to tell where the cursor is.

crossterm reads `NO_COLOR` itself and strips colour sequences when it is set, so
an explicit `--color` also calls `force_color_output` to say which of the two
answers won.

### Type and team analysis

`typechart.rs` holds the Generation VI+ chart as a pure function from
(attacking type, defending type) to a multiplier, with the dual-type case
derived by multiplying across the defender's types exactly as the games do. No
round-trip is needed to answer "what is this weak to?".

`team.rs` builds on it. For each of the 18 attacking types it counts how many
members take super-effective damage and whether *anyone* resists, and it derives
offensive gaps from the union of the members' same-type coverage. Ability
immunities come from a small static table of abilities that grant an outright
immunity to a whole damage type. Abilities that merely soften a type (Thick Fat)
belong to multipliers, and abilities keyed to a class of move rather than a type
(Soundproof, Bulletproof) cannot be expressed as one, so neither is tabled.

### Filtering and sorting

Generations are derived locally from a fixed table of dex ranges — released
generations never gain or lose species — so `gen:` costs no request. `type:`
needs a roster, but `/type/{name}` answers a whole filter in one request that is
then cached permanently; the alternative would be fetching ~1300 species just to
read their typings.

Sorting is deliberately limited to keys the list response already carries. Each
entry's id is parsed out of the URL PokeAPI returns, which is what puts dex
numbers in the sidebar for free and gives the generation filter something to
work from. Ordering by base-stat total would mean those same ~1300 fetches for a
single keypress.

### Alternate forms

Forms such as `raichu-alola` resolve their species and evolution data via the
base species name carried in the Pokémon payload, so they do not 404. Their ids
sit above 10000 and carry no dex meaning, so they show a blank dex column and
are excluded from `dex:` and `gen:` filters rather than being guessed at.

### Evolution requirements

PokeAPI's `evolution_details` are parsed into a structured `EvolutionCondition`
— level, item, held item, known move, happiness, affection, beauty, time of day,
location, gender, trade species, party species, relative physical stats, and the
one-off flags — and phrased through per-language templates, so each translation
decides where the value lands (`Use {}` versus `{} kullan`). Where a stage can
be reached more than one way, the first (current-generation) route is shown.

### Localization

UI strings live in `i18n.rs`; add a language by extending the `Language` enum
and `Language::ALL`, and adding a `Strings` table. Because the renderer re-reads
these every frame, switching language updates the entire interface instantly
with no extra bookkeeping.

Pokédex flavour text, genus, and ability names and descriptions come from
PokeAPI in `en`, `de`, `fr`, `es` and `it`. Ability text is taken from the game
flavour entries rather than the effect entries: PokeAPI carries flavour in all
five of those languages, while effect text exists only in English, German and
French.

Item, move and location names inside evolution requirements stay in English —
they arrive as PokeAPI slugs, and localizing each would cost an extra request
per name.

For a UI language PokeAPI has no text for (Turkish), the English blurb is
translated on demand through the free, key-less
[MyMemory](https://mymemory.translated.net/) API and cached. This is
best-effort: if the service errors or rate-limits, the English original is
shown.

### Dependencies

[`ratatui`](https://crates.io/crates/ratatui) ·
[`crossterm`](https://crates.io/crates/crossterm) ·
[`clap`](https://crates.io/crates/clap) ·
[`tokio`](https://crates.io/crates/tokio) ·
[`reqwest`](https://crates.io/crates/reqwest) ·
[`serde`](https://crates.io/crates/serde) ·
[`serde_json`](https://crates.io/crates/serde_json) ·
[`image`](https://crates.io/crates/image) ·
[`futures`](https://crates.io/crates/futures) ·
[`anyhow`](https://crates.io/crates/anyhow) ·
[`thiserror`](https://crates.io/crates/thiserror)

---

## Development

The checks CI enforces on every pull request, in the order it runs them:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Tests run on Linux, macOS and Windows. A separate job builds against the
`rust-version` floor declared in `Cargo.toml`, so a dependency bump that raises
the minimum supported Rust version fails the build rather than reaching users.
`cargo deny` checks licenses and RUSTSEC advisories weekly and whenever the
manifest changes.

Everything under `typechart.rs`, `team.rs`, `query.rs` and `models.rs` is pure
and unit-tested; new logic belongs there rather than in `app.rs` or `ui.rs`
wherever it can be expressed without a terminal or a network.

## Credits

- Data and sprites: [**PokeAPI**](https://pokeapi.co/). Please respect their
  [fair-use policy](https://pokeapi.co/docs/v2) — the on-disk cache exists partly
  so this client asks for each resource once and never again.
- Translation fallback: [**MyMemory**](https://mymemory.translated.net/).
- Pokémon is © Nintendo / Game Freak / The Pokémon Company. This is a
  non-commercial, educational project.

## License

MIT — see [`LICENSE`](LICENSE).
