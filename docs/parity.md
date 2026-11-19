# Port to the target architecture: parity record

Status of the first milestone of [architecture.md](architecture.md): today's feature set
(H-Group levels 1–3 and the partial level 5/6/7/9 behaviour described in the
[README](../README.md)) runs on the new layers (`domain`, `variants`, `engine`,
`knowledge`, `conventions`, `policy`, `agent`, `adapters`). The legacy implementation
(`game`, `hyphenated`) was removed afterwards (milestone M5, commit after `10b36c9`); the
measurements and commands below are from before the removal and need a checkout of
`10b36c9` to reproduce.

## Results (2026-11-19)

10,000 paired seeds (`0..10000`), No Variant, every seat the same bot. "Differing" counts
games whose result line (outcome, score, turns, blind plays, replay) is not identical.

| Players | Legacy mean | New mean | Strikeouts legacy → new | Invalid | Differing games |
| --- | --- | --- | --- | --- | --- |
| 2 | 21.378 | 21.378 | 24 → 24 | 0 → 0 | 0 |
| 3 | 22.973 | 22.973 | 48 → 48 | 0 → 0 | 0 |
| 4 | 22.843 | 22.843 | 77 → 77 | 0 → 0 | 1 |
| 5 | 21.285 | 21.315 | 210 → 214 | 0 → 0 | 3353 |

For 2–4 players the new bot reproduces the legacy bot exactly: on 6,000 games
(`RAIKAN_EQUIV_GAMES=2000 cargo test --release --test convention_equivalence`) every
seat's notes (beliefs, play, trash, clued, delay, promise, 1s-order and lock flags) and
every decision are equal at every event.

The 5-player differences come from a legacy bug. The old `Line` stamps cards with a turn
counter that starts at −16 and stops at 0 during the deal, so with 20 starting cards
several cards share a stamp, and the "everybody knows this card" lookup
`locked = (player, turn)` can point at the wrong card. The new code identifies cards by
deck order. With the legacy counter widened (start at −32), only 28 of 10,000 5-player
games differ and the means are equal (21.315 both). The paired difference
(+0.029 ± 0.028) is within noise; the 4 extra strikeouts (214 vs 210 of 10,000) are as
well.

Cost: about 160 µs per turn (all seats updating, 4 players) against 119 µs for the
legacy bot (`cargo bench --bench simulation -- 200 4 --bot new|legacy`). Both bots
produce identical replays on those 200 games (same checksum).

## Regression protection after the removal

- [tests/golden_selfplay.rs](../tests/golden_selfplay.rs) fixes the complete replays of
  20 seeded games per player count (2–5). The corpus was recorded from the new bot, whose
  replays equalled the legacy bot's for 2–4 players (checked on 60 seeds per count before
  the removal).
- `raikan stats` and `raikan diff OLD.list NEW.list` compare two builds.

## Changes after the port (2026-11-20)

Review findings fixed after the port, with their measured effect (10,000 paired seeds
No Variant, 5,000 for Rainbow (6 Suits); mean score before → after, strikeouts):

- **Teammate predictions no longer use cards the teammate cannot see.** `perspective()`
  used to clone our notes, which include deductions from the teammate's own cards. The
  agent now keeps one view per seat, fed only what that seat observes; our own cards are
  added where our notes identify them, and as possibility hints otherwise (the teammate
  sees them). `tests/bot_selfplay.rs` swaps a teammate's hidden card and requires equal
  predictions while that teammate's observations and our own beliefs are equal.

  | Players | Before | After | Strikeouts |
  | --- | --- | --- | --- |
  | 2 | 21.378 | 21.419 (+0.042 ± 0.010) | 24 → 6 |
  | 3 | 22.973 | 23.051 (+0.077 ± 0.020) | 48 → 19 |
  | 4 | 22.843 | 22.987 (+0.144 ± 0.025) | 77 → 14 |
  | 5 | 21.315 | 21.649 (+0.334 ± 0.037) | 214 → 45 |
  | Rainbow 3 | 26.801 | 27.054 (+0.253 ± 0.054) | 77 → 28 |
  | Rainbow 4 | 26.420 | 26.966 (+0.546 ± 0.069) | 134 → 35 |

  The bot is about 1.5× slower (every event is also interpreted in the teammates' views).
  The receiver check of our own clues now runs the complete clue reader on the
  receiver's view from before the clue.
- **Sampler:** convention-weighted probabilities were wrong when a convention restricted
  an otherwise unconstrained card; both counting tables now cover the same cards.
- **Coverage:** every level 1–20 convention is catalogued; profiles report missing ones.
- **Variants:** Color Blind and Number Blind (empty clues) are reported as unsupported.

## What is in place

| Architecture section | State |
| --- | --- |
| §3 layers | Modules exist; `tests/layering.rs` enforces the import direction. |
| §4 rules layer | Done: variant catalogue (hanab.live, pinned), touch tables, `GameState`/`World`/`Event` with redactable identities, simulator, replay JSON/URL. Orange, up-or-down, hidden-information, sudoku, alternating clues and reversed suits report `Unsupported`. |
| §5 knowledge | Done: sight, literal information, card counting, constraint store, exact sampler. The fallback sampler for more than 16 constrained cards is missing (not needed by any supported variant). |
| §6 conventions | Rule manifest with levels, prerequisites and coverage, plus a catalogue of every level 1–20 convention including missing ones ([support-matrix.md](support-matrix.md)); per-game `Profile` with digest; per-viewpoint `View` folding records into notes with provenance; Occam tiers, reactions, strike recovery; the agent keeps one view per teammate fed only that teammate's observations (see below). **Gaps:** teammate views know only the root seat's cards its notes identify, plus possibility hints for the others, and their own predictions of third seats still use the depth-2 approximation (notes with the seat's cards hidden); reads are not routed through a logged `InterpretCtx`, so there is no interpretation cache and no read-keyed memoisation; precedence is the fixed order of the clue reader, not a data table; rules are gated by the profile but live as methods on `View`, not as `Convention` trait objects. |
| §7 action choice | Blueprint with named reasons and the level-7 look-ahead. The root planner, partner model and legibility guard as specified are the next goal. |
| §8 trace | Interpretation trace (rule and readings per observed clue) and decision trace (action, reason, profile digest) as JSON lines: `raikan selfplay SEED --trace FILE`. No `explain` yet. |
| §9 configuration | `Config { max_level, require_complete }`; `--level` in the CLI, `RAIKAN_LEVEL` in the web client. No TOML yet. |
| §10 testing | Text scenarios (`tests/scenarios/`), golden self-play corpus, hidden-card invariance, level-cap scenarios. Most legacy unit tests were covered by the (now removed) equivalence test, not by rule-keyed scenarios; porting their cases to scenarios is open. |
