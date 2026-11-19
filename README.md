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
The web client supports one table at a time, using the standard `No Variant`
rules. It uses Hanabi Live's current lobby and game-event protocol; spectators
and replay viewers do not send moves.

The client follows the [November 14, 2026 protocol contracts](https://github.com/Hanabi-Live/hanabi-live/tree/1701eb99677f/server/protocol/src).
WebSocket requests use JSON frames with request IDs and correlated replies.
The client loads game setup after `tableJoined`, applies each batch of live
events before choosing a move, and replaces its history with the complete
replay when `gameFinished` arrives. Paused games wait for a resume notification.

## H-Group rule support

Support is partial and varies by convention. This overview covers levels 1–9,
ending at stalling, the highest level with implemented special moves. It does not
mean that all earlier levels are complete. Only the standard five-suit game is
supported; variant-specific rules are not implemented.

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

Focused regression tests live in [fundamentals.rs](src/hyphenated/fundamentals.rs),
with replay coverage in [line_tests.rs](tests/line_tests.rs). The advanced
connection and recovery limitations described under later levels still apply.

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

Focused coverage is in [basic_moves.rs](src/hyphenated/basic_moves.rs), including
all supported player counts and seat rotations, with additional replay cases in
[line_tests.rs](tests/line_tests.rs).

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

Focused coverage is in [basic_strategy_tests.rs](src/hyphenated/basic_strategy_tests.rs),
including independent player views and all supported player counts for 1s order.
The action search remains bounded; level 3 does not add the dedicated end-game
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
* **Gaps:** Multi-color prompt rules are outside the supported variant. Hidden
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

Clue interpretation and card knowledge live in
[line.rs](src/hyphenated/line.rs) and
[interpretation.rs](src/hyphenated/interpretation.rs); stall classification lives
in [stall.rs](src/hyphenated/stall.rs). Regression cases are in
[line_tests.rs](tests/line_tests.rs) and the modules' unit tests.

[Action search](src/hyphenated/search.rs) averages possible own-card identities
using remaining copy counts. It preserves ordinary action priorities unless a
forecast identifies an emergency; background risk on an unknown chop alone does
not trigger a change. Future draws and final-turn limits are not modeled.
Candidate generation, transitions, and evaluation are separate extension points
for deeper expectimax or robust minimax. Such an extension would need joint hand
beliefs, draw outcomes, and the final-turn counter. Teammates remain cooperative;
minimization would apply to uncertain outcomes, not opposing players.

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

Search shares immutable clue-history snapshots, keeps short connection chains
and scratch buffers inline, and represents card possibilities as bitsets.
Candidate clues reuse interpreted player views. An optimistic score bound skips
full safety checks and forecasts only when the clue cannot affect action
selection, stall precedence, or minimum clue value.

## Use as library

Raikan (as a library) offers a few different functions:

1. A hanabi game implementation (using a Player interface for strategy implementations).
   The interface is tried to keep simple/state less. Each player implementation should do state tracking according to its needs.
2. Helper functions to do game state tracking (e.g. represent possible card options)
3. An H-Group rules following player ipmlementation.


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
