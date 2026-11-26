//! End-game conventions (level 8) need an empty deck, which the text scenarios cannot reach
//! cheaply. These tests script a two-player 3-suit game: Bob (seat 0) only clues, Alice
//! (seat 1) discards until the deck is empty, then the positional discard is checked.

use raikan::agent::{Agent, BotAgent, Config};
use raikan::domain::{CardId, Identity, Seat};
use raikan::engine::{self, Action, GameState, World};
use raikan::variants::{ClueValue, Variant};

struct Table {
    variant: Variant,
    world: World,
    state: GameState,
    bots: Vec<BotAgent>,
}

impl Table {
    /// Two players; Bob's (seat 0) and Alice's (seat 1) hands newest first, then the
    /// draw pile in order.
    fn new(bob: &str, alice: &str, draws: &str) -> Self {
        let variant = Variant::by_name("3 Suits").unwrap();
        let parse = |text: &str| -> Vec<Identity> {
            text.split_whitespace()
                .map(|t| variant.parse_identity(t).unwrap())
                .collect()
        };
        let mut deck: Vec<Identity> = Vec::new();
        for hand in [parse(bob), parse(alice)] {
            deck.extend(hand.iter().rev());
        }
        deck.extend(parse(draws));
        assert_eq!(deck.len(), variant.deck_order().len(), "a complete deck");
        let world = World::new(&deck);
        let mut bots: Vec<BotAgent> = (0..2)
            .map(|_| {
                BotAgent::new(Config {
                    max_level: 8,
                    ..Config::default()
                })
            })
            .collect();
        for (seat, bot) in bots.iter_mut().enumerate() {
            bot.start(&variant, 2, Seat(seat as u8));
        }
        let (state, events) = engine::deal(&variant, 2, &world);
        for event in &events {
            for (seat, bot) in bots.iter_mut().enumerate() {
                bot.observe(&event.observed_by(Seat(seat as u8)));
            }
        }
        Self {
            variant,
            world,
            state,
            bots,
        }
    }

    fn slot(&self, seat: u8, slot: usize) -> CardId {
        let hand = self.state.hand(Seat(seat));
        hand[hand.len() - slot]
    }

    fn step(&mut self, action: Action) {
        let events = engine::step(&self.variant, &mut self.state, &self.world, action);
        for event in &events {
            for (seat, bot) in self.bots.iter_mut().enumerate() {
                bot.observe(&event.observed_by(Seat(seat as u8)));
            }
        }
    }

    /// Bob clues Alice's oldest card (which she discards next), touching as few other
    /// cards as possible.
    fn bob_clues(&mut self) {
        let hand = self.state.hand(Seat(1)).to_vec();
        let oldest = self.world.identity(hand[0]);
        let suit = self.variant.suit_of(oldest);
        let candidates = [
            ClueValue::Rank(self.variant.rank_of(oldest)),
            ClueValue::Color(suit as u8),
        ];
        let clue = candidates
            .iter()
            .map(|&value| self.variant.clue_id(value).unwrap())
            .min_by_key(|&clue| {
                let touch = self.variant.touch(clue);
                hand.iter()
                    .filter(|&&c| touch.contains(self.world.identity(c)))
                    .count()
            })
            .unwrap();
        self.step(Action::Clue {
            target: Seat(1),
            clue,
        });
    }

    /// Bob clues, Alice discards her oldest card, until the deck is empty.
    fn run_out_the_deck(&mut self) {
        while self.state.cards_left() > 0 {
            assert!(!self.state.is_over(), "the scripted game ended early");
            if self.state.current == Seat(0) {
                self.bob_clues();
            } else {
                let hand = self.state.hand(Seat(1));
                self.step(Action::Discard(hand[0]));
            }
        }
    }
}

#[test]
fn positional_discard_points_at_the_same_slot() {
    // Bob never draws: his r1 stays on slot 2, unclued.
    // Alice discards the three g1s first, so green is dead, and ends with the five other
    // green cards: she can see every card and her whole hand is trash.
    let mut table = Table::new(
        "g2 r1 g3 b2 b3",
        "r3 r2 g1 g1 g1",
        "r1 r1 r2 r3 r4 r4 r5 b1 b1 b1 b2 b3 b4 b4 b5 g2 g3 g4 g4 g5",
    );
    table.run_out_the_deck();
    let r1 = table.slot(0, 2);
    if table.state.current == Seat(0) {
        table.bob_clues();
    }
    assert!(
        !table.bots[0].view().note(r1).play,
        "Bob does not know his r1 yet"
    );
    // Alice can see every card: she discards slot 2 instead of her chop to tell Bob.
    let state = table.state;
    let chosen = table.bots[1].choose_action(&state);
    let view = table.bots[1].view();
    let target = table.slot(1, 2);
    assert_eq!(
        view.positional_signal(),
        Some((target, false)),
        "positional discard on slot 2\n{view:?}"
    );
    assert_eq!(chosen, Action::Discard(target), "{view:?}");
    table.step(chosen);
    let bob = table.bots[0].view();
    assert!(bob.note(r1).play, "Bob blind-plays slot 2\n{bob:?}");
    let state = table.state;
    assert_eq!(table.bots[0].choose_action(&state), Action::Play(r1));
}

#[test]
fn positional_misplay_when_the_discard_would_be_expected() {
    // Bob's r1 sits on slot 1, and slot 1 is the card Alice would discard anyway (all her
    // cards are known trash), so she misplays it instead.
    let mut table = Table::new(
        "r1 g2 g3 b2 b3",
        "r3 r2 g1 g1 g1",
        "r1 r1 r2 r3 r4 r4 r5 b1 b1 b1 b2 b3 b4 b4 b5 g2 g3 g4 g4 g5",
    );
    table.run_out_the_deck();
    let r1 = table.slot(0, 1);
    if table.state.current == Seat(0) {
        table.bob_clues();
    }
    let state = table.state;
    let chosen = table.bots[1].choose_action(&state);
    let target = table.slot(1, 1);
    assert_eq!(chosen, Action::Play(target), "{:?}", table.bots[1].view());
    table.step(chosen);
    assert_eq!(table.state.strikes, 1);
    let bob = table.bots[0].view();
    assert!(bob.note(r1).play, "Bob blind-plays slot 1\n{bob:?}");
}

#[test]
fn burning_waits_for_a_nearly_empty_deck() {
    // Alice only discards, so pace drops below the number of players with most of the deck
    // left (as in hard variants): no burn until fewer than 1.5 × players cards remain.
    let mut table = Table::new(
        "g2 r1 g3 b2 b3",
        "r3 r2 g1 g1 g1",
        "r1 r1 r2 r3 r4 r4 r5 b1 b1 b1 b2 b3 b4 b4 b5 g2 g3 g4 g4 g5",
    );
    while table.state.cards_left() > 0 {
        if table.state.current == Seat(0) {
            let bob = table.bots[0].view();
            let pace_burn = bob.in_end_game() && bob.pace() <= 0;
            assert_eq!(
                bob.burn_allowed(),
                pace_burn && table.state.cards_left() < 3,
                "deck {}, pace {}",
                table.state.cards_left(),
                bob.pace()
            );
            table.bob_clues();
        } else {
            let hand = table.state.hand(Seat(1));
            table.step(Action::Discard(hand[0]));
        }
    }
}
