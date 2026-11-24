# Raikan target architecture

Status: design target, written 2026-11-18. The first milestone (today's feature set on
the new layers) is implemented; [parity.md](parity.md) lists what is in place and what
is still missing per section. The current behaviour and its gaps are described in the
[README](../README.md).

## 1. Goals

1. **H-Group conventions up to level 20 and beyond.** The level is configured per game and
   limits which rules are active.
2. **Variants.** Six suits, black, rainbow and white first, then orange. The model must
   also fit pink, brown, null, omni, prism, dark suits, reversed, up-or-down,
   clue-starved, and the hidden-information variants (throw-it-in-a-hole, cow & pig, duck).
3. **Real comparison of options.** Plays, discards and clues are compared by simulated
   outcome, not by a fixed priority ladder.
4. **Explainability.** For every decision it must be possible to see what was chosen, what
   it was compared against, and why — and for every belief about a card, which action and
   which convention put it there.

Level, variant and search are independent axes. Adding a convention means a rule and its
tests; adding a suit usually means catalogue data plus convention adaptations. Neither
changes the search.

Non-goals: inventing conventions, and assuming that teammates run the same search.
Teammates may be humans or other H-Group bots, so they are modelled as convention
followers. Learned priors or value models are not required; they can be added later
behind the policy and evaluator interfaces (section 7.6).

## 2. The six decisions that shape everything

1. **Layers with one-way dependencies:** game rules → logical knowledge → conventions →
   action choice. A lower layer never knows about a higher one, and convention code never
   calls the search to decide what an observed move means.
2. **Absolute addressing.** Seats are `0..n`, cards are identified by deck order
   (`CardId`, the same number hanab.live calls `order`), identities by a variant-defined
   index. Relative seats and hand positions exist only in adapters.
3. **Convention state is stored once; a view is a function of sight.** A player's view is
   the shared record of what happened, interpreted against the set of cards that player
   can see (their **sight**). Interpretation is memoised on what it actually read (card
   identities, copy counts, earlier conclusions), which is what makes simulating
   teammates inside a search affordable.
4. **Interpretation is a pure, re-runnable function.** Records of what happened are the
   source of truth; everything believed about a card is rebuilt from them and carries a
   pointer back to them.
5. **Three separate questions per action:** is it mechanically legal, is its meaning
   permitted and legible under the active conventions, and how valuable is it. Only the
   first two remove an action. Preferences are priors. Search answers the third and never
   invents meaning.
6. **Every decision is reproducible** from (replay prefix, configuration, seed), so a
   decision can be recomputed later with full tracing instead of logging everything live.

## 3. Layer map

```
            +--------------------------------------------------------------+
 adapters   | raikan CLI (selfplay, stats, explain, diff)   hanab.live client|
            +-------------------------------+------------------------------+
                                            |
            +-------------------------------v------------------------------+
 agent      | configuration, wiring, observe(events) / choose_action(budget)|
            +------+---------------------------------------------+---------+
                   |                                             |
            +------v-----------+                      +----------v---------+
 choice     | search           | -- uses ----------> | policy: candidates, |
            | belief tree,     |                     | blueprint, partner  |
            | rollouts, value  |                     | model               |
            +------+-----------+                      +----------+---------+
                   |                                             |
            +------v---------------------------------------------v---------+
 meaning    | conventions: profile, rules, records, interpreter, notes     |
            +-------------------------------+------------------------------+
                                            |
            +-------------------------------v------------------------------+
 knowledge  | sight, literal information, constraints, world sampler       |
            +-------------------------------+------------------------------+
                                            |
            +-------------------------------v------------------------------+
 rules      | variant, identities, game state, events, simulator, replay   |
            +--------------------------------------------------------------+

 trace: sink trait and ids sit beside the rules layer; every layer emits its own payloads.
```

Start as modules of the existing crate (`domain`, `variants`, `engine`, `knowledge`,
`conventions`, `policy`, `search`, `agent`, `adapters`, `trace`). Split into workspace
crates when compile time or dependency isolation justifies it; until then a test that
checks module imports keeps the direction honest.

## 4. Rules layer: variant-driven game mechanics

### 4.1 Variant

A `Variant` is built from the hanab.live catalogue (its `suits.json` / `variants.json`,
with the upstream revision pinned) and precomputes every table the other layers need.
Card suit, clue colour, display colour and identity are four separate concepts.

| Concept | Representation |
| --- | --- |
| Identity | `Identity(u8)` = dense index over suit × rank. At most 6 suits × 6 ranks (1–5 plus the up-or-down START card), so an `IdentitySet` is a `u64` bitset. |
| Physical card | `CardId(u8)` = deck order. The deck has at most 60 cards, so a `CardSet` is also a `u64`. |
| Clue | `ClueId(u8)` = dense index over the clues that exist in this variant. Suit and clue colour are *not* 1:1. |
| Touch | One table `touch[ClueId] -> IdentitySet`. This is the only place a touch rule lives. |
| Copies | `copies[Identity] -> u8` (default 3/2/2/2/1; one of each for dark suits; variant overrides). |
| Stacks | Per-suit state enum (`Up(n)`, `Down(n)`, `Undecided`, `Finished`, …). The variant answers `playable(stacks)`, `trash(stacks, discards)`, `critical(stacks, discards)` and `predecessors(identity, stacks)` as `IdentitySet`s. Nothing above this layer assumes "rank + 1" or "5 is the top". |
| Card movement | A **command** (move this card to the stacks or to the discards) is resolved into an **effect** per identity. For orange (inverted) suits the two differ: a discard command can advance the stack. |
| Tokens | Clue tokens in half units, with variant-supplied gain and cost (clue-starved). |
| Legal commands | `legal_commands(state)` — token limits, muted and alternating-clue variants; empty clues where the variant allows them. |
| Capabilities | Queries such as multi-colour touch, colourless, unique copies, inverted. Convention rules key on these, never on variant names. |

Verified against the hanab.live catalogue (all 2428 variants): the maxima are exactly 36
identities (Up or Down, 6 suits) and 60 cards (6 suits), so both bitsets fit a `u64`.
Touch is a pure function of (variant, suit index,
rank, clue), including rainbow (`allClueColors`), white (`noClueColors`), pink
(`allClueRanks`), brown (`noClueRanks`), prism, special ranks, funnels/chimneys and
odds-and-evens. One table therefore covers every listed variant.

### 4.2 State, world, events

- `GameState` is fixed-size and `Copy`: stacks, discard counts per identity, tokens,
  strikes, hands as arrays of `CardId`, cards left in the deck, turn, current seat,
  final-round counter. No heap, no replay log, no player objects.
- `World` assigns an `Identity` to every `CardId`. `GameState` + `World` is a complete,
  simulatable game. The real game is a `GameState` plus a partially known world.
- `Action` is `Play(CardId)`, `Discard(CardId)` or `Clue { target, clue }`. These are
  commands, chosen from the actor's information only.
- The **simulator** is the only implementation of transitions:
  `step(state, world, action) -> Event`. An `Event` carries the command *and* the
  resolved effect, and is the single input of every other layer and of the log.
  Keeping both prevents an orange connection from looking like a sarcastic discard.
- **Event fields are redactable.** In throw-it-in-a-hole the played identity and success
  are hidden; in cow & pig and duck the clue value is. Those fields are options, and
  stacks become something a viewpoint *believes*. Code above this layer asks its view
  "is this identity playable?" and never reads simulator stacks directly. This cannot be
  retrofitted, so it is part of the design from the start.
- **A redacted field is a world variable.** The `World` assigns not only an `Identity` to
  every `CardId` but also a value to every redacted event field (the hidden clue of a
  cow & pig or duck clue). A hidden played card in throw-it-in-a-hole is simply a card
  whose identity is not in anyone's sight, and believed stacks are a function of
  (world, events). The simulator fills these variables; viewpoints sample them like
  any other unseen value (section 5).
- Replay import/export in hanab.live format stays, extended to carry the variant and the
  convention profile digest (section 6.4).

## 5. Knowledge: convention-free reasoning

- **Sight** is a `CardSet`: the cards whose identity a viewpoint knows. A seat's sight is
  every card outside its own hand. The empty sight (only revealed cards) is the
  common-knowledge viewpoint; what every seat knows separately is *not* common
  knowledge. "Everything except the hands of T and U" is what T can assume U knows.
  Level-12 asymmetric-information reasoning and "Bob's Truth" need exactly this depth-2
  viewpoint, and they get it without nested player models.
- **Literal information** per card: an `IdentitySet` from positive and negative clue
  touches (public) narrowed by card counting (depends on sight). It survives every
  correction of a convention mistake. The two parts stay separately readable: the
  public part reads nothing hidden, and card counting is read per identity as "visible
  copies of *i*" (section 6.1), so a rule that consults it does not depend on the
  whole sight.
- **Constraint store** with two strictly separate classes. *Physical* constraints:
  per-card domains, per-identity counts, and *hidden-field* constraints that tie the
  cards touched (and not touched) by a redacted clue to one shared, unknown clue
  value from `touch`. *Convention* constraints: identity restrictions
  and cross-card constraints such as `Located(identity, CardSet)` — "exactly one of these
  cards is r3". Conventions add the second class; this layer only knows how to propagate
  and satisfy them. A belief can be wrong while the world stays valid: a bluffed player's
  apparent identity is a convention constraint in that player's view, never a physical one.
- **World sampler**: draws joint assignments for all unseen cards (own hand and deck)
  that satisfy the physical constraints, weighted by the convention constraints
  (section 7.3). Sampling must be joint and unbiased: per-slot marginals violate card
  counts, and naive slot-by-slot choice with backtracking skews the distribution.
  Rejection of whole deals is unbiased but far too slow: for a typical clued hand only
  about 0.2% of uniform deals fit.
- **Exact sampler.** Deck cards have unrestricted domains, so only the constrained cards
  (own hand, plus hidden cards and hidden clue values in the hidden-information
  variants) need care. A dynamic programme over identities × subset of constrained
  cards already assigned, with weight `copies_left falling |subset|`, counts the
  physical deals exactly and samples backwards from it; the remaining cards are then
  shuffled into the deck. Hidden clue values are enumerated on top (at most a dozen
  per redacted clue). Convention constraints enter as importance weights. A prototype
  with six constrained cards built the table in about 45 µs and drew a world in under
  1 µs, matching enumeration exactly.
- **Fallback sampler.** The table grows as 2^(constrained cards), which is fine up to
  about 16 and not beyond. Throw-it-in-a-hole, where every hidden play adds a
  constrained card, uses a sequential-importance or MCMC sampler behind the same
  `WorldSampler` trait and reports its effective sample size in the trace (7.3).

## 6. Conventions

### 6.1 Records and interpretation

Every observed action produces one **record**:

```rust
struct Record {
    id: RecordId,
    event: Event,
    context: ContextId,            // frozen public state before and after the event
    depends_on: SmallVec<[RecordId; 2]>,
    status: Status,                // Open | Resolved | Retracted(reason)
}
```

A record does not store concrete meanings. Rules are written from an observer's point of
view, as H-Group describes them, against an interpretation context:

```rust
trait Convention {
    fn meta(&self) -> &RuleMeta;
    fn interpret(&self, ctx: &InterpretCtx, event: &Event, out: &mut Vec<Reading>);
    fn propose(&self, ctx: &DecisionCtx, out: &mut Vec<ActionIntent>);
}

struct Reading {
    rule: RuleId,                  // e.g. "L1.play-clue", "L11.bluff"
    tier: Tier,                    // Occam ordering
    focus: Option<CardId>,
    chains: SmallVec<[Chain; 2]>,  // connecting steps: queued, prompt, finesse, …
    commits: Commitments,          // constraints, protection, obligations with deadlines
    expects: Expectations,         // which later events confirm or kill it
}
```

The context is the only way a rule reaches anything that differs between viewpoints or
worlds, and **every such access is recorded as a read** with the value it returned:

| Read | Recorded value |
| --- | --- |
| `ctx.identity(card)` | the identity if the card is in the observer's sight, else "unknown" |
| `ctx.visible_copies(identity)` | the card-counting count in this sight (never "all of the sight") |
| `ctx.note(card)`, `ctx.reserved(card)` | what the earlier records of the same fold concluded about the card |
| `ctx.hidden_field(record)` | a redacted event field, if this viewpoint knows it |

Public facts (clue touches, stacks where public, hand order) need no recording. An
interpretation result is valid for every (sight, world, earlier fold) that returns the
same values for the reads actually made, and is cached under (record, read log). Reads
of earlier conclusions matter: a later finesse depends on which cards an earlier
reading reserved, so leaving them out of the key would return wrong cached results.

The cache per record is indexed as a decision tree on the read sequence (the next read
is determined by the previous values, since rules are deterministic), not scanned
linearly; a prototype with a linear scan spent more time on lookup than on a cheap rule.

- Rules stay natural to write, and enumerating every chain consistent with common
  knowledge is avoided (estimated at around a thousand for a single rank clue in a
  five-player game, against a handful for any real viewpoint).
- Inside a search, sampled worlds differ only in our own hand and the deck, so most
  teammate interpretations are cache hits across worlds. A prototype (5 players, toy
  finesse reader, 200 worlds) hit 92.5%: about 23 distinct readings per 200 worlds for a
  clue to our hand, 12–16 for other clues, because locators scan our finesse position
  and earlier reservations propagate.
- **Reference semantics:** the view of seat *s* must equal what an independent
  interpreter would produce from *s*'s own observation history under that deal. That
  slower implementation is kept as a test oracle (section 10).
- Requirements come in two kinds: *giver saw* (a fact about other hands) and *giver
  believed* (the giver's own view at clue time, which can be wrong). Seats that can see
  the giver's cards know when a clue rests on a false premise.

Each event is processed in fixed phases: literal information; advance or contradict
existing obligations; collect readings from active rules; resolve precedence, keeping
unresolved alternatives; derive notes; commit a delta with its trace.

### 6.2 Notes

What a viewpoint believes about each card — inferred identities, "play now", "play after
X", protected (saved or chop-moved), known trash, promised blind play — is a **note**.
Notes are never edited in place. They are rebuilt by folding the live records,
interpreted in that sight, into the constraint store. Each note keeps the record ids that
produced it, which is what lets `explain` answer "why does the bot think this card is b3?".

Records are not independent (a later finesse position depends on which reading of an
earlier clue is true), so the fold processes records in order and each interpretation
sees the reservations of the earlier ones. Positions such as "second finesse position"
are resolved against the hand at that time and then held as `CardId`s, so a later draw
never inherits a signal.

### 6.3 Retraction

Interpretation is `interpret(event, frozen context, set of retracted records)`. When a
strike or a contradiction kills a record, it is marked retracted with a reason and every
record that depends on it is re-interpreted from its frozen context. Because public state
is fixed-size, keeping one snapshot per turn is cheap. This replaces snapshot chains and
"delete everything since turn t".

### 6.4 Convention profile

One immutable `ConventionProfile` is compiled per game from (maximum level, variant,
convention revision). It holds the active rule ids, slot overrides, the precedence table
and a **digest** that goes into replays, traces and cache keys.

- The cap applies everywhere: interpreting teammates, proposing actions, the partner
  model, rollouts and value features. An unexpected higher-level move still updates
  literal information but gains no disabled meaning.
- Each rule declares its introduction level and **prerequisites**. A variant adaptation
  is gated by them too: a rainbow free-choice bluff needs the level-11 bluff rule.
- A plain `rule.level <= max_level` filter is not enough where a later level changes an
  earlier meaning; disabling the later rule must restore the earlier one (section 6.5).
- Each rule carries a coverage status (interpretation, generation, follow-up/retraction,
  interactions). With `require_complete = true` the profile refuses to compile when a
  requested level or variant has gaps, instead of silently playing a lower level. A
  readable support matrix is generated from the same manifest.
- The profile is frozen for a live game. Changing it offline means rebuilding knowledge
  from history.

### 6.5 Rules

```rust
struct RuleMeta {
    id: RuleId,                // stable, appears in traces and tests
    level: u8,                 // H-Group level that introduces it
    requires: &'static [RuleId],
    doc: &'static str,         // URL of the convention on hanabi.github.io
    applies: fn(&Variant) -> bool,
    trigger: TriggerClass,     // clue / discard / play / situation
    supersedes: &'static [RuleId],
    coverage: Coverage,
}
```

Three mechanisms control behaviour across levels and variants:

1. **Activation** by the profile. Each rule is `Active`, `Off`, or optionally `AvoidOnly`
   (a rule above the table's level proposes no meaning but can flag a candidate as
   hazardous for teammates who habitually play higher).
2. **Policy slots** for rules that later levels or variants *modify* rather than replace.
   A slot is a named function with layered overrides, keyed on suit capabilities, so
   combinations such as "Black & Rainbow" compose; where a combination has its own
   exception it gets an explicit override and its own tests. Examples: `save.identities`
   (black: rank saves a dark 2/5, colour saves a dark 3/4), `focus.identity_filter` (pink
   promise), `bluff.valid_target` (levels 11 → 13 → 15), `prompt.order`,
   `stall.table[severity]`, `dda.enabled` (off in hard variants).
3. **`supersedes`** for genuine replacements only.

There is no `if level >= n` in rule bodies, and the order of modules never decides a
meaning.

### 6.6 Extension points

| Hook | Purpose | Needed by (examples) |
| --- | --- | --- |
| Situation classifiers | Chop, finesse position *k*, loaded, locked, stall severity, early game, score phase, end-game/pace. Each declares its epistemic basis: common, actor, or receiver. | Everything; ejections need 2nd/3rd finesse position, level 19 needs score phase |
| Clue / discard / play readers | `interpret` for an event class | Saves, plays, bluffs; sarcastic, scream, gentleman's, positional discards; order chop move, positional misplay |
| Locator | `locate(identity, ctx)`: queued, publicly known, prompt, finesse with layers, self. Shared by all readers | Finesses, gentleman's discard, positional moves |
| Expected actions | Publicly computable **ranked list** of what the actor was expected to do | A deviation from the expected action carries the meaning; priority (level 25) |
| Clue worth | Public, deterministic value of a clue's plain reading (new touches, plays unlocked) | Trash chop move, tempo clue chop move, trash push, ejections, discharges, ignition, "unnecessary moves" |
| Opportunity oracle | "Could the giver, or another player, have given an equivalent or better clue?" Answers `Supported(evidence)`, `Refuted(evidence)` or `Unknown`, from the historical view and deterministic convention criteria, bounded to clues reaching the same focus | Context, elimination, advanced moves; rainbow free choice |
| Reactions | How open records respond to later events (fulfilled, declined, layered retarget, confirmed) | All multi-turn conventions |
| Rule-owned public state | Small persistent facts a rule needs (early game ended, 5-stall given) | Stalling, score phases |
| Proposers | `propose`: candidate commands with an intended meaning | Policy and search |

Clue worth and the opportunity oracle never consult the search. Otherwise a change of
search budget would change what an observed action means.

**Precedence** between competing readings is an explicit, data-driven table per trigger
class and profile, refined by a comparator over resolved chains in the first responder's
sight (fewest blind plays, others before self, bluff seat). It is conditional and
perspective-dependent, not one global integer per rule. Every proposal ends in the trace
with a status: accepted, shadowed by rule X, blocked because a lower tier was valid in
sight S, or requirement R failed in sight S.

### 6.7 Level map

| Levels | Topic | Main mechanisms |
| --- | --- | --- |
| 1–3 | Fundamentals, basic moves, basic strategy | Clue readers, locator, reactions, retraction for fixes, sarcastic discard reader |
| 4, 6 | Chop moves, tempo clues | Protection notes, clue worth, expected actions (order chop move) |
| 5 | Intermediate finesses | Locator with layers; hidden and clandestine connections; `prompt.order` |
| 7, 22 | Emergency discards | Discard readers, expected actions, stall severity |
| 8 | End-game | Pace classifier, positional discard/misplay readers, rollouts to the real end |
| 9 | Stalling | Severity classifier, `stall.table`, rule-owned state |
| 10 | Special discards | Discard readers plus locator |
| 11, 13, 15 | Bluffs | Apparent vs. actual identity, bluff seat, `bluff.valid_target`, precedence comparator |
| 12 | Context | Depth-2 sight, opportunity oracle |
| 14, 16 | Trash moves, ejections and discharges | Clue worth, finesse position *k*, oracle |
| 17, 18 | Dupe tech, elimination | Cross-card constraints, retroactive re-interpretation |
| 19 | 5 tech | Score-phase classifier, slots |
| 20 | Advanced moves | Oracle; obligations including a required fix before a play |
| 21, 24, 25 | Ignition, unnecessary moves, priority | Clue worth, oracle, expected-action ranking (level 23 not yet analysed) |

A level counts as supported only when every applicable rule can be interpreted,
generated, followed up and recovered from, not when its headline move is recognised.

## 7. Action choice

### 7.1 Policy

- **Candidates** are all mechanically legal commands whose meaning the profile permits
  and that pass the legibility guard (7.5) — every play and discard position, every
  clue. Several intents that produce the same command are merged into one candidate.
- **Intents are never transmitted.** A simulated receiver sees only the command and
  reads it with its own interpreter, so the search cannot create a private channel.
- The **blueprint** ranks candidates by conventional preference and names the rule behind
  each. It is the search prior, the reference for "expected action", the fallback when
  no simulation finishes, and a no-search fast mode for bulk self-play.
- The **partner model** maps a seat's view to a distribution over its candidates:
  convention obligations first, then blueprint order with some spread. It is frozen for
  one planning call and sees only that seat's view.
- Candidates, priors and partner choices must not depend on unobserved cards of the
  sampled world. Tests enforce this by substituting hidden cards (section 10).

### 7.2 Search model

The search is a **root-player belief tree**: Monte Carlo tree search over the bot's own
action/observation histories, with worlds sampled from its belief (the POMCP scheme).

- Decision nodes belong to the bot and are keyed by its action/observation history plus
  profile and variant digests — never by sampled cards. This is what prevents the search
  from "knowing" its hand at later decisions.
- Between two own decisions, the other seats act by the partner model from their own
  views, clues included, and draws come from the sampled deck. Those steps are chance,
  not choices optimised against facts only the bot knows.
- All team returns are added up the visited path; there is no opposing player.
- Other planners (re-determinising IS-MCTS, an exact end-game solver that respects the
  same information limits) fit behind the same `Planner` trait.

### 7.3 Beliefs that can be wrong

Humans make mistakes, so convention constraints are weights, not filters: a world
satisfying all open records has weight 1, a world satisfying only physical constraints
has a small weight ε (configurable per teammate type). If the bot's own view is
unsatisfiable, the newest record is dropped first and the trace names it; if physical
constraints are unsatisfiable, that is a replay error and is reported as one. The trace
reports world count and effective sample size, so a high visit count cannot hide poor
belief coverage.

Worlds are re-sampled for every decision from the constraint store. That keeps decisions
reproducible from the replay prefix. Weighting worlds by how likely the partner model
makes the observed teammate actions is a later refinement.

### 7.4 First planner: expand only the root

With a budget of one or two thousand simulations and dozens of root actions, nodes below
the root would each receive a handful of visits, because nearly every world produces a
different observation sequence. The first planner therefore spends the whole budget at
the root and estimates the rest:

1. Sample W worlds once and reuse them for every candidate (common random numbers), so
   candidates are compared pairwise on identical hidden cards.
2. For each (candidate, world): apply the command; update the records, so teammates read
   our action through the conventions; let the other seats act by the partner model
   until our next turn, or to the real end in the end-game (pace below the number of
   players, the deck nearly empty or the final round; our own later turns then follow
   the blueprint from our view); score the leaf.
3. Allocate by sequential halving over **all** candidates: every candidate gets a few
   worlds, the weaker half is dropped, the rest gets more. Blueprint order only decides
   who is evaluated first.
4. Choose the best paired estimate. When the best candidate is not distinguishable from
   the blueprint's first choice within the noise, take the blueprint's choice; the
   margin is configurable down to zero. The paired standard error only measures sampling
   noise, not the error of the leaf estimate, so the best candidate must also gain at
   least `min_gain` expected points, and both it and the blueprint's choice need enough
   worlds (a time limit can stop the halving early).
5. Worlds are evaluated in parallel with per-world seeds; the result does not depend on
   thread count.

No play-before-clue rule or safe-play shortcut bypasses this comparison; only a single
permitted action returns immediately. Deeper expansion pays off where observations
repeat — small decks, forced sequences — and with larger budgets. It is the same tree
with more nodes, not a different design.

Budgets are simulations, wall time, nodes and memory, with cancellation when a newer
event arrives. Fixed simulation counts are used for reproducible comparisons. The 1–3 ms
per one-rotation rollout assumed here is an estimate, not a measurement. As a reference
point, the current bot spends about 115 µs (4 players) to 186 µs (5 players) per turn
with every seat updating and choosing, i.e. about 1 ms per rotation. At that rate 2000
simulations take about 2 s on one thread, so the 750 ms default needs several cores or
fewer simulations. The guard in partner decisions (7.5) is the main cost driver to
watch.

### 7.5 Legibility guard

A candidate clue is permitted only if, in the receiver's sight and after the seats in
between have acted, the intended reading is the first surviving one. The check runs
through the same interpreter that reads real clues, so giving and reading cannot drift
apart.

A clue must also have a **purpose**, its intent (`conventions::ClueIntent`) as the giver
reads it: it gets a new card clued, promised or chop moved, or it is a stall while a
stall is called for, a valuable tempo clue, or a fix that removes errors. This is the
blueprint's minimum clue value, so the search never compares clues the blueprint would
not give. The intent also names the clue (rule, focus, the identity it plays as and the
connecting finesses and prompts) in decision traces, `explain` and the web client's log.

The seats in between are **not** predicted with the partner model, because that model
applies this guard itself and the check would recurse at every seat. They are assumed
to act on their convention obligations from the clue (the plays, prompts and finesse
responses its readings require) and otherwise by the first entry of the public
expected-action ranking (6.6). That ranking is computed without the guard, so the
recursion depth is one. Where the outcome depends on an in-between seat's free
choice, the clue must be legible for every choice in that seat's top expected
actions, not just one. Inside rollouts the guard result is cached per
(clue, receiver view) like any interpretation. Moves a convention explicitly supports — bluffs, positional misplays, inverted
actions — pass on their semantic check even when a risk heuristic dislikes them.

### 7.6 Value

The objective is the expected final team score under the table's strikeout scoring.
Perfect-game rate, strikeout risk and critical losses are diagnostics. Terminal states
use the real score. A cut-off uses a linear, named-term estimate **of that same
objective**: score, secured future plays, lost maximum score, strikes, exposed critical
chops, tokens, pace. The weights are fitted against blueprint games played to the end
(`raikan fit-value`): every position before the end-game gives the raw features and the
points the team still scored (a strikeout scores 0); least squares, per player count,
with the sign each weight must have by the rules (a strike never helps, a secured play
never hurts), because the fit is observational and a rare event correlated with the game
phase would otherwise get the wrong sign. Fitted weights live in `data/value-weights.json`
and replace the hand-set ones for their player count. Measured so far they predict the
final score far better (held-out R² 0.84–0.89 against 0.17 for 2–4 players) yet decide
worse in paired self-play, so the file ships empty: a good predictor of the score of
blueprint positions is not the same as a good judge of the difference between two
candidates, because the fit is observational. `fit-value --paired` fits the weights to
the final-score differences between candidates rolled out from the same position and
world instead; it ranks held-out pairs better (66% against 60%) but did not decide better
either (docs/parity.md).
This counters horizon bias: a clue whose payoff lies beyond one rotation is credited
through "secured plays". The breakdown is kept per rollout, so a value is always a sum of
named terms. Convention violations are not penalty constants; their cost shows up as
simulated misplays and lost tempo.

Learned priors, a learned value model or a calibrated partner model can replace the
hand-built ones later. They must see only the actor's information and be conditioned on
the profile.

## 8. Trace and explain

### 8.1 Data model

Serde structs, written as one JSON line per event or decision:

- **InterpretationTrace** (per observed event): proposals per rule with status, the
  readings per sight, retractions, and the diff of notes.
- **DecisionTrace** (per own turn): profile digest, configuration and seed; the view
  (hands as seen, notes with provenance); belief health; every candidate with blueprint
  rank and proposing rule, how teammates would read it, visits, mean, standard error,
  value breakdown, and a few sampled rollouts (world, each step with seat, action and
  the rule behind it, leaf breakdown); the chosen action and the selection reason,
  including the paired-difference statistics.

Tracing is off inside rollouts unless a rollout is sampled for display. Live play stores
a compact trace plus the seed; full detail is recomputed on demand.

### 8.2 Tools

```
raikan explain <replay> --turn 23            # decision summary and candidate table
raikan explain <replay> --turn 23 --candidate "clue 2 blue" --world 4
raikan explain <replay> --turn 23 --card 17  # why is this card believed to be b3?
raikan explain <replay> --turn 23 --level 5  # what would level 5 have done?
raikan diff <runA> <runB>                    # first diverging decision, both traces
raikan stats --players 4 --variant black --level 9 --search off|fast|full
```

The text renderer and the later web UI read the same structs. The web UI is an HTTP
server over a trace directory with a recompute endpoint ("re-run turn 23 with action X
forced"); it adds no data model of its own.

## 9. Configuration

```toml
[h_group]
max_level = 8
require_complete = true

[search]
mode = "full"          # off | fast | full
simulations = 2000
time_limit_ms = 750    # omit for reproducible runs
blueprint_margin = 1.0 # in standard errors; 0 = always take the best estimate
min_gain = 0.25        # in expected points; the search must beat the blueprint by this
to_end_pace = 0        # roll out to the end once pace < players + this
seed = 42              # otherwise derived from public history and turn

[teammates]
trust_epsilon = 0.02

[trace]
level = "compact"
dir = "traces"
```

The numbers are starting values to measure, not promises. The variant comes from the
table; a configured variant is only an expected-match check. The level has a process
default and a per-table override; how a table announces its level on hanab.live is an
open question (section 12).

## 10. Testing

- **One scenario format** for all tests: variant, profile, initial deal and a move list
  in text, replacing the four duplicated helpers. It drives the real simulator and checks
  every seat's knowledge after each event. Most convention tests assert permitted
  meanings and responses, not one preferred move.
- **Rule tests are keyed by rule id** and link to the convention's page; each rule is
  tested as giver, receiver and third party, with continuation, cancellation and
  negative examples.
- **Level-cap tests** at each boundary: disabled rules emit neither readings nor actions,
  and a disabled later rule restores the earlier meaning.
- **Replay tests** from seeds and hanab.live replays stay.
- **Self-play matrices**: score distribution per level × variant × player count, on
  paired deals, in blueprint and search mode; `diff` finds the first diverging decision
  between two builds.
- **Invariants**:
  - a seat's memoised view equals the independent per-seat interpreter's view;
  - identical observation histories give identical candidates, priors and partner
    distributions when hidden cards are swapped;
  - sampled worlds never contain an impossible copy, and small enumerated cases match
    the expected probabilities, for both the exact and the fallback sampler;
  - a cached interpretation equals an uncached one in every sampled world (a read that
    bypasses the context shows up as a mismatch);
  - the same seed gives the same decision for any thread count;
  - no chosen action is one the legibility guard rejects.

## 11. Existing code

| Keep as a concept | Drop |
| --- | --- |
| Record / hypothesis / step with stable card ids; Occam tiers that reactivate when lower tiers die (`interpretation.rs`) | Seat-relative indices and views made by cloning and rotating `Line` |
| Hard vs. soft knowledge, derived notes rebuilt by a fold (`refresh_derived`) | The `clued()` procedure mixing mechanics, interpretation and grading; `error += n` |
| The connection search order: queued, known, prompt, finesse by seat with layers, self; wrong-prompt fallback | Sentinels (`rank == 0`, `clued == Some(255)`) and true identities stored inside belief slots |
| Checking our own clue by reading it from each teammate's view | `LineScore` ordering and the unlogged early returns in action choice |
| Stall severity × kind table (`stall.rs`) | Per-slot marginal outcomes; rollouts without draws that stop at the first clue |
| Seeded games, replay import/export, `stats`, regression diff, replay tests | Fixed 5 × 5 layouts; `Suit`/`ClueColor` enums; conventions as `impl Line` methods |

A suggested build order, each step usable on its own: ids, variants and simulator →
knowledge and sight → records, interpreter and notes with levels 1–3 as rules, plus the
interpretation trace → blueprint and self-play parity with today's bot → root planner,
decision trace and `explain` → levels 4–9 and six-suit/black/rainbow/white → orange, web
UI → higher levels. `HyphenatedPlayer` and `game::PlayerStrategy` can stay as thin
adapters while callers move.

## 12. Open questions and risks

1. **Rollout cost and cache hit rate are estimates.** They decide the world count and
   horizon; measure them as soon as levels 1–3 run on the new interpreter. The
   prototype figures in sections 5, 6.1 and 7.4 come from a toy rule and the current
   bot; real rules may read our hand more often.
2. **Definition of clue worth.** Many level 14+ conventions hinge on "this clue was
   otherwise useless". It must be public and deterministic; its exact definition should
   be settled when level 4 (trash chop move) is implemented.
3. **Level negotiation on hanab.live.** Table name, chat command, or bot configuration.
4. **Teammate mistake model.** One ε, or per-teammate estimates learned during a game.
5. **Convention details above level 13** in this document are structural only; slot
   ordinals and exceptions must be read from the convention pages when each rule is
   written.
6. **Variant convention levels.** The variant pages do not assign levels; each adaptation
   needs a reviewed minimum level and prerequisites in the manifest.
