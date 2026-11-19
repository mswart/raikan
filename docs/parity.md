# Port to the target architecture: parity record

Status of the first milestone of [architecture.md](architecture.md): today's feature set
(H-Group levels 1–3 and the partial level 5/6/7/9 behaviour described in the
[README](../README.md)) runs on the new layers (`domain`, `variants`, `engine`,
`knowledge`, `conventions`, `policy`, `agent`, `adapters`). The legacy implementation
(`game`, `hyphenated`) is still in the tree as the reference.

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

## Reproduce

```sh
cargo build --release
target/release/raikan stats --players 4 --games 10000 --bot legacy > legacy.list
target/release/raikan stats --players 4 --games 10000 --bot new > new.list
target/release/raikan diff legacy.list new.list          # first regression and the new view
target/release/raikan lockstep --players 4 --games 100   # first diverging decision
RAIKAN_LOCKSTEP_NOTES=1 target/release/raikan lockstep --players 4 --games 100  # first diverging note
```

## What is in place

| Architecture section | State |
| --- | --- |
| §3 layers | Modules exist; `tests/layering.rs` enforces the import direction. |
| §4 rules layer | Done: variant catalogue (hanab.live, pinned), touch tables, `GameState`/`World`/`Event` with redactable identities, simulator, replay JSON/URL. Orange, up-or-down, hidden-information, sudoku, alternating clues and reversed suits report `Unsupported`. |
| §5 knowledge | Done: sight, literal information, card counting, constraint store, exact sampler. The fallback sampler for more than 16 constrained cards is missing (not needed by any supported variant). |
| §6 conventions | Rule manifest with levels, prerequisites and coverage; per-game `Profile` with digest; per-viewpoint `View` folding records into notes with provenance; Occam tiers, reactions, strike recovery. **Gaps:** reads are not routed through a logged `InterpretCtx`, so there is no interpretation cache and no read-keyed memoisation; precedence is the fixed order of the clue reader, not a data table; rules are gated by the profile but live as methods on `View`, not as `Convention` trait objects. |
| §7 action choice | Blueprint with named reasons and the level-7 look-ahead. The root planner, partner model and legibility guard as specified are the next goal. |
| §8 trace | Interpretation trace (rule and readings per observed clue) and decision trace (action, reason, profile digest) as JSON lines: `raikan selfplay SEED --trace FILE`. No `explain` yet. |
| §9 configuration | `Config { max_level, require_complete }`; `--level` in the CLI, `RAIKAN_LEVEL` in the web client. No TOML yet. |
| §10 testing | Text scenarios (`tests/scenarios/`), legacy equivalence, engine equivalence, hidden-card invariance, level-cap scenarios. Most legacy unit tests are covered by the equivalence test, not yet by rule-keyed scenarios. |

## Before deleting the legacy code (milestone M5)

The equivalence tests use the legacy bot as the reference. Before deleting it:

1. Port the remaining legacy unit tests in `src/hyphenated/*` and `tests/line_tests.rs`
   to scenarios. Their intent is covered by the equivalence test today.
2. Decide whether to keep a frozen corpus of the legacy decisions as golden files.
