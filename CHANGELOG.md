# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

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
