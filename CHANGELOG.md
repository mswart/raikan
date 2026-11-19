# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/mswart/raikan/compare/v0.3.0...v0.3.1) - 2026-11-19

### Fixed

- *(deps)* update rust
- *(deps)* fix clippy warning around slog-term

### Other

- *(deps)* Update code to work with newer deps

## [0.3.0](https://github.com/mswart/raikan/compare/v0.2.2...v0.3.0) - 2026-11-19

This release restarts hopefully more active development on raikan. The goal is to implement more levels and the first variants.

### Added

- *(h-group)* Implement level 1 to 3 completely
- *(h-group)* Rework finess clue interpretation; this involved fixing various issues around finesses.
- *(h-group)* Implement stalling with locked-hand and eight-clue saves
- Add turn-aware action selection
- Add a deterministic simulation benchmark with replay checksums and optional
  allocation counting, plus comparisons against exhaustive action evaluation.

### Fixed

- *(bot)* Adapt to changed server protocol
- *(rules)* Deal four cards per player in five-player games and replays, matching the
  game engine's hand layout.
- *(h-group)* Prioritize safe fixes before ordinary plays, recognize duplicate rank-play
  clues for discard recovery, and use the same ordering and discard signals in
  action forecasts. Preserve literal evidence when correcting information locks.
- *(h-group)* Remove blocked ranks from good-touch deductions, preserve literal information,
  and avoid reserving identities from unprotected cards.
- *(utils*) Fix hand removal at the end of the five-player slot array.

### Performance

- Reduce simulation allocations with shared immutable clue history, inline
  connection chains and scratch buffers, and compact card-possibility bitsets.
- Reuse clue interpretations, skip provably dominated forecasts, and build
  later player views only when the search reaches them. Enable release LTO.

### Other

- *(deps)* update rust crate serde to v1.0.229 ([#116](https://github.com/mswart/raikan/pull/116))
- *(deps)* update rust crate rand to v0.8.8 ([#113](https://github.com/mswart/raikan/pull/113))

### Fixed

## [0.2.2](https://github.com/mswart/raikan/compare/v0.2.1...v0.2.2) - 2025-09-26

### Other

- update Cargo.toml dependencies

## [0.2.1](https://github.com/mswart/raikan/compare/v0.2.0...v0.2.1) - 2025-09-24

### Fixed

- *(webclient)* replace websocket and native-tls

## [0.2.0](https://github.com/mswart/raikan/releases/tag/v0.2.0) - 2025-09-14

Initial public release.
