# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Reduce simulation allocations with shared immutable clue history, inline
  connection chains and scratch buffers, and compact card-possibility bitsets.
- Reuse clue interpretations, skip provably dominated forecasts, and build
  later player views only when the search reaches them. Enable release LTO.
- Add a deterministic simulation benchmark with replay checksums and optional
  allocation counting, plus comparisons against exhaustive action evaluation.

### Fixed

- Deal four cards per player in five-player games and replays, matching the
  game engine's hand layout.

- Complete standard-game level-2 connection handling, including prompt
  confirmation, declined connections, and mixed chains of up to four links.
- Recover wrong prompts using the original finesse position, including subsequent
  links; otherwise withdraw deductions since the mistaken clue without losing
  literal evidence or earlier saves.
- Fix hand removal at the end of the five-player slot array.
- Add level-2 regression coverage across player counts and seat rotations, and
  enable three replay tests with current convention expectations and an actual
  prompt-misplay recovery scenario.
- Apply level-1 clue focus consistently, including implicit clues on finessed
  cards, and protect those cards when selecting chop.
- Support double-chop 2 saves while preserving the visible rule in each player's
  view; retain playable alternatives in ambiguous critical saves.
- Remove blocked ranks from good-touch deductions, preserve literal information,
  and avoid reserving identities from unprotected cards.
- Enforce minimum clue value in action selection while retaining fixes, stalls,
  and re-clues that introduce new connections.
- Add level-1 regression coverage and enable six previously ignored replay tests.

## [0.2.2](https://github.com/mswart/raikan/compare/v0.2.1...v0.2.2) - 2025-09-26

### Other

- update Cargo.toml dependencies

## [0.2.1](https://github.com/mswart/raikan/compare/v0.2.0...v0.2.1) - 2025-09-24

### Fixed

- *(webclient)* replace websocket and native-tls

## [0.2.0](https://github.com/mswart/raikan/releases/tag/v0.2.0) - 2025-09-14

Initial public release.
