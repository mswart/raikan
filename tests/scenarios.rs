//! Rule-keyed convention scenarios.
//!
//! Each file in `tests/scenarios/` describes a table in text, drives the real simulator with
//! one `BotAgent` per seat, and checks every seat's knowledge after each command:
//!
//! ```text
//! # rule: L1.save-5                      rule id(s) the scenario covers
//! level 9                                optional profile cap (default 9)
//! search full                            optional root search mode (default off)
//! variant No Variant                     optional (default No Variant)
//! hand 0: r1 y2 g3 b4                    starting hands, slot 1 (newest) first
//! hand 1: ...
//! draws: r2 r3                           next cards of the deck (rest: catalogue order)
//! clue 0 1 5                             seat 0 clues seat 1 with rank 5 (or a colour name)
//! play 1 2                               seat 1 plays its slot 2 (1 = newest)
//! discard 1 4
//! note 2 1 4: r5,y5 save                 seat 2's view of seat 1 slot 4: identities + flags
//! note * 1 4: r5                         every seat's view
//! action 1: play 3 | discard 4 | clue 2 red    the bot of seat 1 chooses this (no step)
//! not-action 1: clue 2 red
//! ```
//!
//! Flags after the identities: `play`, `noplay`, `trash`, `notrash`, `clued`, `promised`,
//! `delayed`. Identities may be `?` to skip that check.

use std::path::Path;

use raikan::agent::{Agent, BotAgent, Config};
use raikan::conventions::View;
use raikan::domain::{CardId, Identity, IdentitySet, Seat};
use raikan::engine::{self, Action, GameState, World};
use raikan::variants::{ClueValue, Variant};

struct Table {
    variant: Variant,
    world: World,
    state: GameState,
    bots: Vec<BotAgent>,
}

fn parse_identities(variant: &Variant, text: &str) -> Vec<Identity> {
    text.split_whitespace()
        .map(|t| {
            variant
                .parse_identity(t)
                .unwrap_or_else(|| panic!("unknown card {t}"))
        })
        .collect()
}

impl Table {
    fn slot(&self, seat: Seat, slot: usize) -> CardId {
        let hand = self.state.hand(seat);
        assert!(
            (1..=hand.len()).contains(&slot),
            "seat {} has no slot {slot}",
            seat.0
        );
        hand[hand.len() - slot]
    }

    fn clue(&self, text: &str) -> raikan::domain::ClueId {
        let value = match text.parse::<u8>() {
            Ok(rank) => ClueValue::Rank(rank),
            Err(_) => {
                let index = self
                    .variant
                    .clue_colors
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(text))
                    .unwrap_or_else(|| panic!("unknown clue {text}"));
                ClueValue::Color(index as u8)
            }
        };
        self.variant.clue_id(value).expect("clue exists")
    }

    fn parse_action(&self, seat: Seat, words: &[&str]) -> Action {
        match words {
            ["play", slot] => Action::Play(self.slot(seat, slot.parse().unwrap())),
            ["discard", slot] => Action::Discard(self.slot(seat, slot.parse().unwrap())),
            ["clue", target, clue] => Action::Clue {
                target: Seat(target.parse().unwrap()),
                clue: self.clue(clue),
            },
            other => panic!("bad action {other:?}"),
        }
    }

    fn step(&mut self, action: Action) {
        let events = engine::step(&self.variant, &mut self.state, &self.world, action);
        assert!(
            !events.iter().any(|e| matches!(
                e,
                engine::Event::GameOver {
                    reason: engine::EndReason::Illegal
                }
            )),
            "illegal action {action:?}"
        );
        for event in &events {
            for (seat, bot) in self.bots.iter_mut().enumerate() {
                bot.observe(&event.observed_by(Seat(seat as u8)));
            }
        }
    }
}

fn check_note(view: &View, variant: &Variant, card: CardId, spec: &str, context: &str) {
    let note = view.note(card);
    let mut words = spec.split_whitespace();
    let identities = words.next().unwrap_or("?");
    if identities != "?" {
        let expected: IdentitySet = if identities == "{}" {
            IdentitySet::EMPTY
        } else {
            identities
                .split(',')
                .map(|t| {
                    variant
                        .parse_identity(t)
                        .unwrap_or_else(|| panic!("unknown card {t}"))
                })
                .collect()
        };
        let names = |set: IdentitySet| {
            set.iter()
                .map(|i| variant.identity_name(i))
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(
            note.belief.soft,
            expected,
            "{context}: identities {{{}}} != expected {{{}}}\n{view:?}",
            names(note.belief.soft),
            names(expected)
        );
    }
    for flag in words {
        let ok = match flag {
            "play" => note.play && note.delayed == 0,
            "noplay" => !note.play,
            "trash" => note.trash,
            "notrash" => !note.trash,
            "clued" => note.clued,
            "unclued" => !note.clued,
            "promised" => note.promised.is_some(),
            "delayed" => note.delayed > 0,
            other => panic!("unknown flag {other}"),
        };
        assert!(
            ok,
            "{context}: flag {flag} does not hold for {note:?}\n{view:?}"
        );
    }
}

fn run(path: &Path) {
    let text = std::fs::read_to_string(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let mut level = 9;
    let mut search = raikan::search::SearchConfig::default();
    let mut variant = Variant::no_variant();
    let mut hands: Vec<Vec<Identity>> = Vec::new();
    let mut draws: Vec<Identity> = Vec::new();
    let mut commands: Vec<(usize, String)> = Vec::new();
    let mut has_rule = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with("# rule:") {
            has_rule = true;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("level ") {
            level = rest.trim().parse().unwrap();
        } else if let Some(rest) = line.strip_prefix("search ") {
            search = raikan::search::SearchConfig::new(rest.trim().parse().unwrap());
        } else if let Some(rest) = line.strip_prefix("variant ") {
            variant = Variant::by_name(rest.trim()).unwrap();
        } else if let Some(rest) = line.strip_prefix("hand ") {
            let (seat, cards) = rest.split_once(':').unwrap();
            assert_eq!(
                seat.trim().parse::<usize>().unwrap(),
                hands.len(),
                "{name}: hands in seat order"
            );
            hands.push(parse_identities(&variant, cards));
        } else if let Some(rest) = line.strip_prefix("draws:") {
            draws = parse_identities(&variant, rest);
        } else {
            commands.push((n + 1, line.to_string()));
        }
    }
    assert!(
        has_rule,
        "{name}: scenarios must name their rule with `# rule:`"
    );
    let seats = hands.len() as u8;
    // Deck: hands in deal order (seat by seat, oldest first), then the draws, then the
    // remaining copies in catalogue order.
    let mut deck: Vec<Identity> = Vec::new();
    for hand in &hands {
        assert_eq!(
            hand.len(),
            engine::hand_size(seats) as usize,
            "{name}: hand size"
        );
        deck.extend(hand.iter().rev());
    }
    deck.extend(&draws);
    let mut remaining = variant.deck_order();
    for identity in &deck {
        let pos = remaining
            .iter()
            .position(|i| i == identity)
            .unwrap_or_else(|| {
                panic!(
                    "{name}: too many copies of {}",
                    variant.identity_name(*identity)
                )
            });
        remaining.remove(pos);
    }
    deck.extend(remaining);
    let world = World::new(&deck);
    let mut bots: Vec<BotAgent> = (0..seats)
        .map(|_| {
            BotAgent::new(Config {
                max_level: level,
                search: search.clone(),
                ..Config::default()
            })
        })
        .collect();
    for (seat, bot) in bots.iter_mut().enumerate() {
        bot.start(&variant, seats, Seat(seat as u8));
    }
    let (state, events) = engine::deal(&variant, seats, &world);
    for event in &events {
        for (seat, bot) in bots.iter_mut().enumerate() {
            bot.observe(&event.observed_by(Seat(seat as u8)));
        }
    }
    let mut table = Table {
        variant,
        world,
        state,
        bots,
    };

    for (line_no, command) in commands {
        let context = format!("{name}:{line_no} `{command}`");
        let words: Vec<&str> = command.split_whitespace().collect();
        match words[0] {
            "clue" | "play" | "discard" => {
                let seat = Seat(words[1].parse().unwrap());
                assert_eq!(
                    table.state.current, seat,
                    "{context}: not seat {}'s turn",
                    seat.0
                );
                let mut action_words = vec![words[0]];
                action_words.extend(&words[2..]);
                let action = table.parse_action(seat, &action_words);
                table.step(action);
            }
            "note" => {
                let (head, spec) = command["note".len()..]
                    .split_once(':')
                    .expect("note V S N: spec");
                let parts: Vec<&str> = head.split_whitespace().collect();
                let holder = Seat(parts[1].parse().unwrap());
                let card = table.slot(holder, parts[2].parse().unwrap());
                let viewers: Vec<u8> = if parts[0] == "*" {
                    (0..seats).collect()
                } else {
                    vec![parts[0].parse().unwrap()]
                };
                for viewer in viewers {
                    check_note(
                        table.bots[viewer as usize].view(),
                        &table.variant,
                        card,
                        spec,
                        &format!("{context} (view of {viewer})"),
                    );
                }
            }
            "action" | "not-action" => {
                let (head, spec) = command.split_once(':').expect("action S: ...");
                let seat = Seat(head.split_whitespace().nth(1).unwrap().parse().unwrap());
                assert_eq!(
                    table.state.current, seat,
                    "{context}: not seat {}'s turn",
                    seat.0
                );
                let spec_words: Vec<&str> = spec.split_whitespace().collect();
                let expected = table.parse_action(seat, &spec_words);
                let state = table.state;
                let chosen = table.bots[seat.index()].choose_action(&state);
                let reason = table.bots[seat.index()].last_decision().map(|d| d.reason);
                if std::env::var_os("SCENARIO_PLAN").is_some()
                    && let Some(plan) = table.bots[seat.index()].last_plan()
                {
                    eprintln!("{context}: {}", serde_json::to_string_pretty(plan).unwrap());
                }
                if words[0] == "action" {
                    assert_eq!(
                        chosen,
                        expected,
                        "{context}: chose {chosen:?} ({reason:?})\n{:?}",
                        table.bots[seat.index()].view()
                    );
                } else {
                    assert_ne!(chosen, expected, "{context}: chose it ({reason:?})");
                }
            }
            other => panic!("{context}: unknown command {other}"),
        }
    }
}

#[test]
fn scenarios() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let mut failures = Vec::new();
    for file in &files {
        if let Err(panic) = std::panic::catch_unwind(|| run(file)) {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failures.push(format!("{}: {message}", file.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenarios failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n\n")
    );
}
