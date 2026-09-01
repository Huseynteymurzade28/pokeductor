# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

`0.1.0` and `0.2.0` predate this file and are reconstructed from the commit
history, so they are summaries rather than the record kept as the work landed.
Tagging began at `0.3.0`, so those two link commit ranges rather than tags.

## [Unreleased]

### Added

- A moves card, on `M`: the learnset for the newest games a species appears in,
  with each move's type, category, power, accuracy and PP, and a description of
  the one under the cursor. The rows come with the species record, so the card
  costs no request to open; the per-move records are fetched as you scroll and
  cached permanently.
- Two more search filters: `ability:levitate` narrows the list to everything
  that can have an ability, and `egg:dragon` to a breeding group. Both combine
  with `type:` and with each other, and both accept the short forms `a:` and
  `e:`. Breeding groups answer to their in-game names as well as PokeAPI's
  older spellings, so `egg:grass` reaches the group the API files as `plant`.
- A command-line interface: `pokeductor <name>` opens straight on a species,
  `--lang` picks the interface language, `--clear-cache` and `--cache-dir`
  manage the on-disk cache, and `--version` and `--help` answer at last. The
  name argument goes through the search box, so `pokeductor 25` and
  `pokeductor type:ghost` work too. (#5)
- Terminal colour-depth detection. Sprites quantize to the xterm-256 palette on
  terminals without truecolor instead of being dropped, and `--color` and
  `NO_COLOR` override what detection concluded. (#4)
- The party, interface language, sort order and shiny toggle are carried over
  between runs, kept under `$XDG_STATE_HOME/pokeductor`.

### Changed

- The on-disk cache format is version 5: species records now carry their
  learnset, so entries written by an earlier build are re-fetched once.
- `reqwest` now uses `rustls-tls` rather than the platform's TLS. Release
  binaries link no system OpenSSL and carry their own root certificates, which
  is what lets one Linux build run anywhere.
- With no colour available, sprites are skipped in favour of the placeholder the
  panels already fall back to, and the list cursor falls back to reverse video.

### Fixed

- The type matchup card ignored the ability immunities the party card already
  accounted for, so the two disagreed about the same species. (#13)

### Performance

- An idle screen no longer redraws eight times a second. The animation tick runs
  only while a request is in flight, which is the only time a spinner is on
  screen: measured idle CPU goes from ~1.1% of a core to 0.00%. (#15)
- A sprite's crop box is worked out once when it is decoded rather than scanned
  out of all 96×96 pixels on every frame, halving the sprite work in a frame
  that draws a full evolution chain. (#16)

## [0.3.1] - 2026-08-21

### Fixed

- Updated `h2` past the empty-DATA-frame advisory.
- Dex range assertions no longer depend on their ranges being single-element.

## [0.3.0] - 2026-08-21

### Added

- An app-wide shiny artwork toggle behind `X`, fetching and caching only the
  palette on screen. (#10)
- Dex-number search: a bare number finds that Pokédex entry, and `dex:` takes a
  number or a range. (#18)
- CI covering formatting, lint, tests on three platforms, and the MSRV. (#1)
- The AUR package documented in the install section.

### Fixed

- Evolution cards stayed blank for species whose default form is named
  differently; the default variety is now resolved before artwork is
  requested. (#12)
- A species recorded as having no artwork was written off permanently. That
  answer now expires, since sprites get backfilled for newly added
  species. (#14)

### Changed

- The API client gained timeouts, bounded retries and a concurrency cap, so a
  slow or flaky network degrades instead of hanging. (#2)
- Updated `anyhow` past the `downcast_mut` unsoundness advisory.

## [0.2.0] - 2026-08-12

### Added

- Offline type-effectiveness analysis: a built-in Generation VI+ chart, and a
  matchup card that opens instantly with no request.
- A party builder with team-wide type analysis — shared weaknesses, unresisted
  types, ability immunities and offensive gaps.
- An abilities card, with descriptions fetched on demand and cached.
- Search filters (`type:`, `gen:`) and a sortable sidebar.
- Evolution requirements spelled out per stage, sized to the panel.
- An on-disk cache of every fetched response, for instant and offline starts.
- A help overlay behind `?`.

### Changed

- The app opens on the first species instead of an empty panel.
- The README rewritten around the interface as it actually is.

## [0.1.0] - 2026-06-16

### Added

- The first release: a terminal Pokédex over PokeAPI, with sprite rendering as
  Unicode half-blocks, evolution chains as connected cards, a language menu, and
  the retro colour palette.

[Unreleased]: https://github.com/Huseynteymurzade28/pokeductor/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/Huseynteymurzade28/pokeductor/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Huseynteymurzade28/pokeductor/compare/77c1e8d...v0.3.0
[0.2.0]: https://github.com/Huseynteymurzade28/pokeductor/compare/94eadb8...77c1e8d
[0.1.0]: https://github.com/Huseynteymurzade28/pokeductor/compare/a42a22b...94eadb8
