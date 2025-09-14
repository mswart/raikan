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
* Some/most finess clues work

Core limitations:

* No stalling
* Finessed cards aren't considered "clued" and therefore often recluded :-(
* Eager action selection (if the bot can clue is probably will, even if it will force the next player to chop a critical card)
* No stalling
* No chop moves etc.
* No variants.

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
