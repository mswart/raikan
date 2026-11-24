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

## Root planner and legibility guard (2026-11-20)

`src/search/` implements the first planner of architecture §7.4 behind `--search`
(`RAIKAN_SEARCH` in the web client); the default stays the blueprint, so the golden corpus
is unchanged. `src/policy/guard.rs` is the legibility guard of §7.5: a clue candidate is
simulated only if `View::clue_views` grades it with no more convention errors than the
position already has (every teammate reads it in its own kept view, the seats in between
act on their obligations, nobody's next play misplays). The blueprint's own choice is
always kept as the reference. Value weights were picked by paired self-play on seeds
0..160 (3 players) among a handful of settings.

Fresh seeds `1000..1320`, No Variant, `fast` (300 simulations, 48 worlds, margin 1.0):

| Players | Blueprint | Search, no guard | Search + guard | Strikeouts (bp / no guard / guard) | Strikes (bp / no guard / guard) |
| --- | --- | --- | --- | --- | --- |
| 2 | 21.372 | 22.250 | 22.834 | 0 / 4 / 0 | 39 / 149 / 28 |
| 3 | 23.159 | 23.359 | 23.837 | 0 / 3 / 0 | 34 / 127 / 12 |
| 4 | 22.978 | 23.191 | 23.934 | 1 / 0 / 0 | 53 / 116 / 7 |
| 5 | 21.719 | 21.384 | 22.869 | 1 / 4 / 0 | 61 / 149 / 16 |

Rainbow (6 Suits), 3 players, seeds `1000..1160`: blueprint 27.144 (2 strikeouts, 45
strikes) → search + guard 28.512 (0 strikeouts, 25 strikes).

Clue purpose, end-game rollouts and `min_gain` (2026-11-24), seeds `0..300`, No Variant,
`fast`, paired by seed against the previous search (3 players 23.970, 4 players 24.030):

| Weights | min_gain | 3 players | 4 players | Overrides (3 / 4) |
| --- | --- | --- | --- | --- |
| hand-set | 0 | +0.06 ± 0.04 | −0.01 ± 0.04 | 18.8% / 21.3% |
| hand-set | 0.1 | +0.12 ± 0.06 | −0.05 ± 0.05 | 16.7% / 19.7% |
| hand-set | **0.25** (default) | +0.18 ± 0.07 | +0.06 ± 0.08 | 12.3% / 14.6% |
| fitted | 0 | +0.11 ± 0.08 | −0.21 ± 0.09 | 16.4% / 18.4% |
| fitted | 0.1 | +0.00 ± 0.09 | −0.05 ± 0.08 | 12.5% / 15.5% |
| fitted | 0.25 | +0.13 ± 0.09 | −0.08 ± 0.08 | 8.8% / 10.8% |

The fitted weights (4000 blueprint games per player count) predict the points still to
come far better (held-out RMSE 1.96 against 5.35 for 3 players) but decide worse, so
`data/value-weights.json` ships empty.

Standard errors of the means are 0.07–0.13 (0.29 and 0.13 for Rainbow). Without the guard, search simulated clues
that teammates misread and paid for them in strikes; with it, search beats the blueprint
at every player count and strikes less.

Cost: about 1 s CPU per 2-player game, 4 s for 3 players, 10 s for 4 and 19 s for 5
(blueprint: about 10 ms per game). Not done: the guard does not enumerate an in-between
seat's free choices (§7.5, "top expected actions"), the partner model has no spread,
worlds are evaluated on one thread, there are no time budgets, and the value weights
are not fitted.

## What is in place

| Architecture section | State |
| --- | --- |
| §3 layers | Modules exist; `tests/layering.rs` enforces the import direction. |
| §4 rules layer | Done: variant catalogue (hanab.live, pinned), touch tables, `GameState`/`World`/`Event` with redactable identities, simulator, replay JSON/URL. Stacks are (played, top, start, direction) and the variant derives every line a stack can still be built along: reversed suits, Up or Down with START cards, Sudoku (unique starts, wrapping). Orange suits turn a play command into a discard and back (`engine::step`); replays record the effect (`step_recorded`). Alternating clues and empty clues (Color Blind, Number Blind). All 2428 catalogue variants but the hidden-information ones load and self-play without illegal moves. **Gaps:** hidden-information variants (Throw It in a Hole, Cow & Pig, Duck) report `Unsupported`; hanab.live also ends a game once the (recomputed) maximum score is reached or no card can be played any more, the engine only at the variant's maximum; the last-card "deck play" of orange variants. The conventions cover none of the new mechanics (`convention_support`). |
| §5 knowledge | Done: sight, literal information, card counting, constraint store, exact sampler. The fallback sampler for more than 16 constrained cards is missing (not needed by any supported variant). |
| §6 conventions | Rule manifest with levels, prerequisites and coverage, plus a catalogue of every level 1–20 convention including missing ones ([support-matrix.md](support-matrix.md)); per-game `Profile` with digest; per-viewpoint `View` folding records into notes with provenance; Occam tiers, reactions, strike recovery; the agent keeps one view per teammate fed only that teammate's observations (see below). **Gaps:** teammate views know only the root seat's cards its notes identify, plus possibility hints for the others, and their own predictions of third seats still use the depth-2 approximation (notes with the seat's cards hidden); reads are not routed through a logged `InterpretCtx`, so there is no interpretation cache and no read-keyed memoisation; precedence is the fixed order of the clue reader, not a data table; rules are gated by the profile but live as methods on `View`, not as `Convention` trait objects. |
| §7 action choice | Blueprint with named reasons and the level-7 look-ahead (default). Opt-in root planner (`--search fast\|full`, see below): exact world sampling from the view's constraint store, sequential halving over all blueprint candidates on common worlds, partner model on each seat's own view (convention obligations with certainty, otherwise the blueprint's choice with an optional spread over its best alternatives, drawn on common random streams), named-term leaf value with hand-set weights (`raikan fit-value` fits them per player count against blueprint games, sign-constrained least squares; opt-in, see below), rollouts to the real end in the end-game (pace below the number of players), blueprint kept unless beaten by `blueprint_margin` paired standard errors and by `min_gain` expected points. Clues must have a purpose (`ClueIntent`: a new card clued, promised or chop moved, a called-for stall, a valuable tempo clue or a fix), which also names them in traces, `explain` and the web log. Search candidates add plays of literally known playable cards and discards of known trash from any slot. Legibility guard (§7.5) over the same clue reader (`policy::guard`), with the free choice of every seat between us and the receiver (the clue the blueprint predicts from its view) checked as a second branch. Worlds on several threads (results independent of the count), optional time limit. **Gaps:** the value fit is observational (blueprint games), not causal; weighting worlds by the likelihood of observed teammate actions; the guard's prediction of a seat in between does not know our unidentified cards (its forecast stops there). |
| §8 trace | Interpretation trace (rule and readings per observed clue) and decision trace (action, reason, profile digest, planner table) as JSON lines: `raikan selfplay SEED --trace FILE`. `raikan explain REPLAY --turn T` (REPLAY: a file, a URL, a hanab.live game id or `seed:N`) recomputes a decision: position with notes, blueprint/bot/played action, a comparison of candidates and requested actions on common worlds (guard verdict, each seat's reading of a clue, value terms, paired difference), rollouts in one world (`--world`), a card's note history with the rules that changed it (`--card`), other levels (`--level`), JSON (`--json`). **Gaps:** card provenance is reconstructed by replaying events, not stored record ids on notes; no web UI or recompute endpoint; a forced action is followed for one rollout horizon, not replayed to the end of the game. |
| §9 configuration | `Config { max_level, require_complete }`; `--level` in the CLI, `RAIKAN_LEVEL` in the web client. No TOML yet. |
| §10 testing | Text scenarios (`tests/scenarios/`), golden self-play corpus, hidden-card invariance, level-cap scenarios. Most legacy unit tests were covered by the (now removed) equivalence test, not by rule-keyed scenarios; porting their cases to scenarios is open. |

## Partner model, free choices and parallel worlds (2026-11-20)

The guard now also branches on the clue an in-between seat would give instead of
discarding (§7.5), and search candidates include known plays and known trash from any
slot. Same seeds `1000..1320`, No Variant, `fast`:

| Players | Search + guard (before) | + free choices, extra candidates | + partner spread 0.1 (paired to spread 0) |
| --- | --- | --- | --- |
| 3 | 23.837 (12 strikes) | 23.947 (12 strikes) | +0.056 ± 0.060 (11 strikes) |
| 4 | 23.934 (7 strikes) | 24.016 (8 strikes) | −0.056 ± 0.062 (5 strikes) |

The free-choice branch changes little (within one standard error). A partner spread of
0.1 has no measurable effect, so the default stays 0 (`--spread` to experiment). CPU per
game with one thread per decision: about 3.5 s (3 players) and 8.3 s (4 players);
`--threads` spreads one decision over cores with identical results.

