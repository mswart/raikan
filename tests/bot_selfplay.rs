//! Self-play smoke test for the new bot: games finish without illegal moves.

use raikan::agent::{Agent, BotAgent, run_game};
use raikan::engine::{EndReason, ReplayAction, shuffled_world};
use raikan::variants::Variant;

fn play(variant: &Variant, seats: usize, seed: u64) -> (u8, Option<EndReason>) {
    let world = shuffled_world(variant, seed);
    let mut agents: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
    let mut refs: Vec<&mut dyn Agent> = agents.iter_mut().map(|a| a as &mut dyn Agent).collect();
    let result = run_game(variant, &world, &mut refs);
    let end = result.replay.actions.iter().find_map(|a| match a {
        ReplayAction::GameOver { reason, .. } => Some(*reason),
        _ => None,
    });
    (result.score, end)
}

#[test]
fn games_finish_legally() {
    let variant = Variant::no_variant();
    let mut total = 0u32;
    let mut games = 0;
    for seats in 2..=5 {
        for seed in 0..5 {
            let (score, end) = play(&variant, seats, seed);
            assert_ne!(end, Some(EndReason::Illegal), "{seats} seats, seed {seed}");
            total += score as u32;
            games += 1;
        }
    }
    assert!(
        total as f64 / games as f64 > 10.0,
        "average score {}",
        total as f64 / games as f64
    );
}

#[test]
fn six_suits_smoke() {
    let variant = Variant::by_name("6 Suits").unwrap();
    for seed in 0..2 {
        let (_, end) = play(&variant, 3, seed);
        assert_ne!(end, Some(EndReason::Illegal));
    }
}

/// A view never knows its own cards, and a seat's first decision does not change when its
/// hidden cards (own hand and the deck) are swapped (docs/architecture.md §10).
#[test]
fn decisions_do_not_depend_on_hidden_cards() {
    use raikan::domain::Seat;
    use raikan::engine::{World, deal};

    let variant = Variant::no_variant();
    for seed in 0..40u64 {
        let seats = 3 + (seed % 3) as u8;
        let world = shuffled_world(&variant, seed);
        let hand = raikan::engine::hand_size(seats) as usize;
        // swap seat 0's starting hand with deck cards that are drawn later
        let mut swapped: Vec<_> = world.as_slice().to_vec();
        let deck_start = hand * seats as usize;
        for i in 0..hand {
            swapped.swap(i, deck_start + i);
        }
        let other = World::new(&swapped);
        let mut decisions = Vec::new();
        for world in [&world, &other] {
            let mut bots: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
            for (s, bot) in bots.iter_mut().enumerate() {
                bot.start(&variant, seats, Seat(s as u8));
            }
            let (state, events) = deal(&variant, seats, world);
            for event in &events {
                for (s, bot) in bots.iter_mut().enumerate() {
                    bot.observe(&event.observed_by(Seat(s as u8)));
                }
            }
            for (s, bot) in bots.iter().enumerate() {
                for &card in state.hand(Seat(s as u8)) {
                    assert_eq!(
                        bot.view().identity(card),
                        None,
                        "seat {s} knows its own card #{}",
                        card.0
                    );
                    assert_eq!(bot.view().known.get(card), None);
                }
            }
            decisions.push(bots[0].choose_action(&state));
        }
        assert_eq!(
            decisions[0], decisions[1],
            "seed {seed}: seat 0 decided on hidden cards"
        );
    }
}
