# Raikan - Hanabi bot (H-Group rules) for hanab.live

![Crates.io Version](https://img.shields.io/crates/v/raikan)
![Crates.io MSRV](https://img.shields.io/crates/msrv/raikan)
![Crates.io License](https://img.shields.io/crates/l/raikan)

Raikan is a Hanabi game implementation (and utils) and implementation of [H-Group rules](https://hanabi.github.io).
It aims to be a usable bot to use on [hanab.live](https://hanab.live).

## Use a binary/cli

`raikan-web` is a CLI tool to connect to the [live game environment hanabi.live](https://hanab.live).
It can join games and be a bot to play with more players than available peoples.

You can either compile the project yourself (enable `webclient` feature) or use the binaries from releases.

Create the bot's account on [hanab.live](https://hanab.live) first, then set
`HANABI_USERNAME` and `HANABI_PASSWORD` to its credentials. The bot logs in as
`client=bot`; it does not create accounts automatically. Login failures report
the HTTP status and the server's explanation.

Alternatively, set `HANABI_SID` to an existing cookie in the form
`hanabi.sid=<session value>`. This skips password login; unset it to use the
username and password again. Do not include `Set-Cookie` attributes such as
`Path` or `HttpOnly`.

Send the bot a DM with `/join` as message to make it join your table.
The web client supports one table at a time. It plays variants whose suits the
conventions cover: plain suits (`No Variant`, 3–6 suits) and rainbow (e.g.
`Rainbow (6 Suits)`, `Rainbow (5 Suits)`). Other special suits (white, black,
pink, …) and variant modifiers are refused unless
`RAIKAN_EXPERIMENTAL_VARIANTS=1` is set. `RAIKAN_LEVEL=<n>` caps the
H-Group level (default 9); `RAIKAN_SEARCH=fast|full` turns on the search (default off,
see below). It uses Hanabi Live's current lobby and game-event protocol; spectators
and replay viewers do not send moves.

The client follows the [November 14, 2026 protocol contracts](https://github.com/Hanabi-Live/hanabi-live/tree/1701eb99677f/server/protocol/src).
WebSocket requests use JSON frames with request IDs and correlated replies.
The client loads game setup after `tableJoined`, applies each batch of live
events before choosing a move, and replaces its history with the complete
replay when `gameFinished` arrives. Paused games wait for a resume notification.

## H-Group rule support

[docs/support-matrix.md](docs/support-matrix.md) lists every convention of levels 1–20
with its coverage (complete, partial or missing); a profile compiled with
`require_complete` refuses levels with gaps.

Support is partial and varies by convention. This overview covers levels 1–9,
ending at stalling, the highest level with implemented special moves. It does not
mean that all earlier levels are complete. Variant support is listed under
[Variants](#variants).

| H-Group level | Status |
| --- | --- |
| [1 — Fundamentals](https://hanabi.github.io/level-1) | Implemented for the standard five-suit game |
| [2 — Basic Moves](https://hanabi.github.io/level-2) | Implemented for the standard five-suit game |
| [3 — Basic Strategy](https://hanabi.github.io/level-3) | Implemented for the standard five-suit game |
| [4 — Chop Moves](https://hanabi.github.io/level-4) | Not implemented |
| [5 — Intermediate Finesses](https://hanabi.github.io/level-5) | Partial |
| [6 — Tempo Clues](https://hanabi.github.io/level-6) | Partial |
| [7 — Emergency Discards](https://hanabi.github.io/level-7) | Limited tactical support; discard signals missing |
| [8 — End-Game](https://hanabi.github.io/level-8) | No dedicated convention support |
| [9 — Stalling](https://hanabi.github.io/level-9) | Most stall types implemented; exceptions remain |

### Level 1 — Fundamentals

* **Focus and chop:** Clues focus chop when newly touched, otherwise the leftmost
  new card; clues with no new touches focus the leftmost re-touched card.
  Finessed cards count as already touched and are protected from chop discards.
  Known trash is discarded first without changing which card is chop.
* **Saves:** Rank-2, rank-5, and critical-card saves are recognized on chop.
  The 2-save visible rule includes the double-chop exception and respects what
  each player can see. Ambiguous critical saves retain both playable and save
  identities instead of ordering an unsafe play. Color clues cannot 5-save.
* **Good touch:** Positive and negative literal clues are preserved separately
  from deductions. Clued cards exclude played cards, blocked ranks, and protected
  duplicates. Merely identifying an unclued card does not reserve it for play.
* **Plays and clue value:** Direct and delayed play clues connect through prompts
  and finesses, with prompts taking precedence. Ordinary clues must gain a new
  card; fixes and supported stalls remain available. Early-game play/save clues,
  including double-chop 2 saves, take precedence over a 5 stall or unknown discard.

Scenarios live in [tests/scenarios](tests/scenarios) (one file per rule case). The
advanced connection and recovery limitations described under later levels still apply.

### Level 2 — Basic Moves

* **Basic moves:** Early-game off-chop 5 stalls; double, triple, and quadruple
  prompts and finesses; mixed prompt/finesse chains; reverse and self-finesses.
  Prompts take precedence over blind plays, and original card identities survive
  draws and hand shifts throughout a chain.
* **Ambiguity:** Receivers wait for possible connecting plays, including later
  links on the same hand. Plays, clues, discards, and unrelated plays distinguish
  competing connections. Existing queued plays can take their turn first.
* **Strike recovery:** Withdraw the mistaken clue's instructions and subsequent
  deductions, preserving earlier knowledge and all positive and negative literal
  clues. A wrong prompt instead requests the original finesse position when it
  remains possible; recovery can continue a multi-step chain and never substitutes
  a newly drawn card for a missing original target.
* **General principles:** Played cards, blocked suits, and protected duplicates
  are trash. One-away cards need a valid connection to receive a play clue.
  Cluing an existing finesse must add value to justify spending a token.

Connections are found by the [locator](src/conventions/locator.rs); open readings react
to later plays, discards and clues in [records.rs](src/conventions/records.rs).

### Level 3 — Basic Strategy

* **Playing 1s:** Starting-hand 1s play oldest first; fresh 1s play newest first
  and precede starting-hand 1s. A newly clued chop 1 takes precedence over both.
  Rank-1 clue focus and simulated plays use the same order, with card provenance
  preserved through hand shifts and seat rotations.
* **Fixes and stops:** Literal corrections can break an incorrect inference
  without revealing a full identity. They cancel obsolete connections and do
  not promise plays from ancillary new touches. Repeating a clue with no new
  information on its touched cards can stop the next queued play; only that card
  becomes discardable. Delayed cards retain their ordinary re-clue meaning.
* **Sarcastic discards:** Discarding a clued, useful duplicate transfers its
  identity to compatible clued cards. A unique target is identified immediately;
  ambiguous targets remain a group until later evidence resolves them. Stable
  card IDs prevent a draw from inheriting the signal. Known duplicates can be
  discarded before ordinary plays when tokens permit it; forecasts follow suit.
* **Strategy and information locks:** Fixes take priority over an ordinary play
  when they safely reduce inference errors, including errors that are not yet
  about to misplay. Scoring rewards useful touches and connecting plays while
  charging for strikes and critical losses. Established identities, alternative
  identities, and connecting cards stay constrained until evidence narrows them
  or a direct conflict requires correction; literal evidence remains intact.

These rules live in [rules.rs](src/conventions/rules.rs). The action choice remains bounded; level 3 does not add the dedicated end-game
planning described under level 8.

### Level 4 — Chop Moves

* **Not implemented:** Trash, 5, and order chop moves. There is no persistent
  protection of an unclued card through a chop-move signal. Saving a card with an
  ordinary clue or a stall does not add this convention.

### Level 5 — Intermediate Finesses

* **Implemented:** Layered finesses track playable cards ahead of a connecting
  target, including layered self-finesses. Queued connections reserve their cards
  across turns and hand shifts. Candidate clues are checked for unsafe or
  duplicate layers, and resolved finesse cards participate in good-touch deductions.
* **Multi-colour prompts** (rainbow): a prompt goes to the clued card with the most
  positive clue information, ties to the newest; negative information is ignored.
* **Gaps:** Double prompts in multi-colour variants are not covered. Hidden
  and clandestine finesse cases are not comprehensively covered by the current
  resolver and regression tests; layered-finesse support is not full level-5 support.

### Level 6 — Tempo Clues

* **Implemented:** Re-cluing can request a play from an already-clued card. In
  supported stalling situations, a tempo clue promises only its focus to play,
  shifting that focus past touched cards already known to be playable.
* **Gaps:** No tempo chop moves, general focus-shifting rule outside stalls, or
  dedicated discard-modulation policy. The full distinction between valuable
  tempo clues and low-value tempo clues is not encoded.

### Level 7 — Emergency Discards

* **Implemented:** Action search forecasts other players' plays and forced
  zero-token discards. It can generate a clue token instead of playing, or avoid
  spending the final token, when that prevents a predicted critical-card loss.
* **Gaps:** Scream and shout discards do not communicate a chop move or force the
  receiver to stall. Token generation is a tactical search result, without the
  full generation-discard convention. Forecasts stop at the next clue-giving
  choice or one table rotation; there is no full All 4's Test.

### Level 8 — End-Game

* **Current behavior:** The game engine handles deck exhaustion and the final
  round, but the strategy continues using its ordinary action policy.
* **Not implemented:** Pace-based planning, final-turn-aware search, positional
  discards or misplays, distribution clues, and a dedicated end-game burning
  policy. Burns available through level-9 stall contexts do not provide general
  end-game stalling.

### Level 9 — Stalling

* **Contexts:** Tracks clue tokens, the opening turn, the first unknown discard,
  locked hands, and double-discard danger in every player's view. Misplays and
  known-trash discards do not end the early game.
* **5 stalls:** Gives one early-game 5 stall when needed, preferring the five
  closest to chop. Further early-game 5 stalls are optional and normally skipped.
  Also recognizes 5 stalls during double-discard danger, with a locked hand, or
  with eight clues after the opening turn.
* **Other stalls:** Tempo, fill-in, and burn clues are supported in those latter
  three contexts. Locked-hand saves may protect any chop card if the clue leaves
  the receiver an unclued card. Eight-clue saves may protect a card on or off chop,
  but cannot focus the newest card or apply on the opening turn.
* **Precedence:** Normal useful play/save clues come first, followed by 5 stalls,
  tempo stalls, then fill-ins and general saves at equal priority, then burns.
  Safe ordinary plays/discards take precedence when available. Non-tempo stalls
  preserve literal information and existing connections without adding a new
  play or finesse instruction.
* **Gaps:** Anxiety plays, scream-discard stall recognition, and the 5-stall
  finesse-position exception are missing. Interpretation is conservative: it
  does not infer a finesse from the availability of a higher-precedence clue.

### Implementation and search limits

Each seat's interpretation is a [View](src/conventions/view.rs) that folds observed
events into notes; clues are read in [clue.rs](src/conventions/clue.rs) and stall
classification lives in [stall.rs](src/conventions/stall.rs). Which rules are active is
decided by the [profile](src/conventions/profile.rs) compiled from the level cap.
Regression cases are the [scenarios](tests/scenarios), a
[golden self-play corpus](tests/golden_selfplay.rs) and the modules' unit tests.

The [blueprint policy](src/policy/blueprint.rs) averages possible own-card identities
using remaining copy counts. It preserves ordinary action priorities unless a
forecast identifies an emergency; background risk on an unknown chop alone does
not trigger a change. Future draws and final-turn limits are not modeled.
Candidate generation, transitions, and evaluation are separate extension points
for deeper expectimax or robust minimax. Such an extension would need joint hand
beliefs, draw outcomes, and the final-turn counter. Teammates remain cooperative;
minimization would apply to uncertain outcomes, not opposing players.

## Variants

The engine simulates every hanab.live catalogue variant except the hidden-information
variants (Throw It in a Hole, Cow & Pig, Duck): reversed suits, Up or Down (with START
cards), Sudoku, orange (a play command on an orange card discards it and a discard
command plays it; replays record the effect), alternating clues and empty clues (Color
Blind, Number Blind) included. Stacks are a direction and a start, and the variant
answers what each stack still needs. The bot plays these variants legally, but the
conventions do not cover them (`raikan-web` refuses such tables). The
conventions cover plain suits and the
[rainbow suit](https://hanabi.github.io/variant-specific/rainbow):

* **Saving rainbow cards:** rainbow 2s are 2-saved like other 2s; a critical rainbow
  2, 3 or 4 can be saved with any colour or its rank (the general critical save).
* **Prompts in multi-colour variants** (level 5): see Level 5 above.
* **Free Choice Finesse** (assigned to level 5; the rainbow page gives no level): when
  the giver could have used another colour touching exactly the same cards, a prompt
  of a clued rainbow card must use the colour that card was clued with; a different
  colour asks for a finesse instead. Seats that see the receiver's hand know whether
  the giver had a free choice; the receiver keeps both readings until the connecting
  play shows which one was meant.
* **Not implemented:** the Free Choice Bluff (level 11), double prompts in multi-colour
  variants, and the bot does not deliberately give Free Choice Finesses (its own
  legibility check rejects them because the receiver cannot see the free choice).

Scenarios: [tests/scenarios](tests/scenarios) (`L5-free-choice-*.txt`).

## Architecture

The code follows the design in [docs/architecture.md](docs/architecture.md): layered
modules (`domain`, `variants`, `engine`, `knowledge`, `conventions`, `policy`, `search`,
`agent`, `adapters`), absolute seats and card ids, a variant catalogue taken from hanab.live, and
a convention profile compiled from the H-Group level. The port reproduced the previous
implementation exactly for 2–4 players before it was removed; see
[docs/parity.md](docs/parity.md) for results and the remaining gaps.

## Command line

```sh
raikan selfplay [SEED] [--players N] [--variant NAME] [--level L] [--trace FILE.jsonl]
raikan stats [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L]
raikan diff OLD.list NEW.list      # first game whose score dropped, with the bot's view
raikan support                     # every H-Group level 1-20 convention and its coverage
raikan explain REPLAY --turn T [--candidate ACTION]... [--all] [--world W]
               [--card CARD [--viewer SEAT]] [--json]
```

Every command that plays accepts `--search off|fast|full` (default `off`: the blueprint
policy alone) and the budget options `--simulations N`, `--worlds W`, `--margin M` and
`--search-seed S`, plus `--spread P` (the partner model's spread, default 0),
`--threads N` (rollouts of one decision on N threads; the result does not depend on it)
and `--time-limit-ms MS` (stop sequential halving early; no longer reproducible). The web
client uses all cores and `RAIKAN_TIME_LIMIT_MS` (default 3000). With search on, each decision samples worlds from the bot's beliefs,
simulates every permitted candidate (clues only if every teammate reads them as
intended) for one rotation of teammate moves (to the real end
once the deck is small) and replaces the blueprint's choice only when another candidate
is better by more than `M` paired standard errors (architecture §7.4). `fast` uses 300
simulations over 48 worlds, `full` 2000 over 256. Search is much slower than the
blueprint (about 1–19 s CPU per game with `fast`, for 2–5 players, against about 10 ms),
and scores higher with fewer strikes; see [docs/parity.md](docs/parity.md).

`stats` prints one line per game (`seed outcome score max turns blind-plays replay-url`)
to stdout and a summary to stderr. `--trace` writes one JSON object per line: how every
seat read every clue (rule and readings) and why every decision was taken.

`explain` recomputes one decision of a game (architecture §8.2). `REPLAY` is a
hanab.live replay JSON file, a `replay-json` URL (or a file holding one), or `seed:N`
for a self-play game with the given options; `T` is the hanab.live turn number (the
decision before that action). It prints:

- the position with the true cards and the notes of the seat to move;
- the blueprint's choice and reason, the bot's choice under the given options (with the
  planner's own candidate table when `--search` is on) and the action actually played;
- a comparison of the blueprint's candidates, the bot's choice, the played action and
  every `--candidate` (`play 2`, `discard 4`, `clue Bob red`; slot 1 is the newest card;
  `--all` adds every legal action), all simulated on the same sampled worlds (the
  planner's, or the `fast` budget). Each row has its mean value, the paired difference
  to the blueprint's choice, the value terms, the legibility guard's verdict and, for
  clues, how the giver and every teammate read it (rule, focus and readings);
- `--world W`: the rollouts of the requested actions (or the blueprint's and the bot's
  choice) in sampled world `W`, step by step with each teammate's reason;
- `--card CARD` (`#17` or `Bob:2`): every event that touched or changed that card's note
  as the seat to move (or `--viewer`) saw it, with the rule that read each clue.

`--level L` shows what the bot would have done at another level; `--json` prints the same
data as JSON.

## Performance and benchmarking

Run simulations with an optimized build:

```sh
cargo build --release
time target/release/raikan stats > /tmp/raikan-stats.list
```

The default runs 100,000 seeded games using the available CPU threads. Redirect
the per-game output when timing it so terminal rendering does not dominate.
Release builds use thin LTO and a single code generation unit.

A deterministic single-thread benchmark supports smaller comparisons:

```sh
cargo bench --bench simulation -- 100 4
cargo bench --bench simulation -- 100 4 --allocations
```

The arguments are game count and player count. The replay checksum covers every
move, so equal checksums verify that an optimization preserved the sampled games.
Allocation counts include reallocations; requested bytes are cumulative, not
peak memory. Use the run without allocation counting for timing.

Views share immutable clue-history snapshots, keep short connection chains inline,
cache play-state sets, and represent card possibilities as bitsets. An optimistic grade
skips
full safety checks and forecasts only when the clue cannot affect action
selection, stall precedence, or minimum clue value.

## Use as library

Raikan (as a library) offers a few different functions:

1. A variant-aware Hanabi engine (`variants`, `engine`) with hanab.live replay import and
   export, and a self-play runner over the `agent::Agent` trait.
2. Convention-free knowledge tracking and world sampling (`knowledge`).
3. An H-Group player (`agent::BotAgent`) with a configurable level cap and optional
   root-planner search (`search`).


## Project status

Raikan was born during Corona when I played frequently Hanabi with friends (following H-Group rules).
But occasionally we weren't enough people for a normal game (or too few to split into two normal sized onces).
Building an Hanabi bot following the already defined rules can't be that difficult, right?
Well, it is rather complex to correctly track knowledge and knowledge views correctly and encode hard-rules for hard decisions to judge different rules against each other.
But I wanted to practice programming in Rust again, and started.

I don't play as often anymore and find only time for more development infrequently.
But I still want to implement more features and rules and make the bot follow (almost all) lower levels.

As I wrote it primarily for me, it isn't very limited in its implemented/documented use cases.
My focues for now is to improve the documentation and architecture and developend tooling in general.
Afterward, the rule implementation can be improved over time.

## Name

Hanabi is Japanies for firework. Raikan is also japanies and means to support to initiate the fireworks.

## Contribution

I hope to accept pull requests for fixes / new improvements.
