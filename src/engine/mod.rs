//! Game mechanics: state, events and the simulator.
//!
//! [`step`] is the only implementation of transitions. Every other layer (and the log)
//! consumes the [`Event`]s it produces.

mod replay;
mod state;

pub use replay::{GameReference, Replay, ReplayAction, ReplayError};
pub use state::{Action, EndReason, GameState, Status, World, hand_size};

use crate::domain::{CardId, CardSet, ClueId, Identity, Seat};
use crate::variants::{ClueValue, Variant};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Something that happened in a game. Carries the command and its resolved effect.
///
/// Identity fields are `Option`s so that an event can be redacted for a viewpoint
/// ([`Event::observed_by`]): a drawer does not see the identity of their own card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Event {
    Draw {
        seat: Seat,
        card: CardId,
        identity: Option<Identity>,
    },
    Play {
        seat: Seat,
        card: CardId,
        identity: Option<Identity>,
        /// `false` is a misplay (strike); the card goes to the discard pile.
        success: bool,
    },
    Discard {
        seat: Seat,
        card: CardId,
        identity: Option<Identity>,
    },
    Clue {
        giver: Seat,
        target: Seat,
        clue: ClueId,
        touched: CardSet,
    },
    GameOver {
        reason: EndReason,
    },
}

impl Event {
    /// The event as `seat` observes it.
    pub fn observed_by(&self, seat: Seat) -> Event {
        match *self {
            Event::Draw {
                seat: drawer, card, ..
            } if drawer == seat => Event::Draw {
                seat: drawer,
                card,
                identity: None,
            },
            other => other,
        }
    }

    /// Whether the event is a turn action (play, discard or clue).
    pub fn is_action(&self) -> bool {
        matches!(
            self,
            Event::Play { .. } | Event::Discard { .. } | Event::Clue { .. }
        )
    }
}

/// Events produced by one call of [`step`] or [`deal`].
pub type Events = SmallVec<[Event; 3]>;

/// Shuffle a deck for `seed`. For the same seed this produces the same deck as the
/// original `game::Game::new` (catalogue deck order, PCG64 shuffle).
pub fn shuffled_world(variant: &Variant, seed: u64) -> World {
    let mut rng = rand_pcg::Pcg64::seed_from_u64(seed);
    let mut deck = variant.deck_order();
    deck.shuffle(&mut rng);
    World::new(&deck)
}

/// Deal the starting hands: every seat in order draws a full hand.
pub fn deal(variant: &Variant, num_seats: u8, world: &World) -> (GameState, Vec<Event>) {
    let mut state = GameState::new(variant, num_seats);
    let mut events = Vec::new();
    for seat in state.seats() {
        for _ in 0..state.hand_size {
            if let Some(event) = draw(&mut state, world, seat) {
                events.push(event);
            }
        }
    }
    (state, events)
}

fn draw(state: &mut GameState, world: &World, seat: Seat) -> Option<Event> {
    if state.cards_left() == 0 {
        return None;
    }
    let card = CardId(state.next_card);
    state.next_card += 1;
    state.push_card(seat, card);
    if state.cards_left() == 0 {
        state.final_turns = Some(state.num_seats);
    }
    Some(Event::Draw {
        seat,
        card,
        identity: Some(world.identity(card)),
    })
}

/// Why a command is not legal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Illegal {
    GameOver,
    NotInHand,
    DiscardAtMaxTokens,
    NoTokens,
    BadTarget,
    EmptyClue,
    /// Alternating clues: the same clue type as the previous clue.
    SameClueType,
}

/// Check whether the current seat may submit `action`.
pub fn check_legal(variant: &Variant, state: &GameState, action: Action) -> Result<(), Illegal> {
    if state.is_over() {
        return Err(Illegal::GameOver);
    }
    match action {
        Action::Play(card) => {
            if !state.hand(state.current).contains(&card) {
                return Err(Illegal::NotInHand);
            }
        }
        Action::Discard(card) => {
            if !state.hand(state.current).contains(&card) {
                return Err(Illegal::NotInHand);
            }
            if state.tokens >= variant.tokens.max && !variant.has_inverted() {
                return Err(Illegal::DiscardAtMaxTokens);
            }
        }
        Action::Clue { target, clue } => {
            if target == state.current || target.0 >= state.num_seats {
                return Err(Illegal::BadTarget);
            }
            if state.tokens < variant.tokens.clue_cost {
                return Err(Illegal::NoTokens);
            }
            if clue.index() >= variant.num_clues() {
                return Err(Illegal::BadTarget);
            }
            if variant.alternating_clues
                && state.last_clue_rank
                    == Some(matches!(variant.clue_value(clue), ClueValue::Rank(_)))
            {
                return Err(Illegal::SameClueType);
            }
            // Touch is decided by the real cards, so emptiness is checked in `step`.
        }
    }
    Ok(())
}

/// All mechanically legal commands for the current seat whose legality does not depend on
/// hidden cards. Clues are included only if they touch at least one card in `world`.
pub fn legal_commands(variant: &Variant, state: &GameState, world: &World) -> Vec<Action> {
    let mut actions = Vec::new();
    if state.is_over() {
        return actions;
    }
    let seat = state.current;
    for &card in state.hand(seat) {
        actions.push(Action::Play(card));
    }
    if state.tokens < variant.tokens.max {
        for &card in state.hand(seat) {
            actions.push(Action::Discard(card));
        }
    }
    if state.tokens >= variant.tokens.clue_cost {
        for target in state.seats().filter(|&s| s != seat) {
            for clue in variant.clue_ids() {
                if !touched_cards(variant, state, world, target, clue).is_empty() {
                    actions.push(Action::Clue { target, clue });
                }
            }
        }
    }
    actions
}

/// The cards in `target`'s hand that `clue` touches.
pub fn touched_cards(
    variant: &Variant,
    state: &GameState,
    world: &World,
    target: Seat,
    clue: ClueId,
) -> CardSet {
    let touch = variant.touch(clue);
    state
        .hand(target)
        .iter()
        .copied()
        .filter(|&c| touch.contains(world.identity(c)))
        .collect()
}

/// Apply an observed event to a public state. This is the observer-side mirror of [`step`]:
/// it needs no world, because every event carries what it changes publicly.
pub fn apply_event(variant: &Variant, state: &mut GameState, event: &Event) {
    let tokens = variant.tokens;
    let end_turn = |state: &mut GameState| {
        state.turn += 1;
        state.current = state.current.after(1, state.num_seats);
        if let Some(left) = state.final_turns.as_mut() {
            *left = left.saturating_sub(1);
        }
    };
    match *event {
        Event::Draw { seat, card, .. } => {
            // Observed histories may be partial; never move the deck pointer backwards.
            state.next_card = state.next_card.max(card.0 + 1);
            state.push_card(seat, card);
            if state.cards_left() == 0 {
                // the drawer's own turn already ended; everybody gets one more turn
                state.final_turns = Some(state.num_seats);
            }
        }
        Event::Play {
            seat,
            card,
            identity,
            success,
        } => {
            state.remove_card(seat, card);
            if let Some(identity) = identity {
                if success {
                    let completed = variant.play(&mut state.stacks, identity);
                    if completed && state.tokens < tokens.max {
                        state.tokens = (state.tokens + tokens.stack_complete_gain).min(tokens.max);
                    }
                } else {
                    state.discarded[identity.index()] += 1;
                }
            }
            if !success {
                state.strikes += 1;
            }
            end_turn(state);
        }
        Event::Discard {
            seat,
            card,
            identity,
        } => {
            state.remove_card(seat, card);
            if let Some(identity) = identity {
                state.discarded[identity.index()] += 1;
            }
            state.tokens = (state.tokens + tokens.discard_gain).min(tokens.max);
            end_turn(state);
        }
        Event::Clue { touched, clue, .. } => {
            state.tokens = state.tokens.saturating_sub(tokens.clue_cost);
            state.clued |= touched;
            state.last_clue_rank = Some(matches!(variant.clue_value(clue), ClueValue::Rank(_)));
            end_turn(state);
        }
        Event::GameOver { reason } => state.status = Status::Over(reason),
    }
}

/// Apply one command of the current seat. Illegal commands end the game with
/// [`EndReason::Illegal`]. An inverted (orange) card turns a play command into a discard
/// and a discard command into a play.
pub fn step(variant: &Variant, state: &mut GameState, world: &World, action: Action) -> Events {
    step_as(variant, state, world, action, true)
}

/// Apply one recorded action of a replay. Replays record the effect (hanab.live's
/// "recorded" interpretation), so inverted cards are not flipped again.
pub fn step_recorded(
    variant: &Variant,
    state: &mut GameState,
    world: &World,
    action: Action,
) -> Events {
    step_as(variant, state, world, action, false)
}

fn step_as(
    variant: &Variant,
    state: &mut GameState,
    world: &World,
    action: Action,
    live: bool,
) -> Events {
    let mut events = Events::new();
    let seat = state.current;
    let mut touched = CardSet::EMPTY;
    let legal = check_legal(variant, state, action).and_then(|()| {
        if let Action::Clue { target, clue } = action {
            touched = touched_cards(variant, state, world, target, clue);
            if touched.is_empty() && !variant.may_touch_nothing(clue) {
                return Err(Illegal::EmptyClue);
            }
        }
        Ok(())
    });
    if legal.is_err() {
        if !state.is_over() {
            state.status = Status::Over(EndReason::Illegal);
            events.push(Event::GameOver {
                reason: EndReason::Illegal,
            });
        }
        return events;
    }

    let final_turns_before = state.final_turns;
    state.turn += 1;
    let tokens = variant.tokens;
    // An inverted (orange) card turns a play command into a discard and a discard command
    // into a play: the event is the effect.
    let effect = match action {
        Action::Play(card) | Action::Discard(card) => {
            let inverted = live && variant.suits[variant.suit_of(world.identity(card))].inverted;
            let play = matches!(action, Action::Play(_)) != inverted;
            Some((card, play))
        }
        Action::Clue { .. } => None,
    };
    match (action, effect) {
        (_, Some((card, false))) => {
            state.remove_card(seat, card);
            let identity = world.identity(card);
            state.discarded[identity.index()] += 1;
            state.tokens = (state.tokens + tokens.discard_gain).min(tokens.max);
            events.push(Event::Discard {
                seat,
                card,
                identity: Some(identity),
            });
        }
        (_, Some((card, true))) => {
            state.remove_card(seat, card);
            let identity = world.identity(card);
            let success = variant.playable(&state.stacks).contains(identity);
            if success {
                let completed = variant.play(&mut state.stacks, identity);
                if completed && state.tokens < tokens.max {
                    state.tokens = (state.tokens + tokens.stack_complete_gain).min(tokens.max);
                }
            } else {
                state.discarded[identity.index()] += 1;
                state.strikes += 1;
            }
            events.push(Event::Play {
                seat,
                card,
                identity: Some(identity),
                success,
            });
            if state.strikes >= 3 {
                state.current = seat.after(1, state.num_seats);
                state.status = Status::Over(EndReason::Strikeout);
                events.push(Event::GameOver {
                    reason: EndReason::Strikeout,
                });
                return events;
            }
        }
        (Action::Clue { target, clue }, None) => {
            state.tokens -= tokens.clue_cost;
            state.clued |= touched;
            state.last_clue_rank = Some(matches!(variant.clue_value(clue), ClueValue::Rank(_)));
            events.push(Event::Clue {
                giver: seat,
                target,
                clue,
                touched,
            });
        }
        (Action::Play(_) | Action::Discard(_), None) => {
            unreachable!("effects exist exactly for card commands")
        }
    }

    if !matches!(action, Action::Clue { .. })
        && let Some(event) = draw(state, world, seat)
    {
        events.push(event);
    }

    state.current = seat.after(1, state.num_seats);

    if state.score(variant) == variant.max_score() {
        state.status = Status::Over(EndReason::Normal);
        events.push(Event::GameOver {
            reason: EndReason::Normal,
        });
    } else if let Some(left) = final_turns_before {
        let left = left - 1;
        state.final_turns = Some(left);
        if left == 0 {
            state.status = Status::Over(EndReason::Normal);
            events.push(Event::GameOver {
                reason: EndReason::Normal,
            });
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(seed: u64, seats: u8) -> (Variant, GameState, World) {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, seed);
        let (state, events) = deal(&variant, seats, &world);
        assert_eq!(events.len(), (seats * hand_size(seats)) as usize);
        (variant, state, world)
    }

    #[test]
    fn initial_state() {
        let (variant, state, world) = setup(0, 4);
        assert_eq!(
            state.hand(Seat(0)),
            &[CardId(0), CardId(1), CardId(2), CardId(3)]
        );
        assert_eq!(state.hand(Seat(3)).len(), 4);
        assert_eq!(state.cards_left(), 34);
        assert_eq!(state.clue_tokens(), 8);
        assert_eq!(world.len(), 50);
        // discarding at 8 clues is illegal, playing and cluing is legal
        let legal = legal_commands(&variant, &state, &world);
        assert!(!legal.iter().any(|a| matches!(a, Action::Discard(_))));
        assert_eq!(
            legal
                .iter()
                .filter(|a| matches!(a, Action::Play(_)))
                .count(),
            4
        );
        assert!(legal.iter().any(|a| matches!(a, Action::Clue { .. })));
    }

    fn play_game(variant: &Variant, seed: u64) -> GameState {
        let world = shuffled_world(variant, seed);
        let (mut state, _) = deal(variant, 3, &world);
        // a fixed, simple policy: play the newest card unless it would be the third strike
        while !state.is_over() {
            let hand = state.hand(state.current);
            let card = hand[hand.len() - 1];
            let action = if state.strikes < 2 || state.tokens >= variant.tokens.max {
                Action::Play(card)
            } else {
                Action::Discard(hand[0])
            };
            step(variant, &mut state, &world, action);
        }
        state
    }

    #[test]
    fn inverted_cards_swap_play_and_discard() {
        let variant = Variant::by_name("Orange (6 Suits)").unwrap();
        let orange = variant.suits.iter().position(|s| s.inverted).unwrap();
        // a deal that gives seat 0 an orange card
        let (world, mut state, card) = (0..100)
            .find_map(|seed| {
                let world = shuffled_world(&variant, seed);
                let (state, _) = deal(&variant, 3, &world);
                let card = state
                    .hand(Seat(0))
                    .iter()
                    .copied()
                    .find(|&c| variant.suit_of(world.identity(c)) == orange)?;
                Some((world, state, card))
            })
            .expect("some deal has an orange card in seat 0's hand");
        // a discard at maximum tokens is legal in variants with inverted suits
        assert!(check_legal(&variant, &state, Action::Discard(card)).is_ok());
        let before = state;
        let events = step(&variant, &mut state, &world, Action::Play(card));
        assert!(matches!(events[0], Event::Discard { card: c, .. } if c == card));
        assert_eq!(state.strikes, 0);
        // a replay records the effect and applies it unflipped
        let mut replayed = before;
        step_recorded(&variant, &mut replayed, &world, Action::Discard(card));
        assert_eq!(replayed, state);
    }

    #[test]
    fn alternating_clues() {
        let variant = Variant::by_name("Alternating Clues (5 Suits)").unwrap();
        let world = shuffled_world(&variant, 1);
        let (mut state, _) = deal(&variant, 3, &world);
        let rank_clue = |state: &GameState| {
            variant
                .clue_ids()
                .filter(|&c| matches!(variant.clue_value(c), ClueValue::Rank(_)))
                .find(|&c| !touched_cards(&variant, state, &world, Seat(1), c).is_empty())
                .unwrap()
        };
        let clue = rank_clue(&state);
        step(
            &variant,
            &mut state,
            &world,
            Action::Clue {
                target: Seat(1),
                clue,
            },
        );
        // seat 1 may not give a rank clue now
        let again = Action::Clue {
            target: Seat(2),
            clue,
        };
        assert_eq!(
            check_legal(&variant, &state, again),
            Err(Illegal::SameClueType)
        );
    }

    #[test]
    fn empty_clues_are_legal_where_the_variant_allows() {
        let variant = Variant::by_name("Color Blind (5 Suits)").unwrap();
        let world = shuffled_world(&variant, 2);
        let (mut state, _) = deal(&variant, 3, &world);
        let red = variant.clue_id(ClueValue::Color(0)).unwrap();
        let events = step(
            &variant,
            &mut state,
            &world,
            Action::Clue {
                target: Seat(1),
                clue: red,
            },
        );
        assert!(matches!(events[0], Event::Clue { touched, .. } if touched.is_empty()));
        assert_eq!(state.clue_tokens(), 7);
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 2);
        let (mut state, _) = deal(&variant, 3, &world);
        let absent = variant
            .clue_ids()
            .find(|&c| touched_cards(&variant, &state, &world, Seat(1), c).is_empty());
        if let Some(clue) = absent {
            let events = step(
                &variant,
                &mut state,
                &world,
                Action::Clue {
                    target: Seat(1),
                    clue,
                },
            );
            assert!(matches!(
                events[0],
                Event::GameOver {
                    reason: EndReason::Illegal
                }
            ));
        }
    }

    #[test]
    fn new_stack_kinds_play_to_the_end() {
        for name in [
            "Reversed (6 Suits)",
            "Up or Down (5 Suits)",
            "Sudoku (5 Suits)",
            "Orange (6 Suits)",
        ] {
            let variant = Variant::by_name(name).unwrap();
            for seed in 0..20 {
                let state = play_game(&variant, seed);
                assert!(state.is_over(), "{name}");
                assert!(state.score(&variant) <= variant.max_score());
            }
        }
    }

    #[test]
    fn redaction() {
        let (_, _, world) = setup(0, 3);
        let event = Event::Draw {
            seat: Seat(1),
            card: CardId(7),
            identity: Some(world.identity(CardId(7))),
        };
        assert_eq!(
            event.observed_by(Seat(1)),
            Event::Draw {
                seat: Seat(1),
                card: CardId(7),
                identity: None
            }
        );
        assert_eq!(event.observed_by(Seat(0)), event);
    }

    #[test]
    fn illegal_moves_end_the_game() {
        let (variant, mut state, world) = setup(1, 3);
        let card = state.hand(Seat(0))[0];
        let events = step(&variant, &mut state, &world, Action::Discard(card));
        assert_eq!(
            events.as_slice(),
            &[Event::GameOver {
                reason: EndReason::Illegal
            }]
        );
        assert!(state.is_over());

        let (variant, mut state, world) = setup(1, 3);
        let clue = variant.clue_ids().next().unwrap();
        let events = step(
            &variant,
            &mut state,
            &world,
            Action::Clue {
                target: Seat(0),
                clue,
            },
        );
        assert_eq!(
            events.as_slice(),
            &[Event::GameOver {
                reason: EndReason::Illegal
            }]
        );
    }

    #[test]
    fn clues_need_tokens_and_a_valid_target() {
        let (variant, mut state, world) = setup(4, 3);
        let target = Seat(1);
        let clue = variant
            .clue_ids()
            .find(|&c| !touched_cards(&variant, &state, &world, target, c).is_empty())
            .unwrap();
        assert_eq!(
            check_legal(
                &variant,
                &state,
                Action::Clue {
                    target: Seat(3),
                    clue
                }
            ),
            Err(Illegal::BadTarget)
        );
        state.tokens = 0;
        assert_eq!(
            check_legal(&variant, &state, Action::Clue { target, clue }),
            Err(Illegal::NoTokens)
        );
        assert!(
            !legal_commands(&variant, &state, &world)
                .iter()
                .any(|a| matches!(a, Action::Clue { .. }))
        );
        let events = step(&variant, &mut state, &world, Action::Clue { target, clue });
        assert_eq!(
            events.as_slice(),
            &[Event::GameOver {
                reason: EndReason::Illegal
            }]
        );
    }

    #[test]
    fn three_strikes() {
        let (variant, mut state, world) = setup(2, 2);
        let mut strikes = 0;
        while !state.is_over() {
            let seat = state.current;
            // play the first card that is not playable, otherwise give any legal clue
            let bad = state
                .hand(seat)
                .iter()
                .copied()
                .find(|&c| !variant.playable(&state.stacks).contains(world.identity(c)));
            let action = match bad {
                Some(card) => {
                    strikes += 1;
                    Action::Play(card)
                }
                None => legal_commands(&variant, &state, &world)
                    .into_iter()
                    .find(|a| matches!(a, Action::Clue { .. }))
                    .unwrap(),
            };
            step(&variant, &mut state, &world, action);
        }
        assert_eq!(strikes, 3);
        assert_eq!(state.status, Status::Over(EndReason::Strikeout));
    }

    #[test]
    fn final_round() {
        // Discard-only game: every seat gets exactly one more turn after the last draw.
        let (variant, mut state, world) = setup(3, 3);
        let mut turns_after_last_draw = None;
        while !state.is_over() {
            let seat = state.current;
            let action = if state.tokens >= variant.tokens.max {
                let target = seat.after(1, state.num_seats);
                let clue = variant
                    .clue_ids()
                    .find(|&c| !touched_cards(&variant, &state, &world, target, c).is_empty())
                    .unwrap();
                Action::Clue { target, clue }
            } else {
                Action::Discard(state.hand(seat)[0])
            };
            step(&variant, &mut state, &world, action);
            if let Some(n) = turns_after_last_draw.as_mut() {
                *n += 1;
            } else if state.cards_left() == 0 {
                turns_after_last_draw = Some(0);
            }
        }
        assert_eq!(turns_after_last_draw, Some(3));
        assert_eq!(state.status, Status::Over(EndReason::Normal));
    }

    #[test]
    fn observed_events_reproduce_the_state() {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 21);
        let (mut state, events) = deal(&variant, 3, &world);
        let mut observed = GameState::new(&variant, 3);
        for event in &events {
            apply_event(&variant, &mut observed, &event.observed_by(Seat(1)));
        }
        assert_eq!(observed, state);
        let mut turn = 0;
        while !state.is_over() {
            let legal = legal_commands(&variant, &state, &world);
            let action = legal[(turn * 7) % legal.len()];
            turn += 1;
            for event in step(&variant, &mut state, &world, action) {
                apply_event(&variant, &mut observed, &event.observed_by(Seat(1)));
            }
            assert_eq!(observed, state, "turn {turn}");
        }
    }
}
