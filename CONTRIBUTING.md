# Contributing

Thanks for looking. This is a short file on purpose: it says what CI will hold
a change to, where a change belongs, and the three habits the code has that
are not obvious from reading any one file of it.

## Running it

```bash
cargo run                 # the TUI
cargo run -- gengar       # opened on a species
cargo run -- --help       # the flags
```

Every request is cached under the directory `pokeductor --cache-dir` prints,
so a second run of anything is offline. `--clear-cache` empties it.

## What CI runs

Exactly these, in this order, on every pull request
(`.github/workflows/ci.yml`):

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Run them before pushing and there is nothing left for CI to find. Tests run
on Linux, macOS and Windows; a change that reaches for a platform-specific
path or terminal feature should expect the other two to notice.

A fourth job builds against the minimum supported Rust version, which is the
`rust-version` line in `Cargo.toml` (1.88 at the time of writing). Bumping it
is fine when a dependency needs it — say so in the commit, and update the
comment beside it that says why.

There is one ignored test, `api::tests::the_live_api_still_answers`, which
hits pokeapi.co for real. Run it deliberately after touching `api.rs`:

```bash
cargo test --all-features -- --ignored --nocapture
```

`cargo deny` checks licences and advisories on a schedule and whenever the
manifest changes; a new dependency has to clear `deny.toml`.

## Where a change belongs

The README's [Architecture](README.md#architecture) table names every module
and what it is for. The short version: anything that can be expressed without
a terminal or a network — a rule about types, a parser, a piece of arithmetic
— goes in a pure module (`typechart.rs`, `team.rs`, `compare.rs`, `query.rs`,
`models.rs`) with tests beside it, and `app.rs` and `ui.rs` only call it.
Those two files are the largest in the repo and the hardest to test, so logic
that lands in them tends to stay unverified.

Every user-facing string lives in `i18n.rs`, in all six languages. The
`Strings` struct is a struct rather than a map so that a missing translation
is a compile error rather than an English label in a Turkish interface.

Anything cached on disk has a version constant (`cache::VERSION`,
`session::VERSION`). Changing the shape of a stored record means bumping it;
the old files are then read as misses rather than parsed into the wrong
struct.

## House style

Three habits, each with an example lifted from the code as it is.

### Comments say why, not what

A comment restating the line under it is noise; a comment saying why the line
is that way and not another is the only thing a future reader cannot get from
the code. From `src/cache.rs`:

```rust
/// How long the master species list stays fresh. Individual records never
/// expire — PokeAPI does not rewrite history, it only appends new species, and
/// those show up when the list is refreshed.
const LIST_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
```

The number is obvious. That records are permanent, and why that is safe, is
not.

### Tests are named as the rule they defend

A test name is a sentence stating what is true, so a failure reads as "this
is no longer true" rather than as "test_filter_3 failed". From `src/query.rs`,
`src/session.rs` and `src/app.rs`:

```rust
fn unrecognised_terms_stay_searchable_text()
fn a_file_from_another_version_reads_as_a_fresh_session()
fn the_comparison_key_pins_then_compares_then_lets_go()
```

One rule per test. When a test needs a comment, it is usually to say why the
rule is the right one, in the same spirit as the comments above.

### Commits describe the problem before the change

The subject is a sentence in the imperative saying what the commit does. The
body opens with what was wrong or missing and why it mattered, and only then
says what changed and why that way. Someone reading `git log` in a year
should be able to tell whether the reasoning still holds. From the history:

```
Stop redrawing an idle screen eight times a second

The event loop drove a 120 ms ticker unconditionally, and every tick redrew
the whole interface — sidebar, info panel, sprite, evolution graph. That is
roughly eight full renders a second, forever, including on a completely idle
screen where nothing is loading and nothing is animated. A Pokedex is the
kind of thing left open in a split for hours, so it burned CPU and laptop
battery to animate a spinner that was not on screen.

The spinner is the only thing the tick exists for, and the app already knows
when one is up: [...] `is_busy` is their union, and the ticker is now a
conditional branch of the `select!` rather than an unconditional one. Idle,
the loop blocks on input and messages alone and draws when one of them says
something changed.
```

That one goes on to say how it was measured, which is worth the lines
whenever a change claims to make something faster.

A commit that closes an issue ends with `Closes #N`.

## Changelog

User-visible changes get a line under `Unreleased` in
[`CHANGELOG.md`](CHANGELOG.md) as they land, with the issue number where
there is one. The release process moves that section under a version and is
described in the README under [Releasing](README.md#releasing).
