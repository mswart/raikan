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
Alternatively, set `HANABI_FOLLOW=<username>` to let the bot join every pregame
table that user creates (if a seat is free). It does not leave a running game
for it, but does leave a finished game's shared replay. A table the bot was
booted from, or could not join, is not retried.
The web client supports one table at a time. It plays variants whose suits the
conventions cover: plain suits (`No Variant`, 3–6 suits), rainbow, black, white and
their dark forms gray and dark rainbow, alone or combined (e.g. `Rainbow (6 Suits)`,
`Black (5 Suits)`, `White (6 Suits)`, `Gray (6 Suits)`, `Dark Rainbow (6 Suits)`,
`Black & Rainbow (6 Suits)`, `Rainbow & White (5 Suits)`). Other special suits (pink,
brown, null, prism, light pink, the other dark suits, …) and variant modifiers are
refused unless `RAIKAN_EXPERIMENTAL_VARIANTS=1` is set. `RAIKAN_LEVEL=<n>` caps the
H-Group level (default 10); `RAIKAN_SEARCH=fast|full` turns on the search (default off,
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

Every convention of levels 1–10 is implemented for the standard game: a profile
compiled with `require_complete` accepts every level up to 10 (the variant-specific
Free Choice Finesse stays partial, see [Variants](#variants)). The default level cap
is still 9. Levels 11 and above are not implemented.

| H-Group level | Status |
| --- | --- |
| [1 — Fundamentals](https://hanabi.github.io/level-1) | Implemented for the standard five-suit game |
| [2 — Basic Moves](https://hanabi.github.io/level-2) | Implemented for the standard five-suit game |
| [3 — Basic Strategy](https://hanabi.github.io/level-3) | Implemented for the standard five-suit game |
| [4 — Chop Moves](https://hanabi.github.io/level-4) | Implemented |
| [5 — Intermediate Finesses](https://hanabi.github.io/level-5) | Implemented |
| [6 — Tempo Clues](https://hanabi.github.io/level-6) | Implemented |
| [7 — Emergency Discards](https://hanabi.github.io/level-7) | Implemented |
| [8 — End-Game](https://hanabi.github.io/level-8) | Implemented |
| [9 — Stalling](https://hanabi.github.io/level-9) | Implemented |
| [10 — Special Discards](https://hanabi.github.io/level-10) | Implemented |

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

* **Chop-moved cards:** A chop move protects an unclued card for the rest of the game.
  It is neither on chop nor in finesse position, is never prompted, and counts as
  already touched for clue focus; a clue that touches no other new card focuses the
  newest chop-moved card it touches (a normal play clue, not a tempo clue).
* **Trash chop move:** A clue whose new cards are all known trash (played, dead or
  clued elsewhere) chop moves every unclued card to the right of the rightmost trash
  card. The bot gives one only when every moved card is critical or an otherwise unseen
  2, never in place of a direct 5 or save clue.
* **5's chop move:** A 5 clue touching a 5 one away from chop (counting only unclued
  cards) chop moves the chop; the 5 is not a play clue. A 5 stall takes precedence.
* **Order chop move:** Playing an unknown clued 1 that skips `k` older 1s of the agreed
  order (same number of positive clues, not finessed) chop moves the chop of the seat `k`
  places later. The bot plays its 1s out of order when that protects a critical card or
  an unseen 2, skipping as few 1s as possible.

The rules live in [chop_move.rs](src/conventions/chop_move.rs).

### Level 5 — Intermediate Finesses

* **Layered, hidden and clandestine finesses:** Layered finesses track playable cards
  ahead of a connecting target, including layered self-finesses. A player who plays a
  clued card as a perceived prompt and finds another playable card blind-plays the next
  finesse position (hidden finesse); observers do not read that play as declining the
  finesse. When a layer connects to the clue too (clandestine), the receiver keeps both
  readings until the finessed player's next turn shows which one was meant.
* **Queued and ambiguous finesses:** A finessed card counts as clued, so a second
  finesse uses the next finesse position and the blind plays happen in order. When two
  players hold the card on finesse position, each sees the other's copy; once the first
  passes, the second plays, even if a teammate's clue reads wrongly until then.
* **Urgency Principle:** Finessed players blind-play as soon as possible. A save of a
  card on chop defers the blind play once; teammates keep waiting for one more turn.
* **Layered Finesse Dupe Clue:** A clued card whose exact identity is already promised
  to a blind play elsewhere is passed back with a discard.
* **Schrödinger's Cat Principle:** Receivers keep every live reading of a clue until
  plays, discards and clues of the intervening seats decide it.
* **Multi-colour prompts** (rainbow): a prompt goes to the clued card with the most
  positive clue information, ties to the newest; negative information is ignored.

### Level 6 — Tempo Clues

* **Tempo clues:** A clue that touches no new card asks the receiver to play its focus,
  the newest touched card. Focus shifts past touched cards already known to be playable;
  a clue on known-playable cards only is a burn.
* **Valuable tempo clues:** A tempo clue is valuable when it gets two or more clued cards
  played, when its focus (not a 5) could not have been prompted directly because a newer
  clued card would take the prompt, or when it unlocks a locked hand. Valuable tempo
  clues are ordinary play clues and satisfy the minimum clue value.
* **Tempo Clue Chop Move:** Outside stalling situations, a tempo clue that is not
  valuable also chop moves the receiver's chop. The bot gives one only when the chop is
  worth protecting. Stalling situations read it as a tempo clue stall instead (level 9).

### Level 7 — Emergency Discards

* **Scream discard:** Discarding the chop while holding a known play (at 0 clues, or at
  1 clue with a locked next player), or while holding known trash, chop moves the next
  player's chop. That player must clue instead of discarding; stall clues are allowed.
  The bot screams only to save a critical card the next player would otherwise discard.
* **Shout discard:** Discarding known trash while holding a known play has the same
  effect at any clue count. The bot shouts at 0 or 1 clue to save a critical, playable
  or unseen-2 chop card.
* **Generation discard:** Discarding a known-playable clued card asks the next player
  to clue without a chop move. The bot does it at 0 clues, with a known non-critical
  card, when the next player would discard and the player after that would lose a
  critical chop card. Returning a duplicate is a sarcastic or dupe discard instead.
* **Blind-playing chop-moved cards:** Instead of discarding, the bot blind-plays a
  chop-moved card that is at least 80% likely to be playable while the team has a strike
  to spare.
* **Look-ahead:** Action search forecasts other players' plays and forced zero-token
  discards for one rotation and intervenes in predictable emergencies.

The signals live in [emergency.rs](src/conventions/emergency.rs).

### Level 8 — End-Game

* **End-game:** Pace is the score plus the cards left in the deck plus the number of
  players minus the maximum score. The end-game starts when pace drops below the number
  of players. Chop moves are off from then on; a clue that would be one reads as a stall
  or by its literal meaning.
* **Positional discards and misplays:** Once the deck is empty, a player sees every
  card. A discard from a slot other than the expected one (known trash, else chop) asks
  the last player in turn order with a playable card on that slot to blind-play it. When
  that slot is the expected discard, the player misplays it instead; a misplay where a
  discard would have worked promises two blind plays. The bot signals only when all its
  unknown cards are trash and every teammate reads the same target.
* **Distribution clue:** In the end-game a play clue may duplicate a playable card held
  in a hand that already has two or more queued plays; good touch does not rule it out.
* **Burning and the Pace +1 rule:** In the end-game a player without a play stalls
  (burns a clue) instead of discarding when it holds two or more useful clued cards
  (team distribution), or when pace allows at most one more discard and a teammate
  without a play will need it.

The end-game rules live in [endgame.rs](src/conventions/endgame.rs); the positional
signals are tested in [tests/endgame.rs](tests/endgame.rs).

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
* **Permission to Discard:** With three or more players, an early-game 5 stall on the
  player right before us is optional: that player saw no clue for us to give, so we may
  discard instead.
* **5 Stall Finesse Position Exception:** A 5 stall is still allowed when every
  remaining play clue would touch a playable card on finesse position that another
  teammate could finesse (its successor is visible to that teammate in a third hand and
  a clue would be left).
* **Anxiety Play:** A locked player at 0 clues with no known play or trash plays the
  card most likely to be playable (the newest on ties) while a strike can be spared.
* **Scream-discard stalls:** A player screamed or shouted at must clue and may stall at
  double-discard severity (level 7).
* Interpretation is conservative: it does not infer a finesse from the availability of
  a higher-precedence clue.

### Level 10 — Special Discards

* **Gentleman's Discard:** Discarding a clued card whose identity the discarder knew
  from public clue information, that is playable and duplicates no clued or promised
  card, promises the other copy on the finesse position of the first other player (in
  turn order) who visibly holds it there; a player who sees no such copy holds it
  itself. It is read as a finesse, so a layered Gentleman's Discard unravels like a
  layered finesse. Locked players' forced discards carry no such meaning. The bot gives
  one at 0 clues where a generation discard would otherwise be needed, so the card is
  not lost.
* **Baton Discard:** The same with a known unplayable card: the receiving card counts as
  clued with that identity (exactly on finesse position, never layered). The bot uses it
  instead of discarding a chop that may be critical.
* **Sarcastic Finesse:** A play clue's focus may duplicate the giver's own clued card
  that the giver does not know when the identity still needs a connection; once the
  finesse resolves, the receiver returns its copy with a sarcastic discard.
* **Certain Finesse and Certain Discard:** A finessed card whose promised identity is
  clued in another hand is discarded instead of played; teammates move the connection to
  the clued copy. Composition finesses combine both.
* **Double Gentleman's Discard (illegal):** Only the true identity of a card the
  discarder knew is transferred; the bot never transfers a card it does not know.

The level-10 discards live in [gentleman.rs](src/conventions/gentleman.rs). Because a
known playable discard now reads as a Gentleman's Discard, level 10 replaces the level-7
generation discard by it.

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
conventions do not cover them (`raikan-web` refuses such tables).

The conventions cover these suits, alone or combined, without variant-wide modifiers:

| Suit | Touched by | Copies | Example variants |
| --- | --- | --- | --- |
| Plain (red, yellow, green, blue, purple, teal) | its colour, its rank | 3/2/2/2/1 | `No Variant`, `6 Suits`, `4 Suits`, `3 Suits` |
| [Rainbow](https://hanabi.github.io/variant-specific/rainbow) | every colour, its rank | 3/2/2/2/1 | `Rainbow (6 Suits)`, `Rainbow (5 Suits)` |
| [Black](https://hanabi.github.io/variant-specific/black) (dark) | black, its rank | one of each | `Black (6 Suits)`, `Black (5 Suits)` |
| [White](https://hanabi.github.io/variant-specific/white) (light) | no colour, its rank | 3/2/2/2/1 | `White (6 Suits)`, `White (5 Suits)` |
| [Gray](https://hanabi.github.io/variant-specific/gray) (dark white) | no colour, its rank | one of each | `Gray (6 Suits)`, `Gray (5 Suits)` |
| [Dark Rainbow](https://hanabi.github.io/variant-specific/dark-rainbow) | every colour, its rank | one of each | `Dark Rainbow (6 Suits)`, `Dark Rainbow (5 Suits)` |

Combinations such as `Black & Rainbow`, `Black & White`, `Black & Gray`,
`Black & Dark Rainbow`, `Rainbow & White`, `Rainbow & Gray`, `White & Dark Rainbow` and
`Dark Rainbow & Gray` are covered too. The variant pages assign no levels; their save
rules are reviewed as level 1 together with the save clues.

Rainbow:

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

Black, gray and dark rainbow (dark suits, one copy of each card):

* **Black 2 & 5 saves:** dark 2s and 5s on chop are saved with their rank. A colour clue
  saves them only under the black page's exceptions; otherwise a black clue on a black 2
  or 5 is a play clue for everyone who sees it. The page does not define the exceptions;
  the readings implemented are:
  1. the clue newly touches two or more critical cards (black, or critical rainbow or
     dark rainbow);
  2. good touch: the rank clue would newly touch a card known to be trash or a duplicate
     of a clued card, and the colour clue touches no such card;
  3. fill-in: the clue re-touches an already clued card and narrows what it can be;
  4. negative information: an untouched clued card's identity, playability or trash
     status becomes known.
* **Black 3 & 4 saves:** dark 3s and 4s on chop are saved with a colour clue. A rank clue
  saves them only as a Florrat Save (it newly touches another critical card); otherwise
  it is a play clue for everyone who sees it.
* **Gray saves:** gray cards can only be touched by rank: gray 2s and 5s use the 2 and 5
  saves, gray 3s and 4s the 3 or 4 clue.
* **Dark rainbow saves:** dark rainbow 2s and 5s are saved with their rank; only a black
  clue (in variants with a black suit) can save them under the black exceptions, any
  other colour on them is a play clue. Dark rainbow 3s and 4s are saved with a black clue when the variant
  has a black suit; otherwise with their rank, or with a colour when the clue also newly
  touches another card that is critical because of discards (not a 5, not dark), or when
  the colour's own suit is completely played or accounted for (every card played, dead or
  clued).
* **Who can tell:** seats that see the touched cards judge the exceptions by their
  identities. The receiver judges by what its cards may be, so it keeps a save reading
  whenever an exception is possible. The good-touch exception depends on cards that only
  the giver and third parties see: the receiver counts only its cards of known identity,
  so it may read a good-touch save of a black 2 or 5 as a save of a black 3 or 4 (the card
  is protected either way).

White and gray (no colour touches them):

* **Loaded play clue assumption:** in a variant with a colourless suit, a 3 or 4 clue to
  a player with a known play or known trash is a play clue, never a save: a loaded
  player does not discard, and white 3s and 4s on chop are otherwise hard to get played.

Scenarios: [tests/scenarios](tests/scenarios) (`L5-free-choice-*.txt`,
`L1-black-*.txt`, `L1-white-*.txt`).

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
raikan explain REPLAY [--turn T] [--candidate ACTION]... [--all] [--world W]
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
hanab.live replay JSON file (also the `https://hanab.live/export/ID` format), a
`replay-json` URL (or a file holding one), a hanab.live game id or replay URL such as
`2073109` or `https://hanab.live/shared-replay/2073109#12` (downloaded from hanab.live,
needs the default `fetch` feature), or `seed:N` for a self-play game with the given
options; `T` is the hanab.live turn number (the decision before that action) and
defaults to the `#T` of a replay URL. It prints:

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
