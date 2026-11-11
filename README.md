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

You need to set `HANABI_USERNAME` and `HANABI_PASSWORD` environment variables to define which account to use for hanab.live.
Remember that new accounts are crated on-the-fly if the username isn't taken yet.

Send the bot a DM with `/join` as message to make it join your table.

## H-Group rule support

Experienced beginner maybe?
While limitations will come up, it allows for first successful games (sometimes even perfect ones).

Implemented features:

* Basic clue and save rules are implemented
* Play-clue interpretations track prompts, finesses, and layered finesses
* Blind-play and non-play evidence disambiguates pending connections
* Resolved finesse cards count as clued for good-touch deductions
* Action selection forecasts other players' plays and forced discards, accounting for clue tokens, strikes, and critical-card losses
* Urgent fixes or token-generating discards can take priority over an otherwise playable card
* 5 stalls in the early game and in double-discard, locked-hand, and eight-token situations
* Tempo, fill-in, and burn stalls in double-discard, locked-hand, and eight-token situations, with clue precedence
* Locked-hand saves on chop that leave the receiver an unclued card, and eight-clue saves on older cards after the opening turn

Core limitations:

* Anxiety plays and the 5-stall finesse-position exception are not implemented
* Stall interpretation is conservative; it does not infer finesses from the availability of higher-precedence clues
* Action forecasts stop at the next clue-giving choice or one table rotation; future draws and final-turn limits are not modeled
* No chop moves etc.
* No variants.

Action search averages possible identities of the bot's own cards using remaining
copy counts. It preserves the usual action priorities unless the forecast finds a
misplay, a forced loss in another hand, or a loss of a protected own card. The
background risk of discarding an ordinary unknown chop does not trigger a change.
The search separates candidates, transitions, and evaluation so it can later grow
into deeper expectimax or robust minimax: teammates remain cooperative, while
uncertain card outcomes can be averaged or evaluated pessimistically. Deeper
search will need joint hand beliefs, draw outcomes, and the final-turn counter.

Stalling follows the [H-Group stall table](https://hanabi.github.io/level-9#allowable-stall-clues-stall-table).
Normal plays, safe discards, and useful play/save clues take precedence over
stalls. The bot gives one early-game 5 stall when needed, choosing the five closest
to chop; further early-game 5 stalls are optional and are normally skipped.
5, fill-in, general saves, and burn stalls preserve literal information and existing connections
without adding a new play or finesse instruction. Tempo stalls request an
immediate play of the focus, without creating a connecting chain.
Locked-hand saves and eight-clue saves have equal precedence with fill-in clues,
after 5 and tempo stalls and before burns. Ordinary rank-2/rank-5 saves, literal
plays, and critical chop saves keep priority over these general saves.
Public actions track clue tokens, the opening turn, the first unknown discard,
and possible double-discard situations for every player view.

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
