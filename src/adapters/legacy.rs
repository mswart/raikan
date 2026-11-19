//! Runs the original `hyphenated` bot on the new engine by translating absolute events into
//! the relative-seat / hand-position calls of `game::PlayerStrategy`.
//!
//! Only the standard five suits are supported, because the old bot hard-codes them.

use std::collections::VecDeque;

use crate::PositionSet;
use crate::agent::Agent;
use crate::domain::{CardId, Identity, Seat};
use crate::engine::{Action, Event, GameState};
use crate::game::{self, PlayerStrategy};
use crate::hyphenated::HyphenatedPlayer;
use crate::variants::{ClueValue, Variant};

const SUITS: [fn() -> game::Suit; 5] = [
    game::Suit::Red,
    game::Suit::Yellow,
    game::Suit::Green,
    game::Suit::Blue,
    game::Suit::Purple,
];
const COLORS: [fn() -> game::ClueColor; 5] = [
    game::ClueColor::Red,
    game::ClueColor::Yellow,
    game::ClueColor::Green,
    game::ClueColor::Blue,
    game::ClueColor::Purple,
];

pub struct LegacyAgent {
    player: Box<dyn PlayerStrategy>,
    variant: Option<Variant>,
    seat: Seat,
    num_seats: u8,
    /// Hands newest first, as the old engine stores them.
    hands: Vec<VecDeque<CardId>>,
}

impl LegacyAgent {
    pub fn new(player: Box<dyn PlayerStrategy>) -> Self {
        Self {
            player,
            variant: None,
            seat: Seat(0),
            num_seats: 0,
            hands: Vec::new(),
        }
    }

    pub fn hyphenated() -> Self {
        Self::new(Box::new(HyphenatedPlayer::new(false)))
    }

    fn variant(&self) -> &Variant {
        self.variant.as_ref().expect("start() must be called first")
    }

    fn relative(&self, seat: Seat) -> usize {
        self.seat.distance_to(seat, self.num_seats) as usize
    }

    fn card(&self, identity: Identity) -> game::Card {
        let variant = self.variant();
        game::Card {
            suit: SUITS[variant.suit_of(identity)](),
            rank: variant.rank_of(identity),
        }
    }

    fn take(&mut self, seat: Seat, card: CardId) -> usize {
        let hand = &mut self.hands[seat.index()];
        let pos = hand
            .iter()
            .position(|&c| c == card)
            .expect("card must be in hand");
        hand.remove(pos);
        pos
    }
}

impl Agent for LegacyAgent {
    fn start(&mut self, variant: &Variant, num_seats: u8, seat: Seat) {
        assert_eq!(
            variant.name, "No Variant",
            "the legacy bot only supports No Variant"
        );
        self.variant = Some(variant.clone());
        self.seat = seat;
        self.num_seats = num_seats;
        self.hands = vec![VecDeque::new(); num_seats as usize];
        self.player.init(num_seats, seat.0);
    }

    fn observe(&mut self, event: &Event) {
        match *event {
            Event::Draw {
                seat,
                card,
                identity,
            } => {
                self.hands[seat.index()].push_front(card);
                if seat == self.seat {
                    self.player.own_drawn();
                } else {
                    let card = self.card(identity.expect("others' cards are visible"));
                    self.player.drawn(self.relative(seat), card);
                }
            }
            Event::Play {
                seat,
                card,
                identity,
                success,
            } => {
                let pos = self.take(seat, card);
                let card = self.card(identity.expect("played cards are revealed"));
                self.player.played(self.relative(seat), pos, card, success);
            }
            Event::Discard {
                seat,
                card,
                identity,
            } => {
                let pos = self.take(seat, card);
                let card = self.card(identity.expect("discarded cards are revealed"));
                self.player.discarded(self.relative(seat), pos, card);
            }
            Event::Clue {
                giver,
                target,
                clue,
                touched,
            } => {
                let hand = &self.hands[target.index()];
                let mut positions = PositionSet::new(hand.len() as u8);
                for (pos, &card) in hand.iter().enumerate() {
                    if touched.contains(card) {
                        positions.add(pos as u8);
                    }
                }
                let clue = match self.variant().clue_value(clue) {
                    ClueValue::Color(c) => game::Clue::Color(COLORS[c as usize]()),
                    ClueValue::Rank(r) => game::Clue::Rank(r),
                };
                self.player
                    .clued(self.relative(giver), self.relative(target), clue, positions);
            }
            Event::GameOver { .. } => {}
        }
    }

    fn choose_action(&mut self, state: &GameState) -> Action {
        let variant = self.variant().clone();
        let status = game::GameStatus {
            turn: state.turn as u8,
            score: state.score(&variant),
            max_score: variant.max_reachable_score(&state.discarded),
            num_strikes: state.strikes,
            clues: state.clue_tokens(),
            blind_plays: 0,
        };
        let hand = &self.hands[self.seat.index()];
        let card_at = |pos: u8| hand.get(pos as usize).copied().unwrap_or(CardId(u8::MAX));
        match self.player.act(&status) {
            game::Move::Play(pos) => Action::Play(card_at(pos)),
            game::Move::Discard(pos) => Action::Discard(card_at(pos)),
            game::Move::Clue(offset, clue) => {
                let value = match clue {
                    game::Clue::Rank(r) => ClueValue::Rank(r),
                    game::Clue::Color(color) => ClueValue::Color(
                        COLORS
                            .iter()
                            .position(|c| c() == color)
                            .expect("five colours") as u8,
                    ),
                };
                // An out-of-range offset becomes a clue to oneself, which the engine rejects.
                let target = if offset == 0 || offset >= self.num_seats {
                    self.seat
                } else {
                    self.seat.after(offset, self.num_seats)
                };
                Action::Clue {
                    target,
                    clue: variant.clue_id(value).expect("No Variant has every clue"),
                }
            }
        }
    }
}
