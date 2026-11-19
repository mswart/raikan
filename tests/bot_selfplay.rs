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

/// What we predict a teammate believes must not depend on that teammate's own hidden cards
/// (review finding: perspective() used to keep deductions from cards the teammate cannot
/// see). Swap one of seat 1's starting cards with a late deck card: while seat 1 observes
/// the same events in both worlds, seat 0's prediction of seat 1's view must be equal.
/// The prediction may use what seat 0 believes about its own cards (seat 1 sees them), so
/// steps where those beliefs differ between the two worlds are skipped.
#[test]
fn teammate_predictions_do_not_depend_on_their_hidden_cards() {
    use raikan::domain::{CardId, Seat};
    use raikan::engine::{Event, World, deal, step};

    /// What the prediction may use from seat 0's side: its beliefs about its own cards.
    fn own_beliefs(
        bot: &BotAgent,
        state: &raikan::engine::GameState,
    ) -> Vec<(CardId, raikan::conventions::Belief)> {
        state
            .hand(Seat(0))
            .iter()
            .map(|&c| (c, bot.view().note(c).belief))
            .collect()
    }

    let variant = Variant::no_variant();
    let mut compared = 0;
    for seed in 0..30u64 {
        let seats = 3 + (seed % 2) as u8;
        let world = shuffled_world(&variant, seed);
        let hand = raikan::engine::hand_size(seats) as usize;
        // seat 1's first dealt card and the last card of the deck, if they differ
        let a = hand;
        let b = world.len() - 1;
        if world.identity(CardId(a as u8)) == world.identity(CardId(b as u8)) {
            continue;
        }
        let mut swapped = world.as_slice().to_vec();
        swapped.swap(a, b);
        let other = World::new(&swapped);

        // actions of the game in the original world
        let mut players: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
        let mut refs: Vec<&mut dyn Agent> =
            players.iter_mut().map(|b| b as &mut dyn Agent).collect();
        let actions: Vec<_> = run_game(&variant, &world, &mut refs)
            .replay
            .game_actions()
            .collect();

        let mut tables: Vec<(raikan::engine::GameState, Vec<BotAgent>, &World)> = Vec::new();
        for w in [&world, &other] {
            let mut bots: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
            for (s, bot) in bots.iter_mut().enumerate() {
                bot.start(&variant, seats, Seat(s as u8));
            }
            let (state, events) = deal(&variant, seats, w);
            for event in &events {
                for (s, bot) in bots.iter_mut().enumerate() {
                    bot.observe(&event.observed_by(Seat(s as u8)));
                }
            }
            tables.push((state, bots, w));
        }
        for action in actions {
            let mut seen_by_one: Vec<Vec<Event>> = Vec::new();
            for (state, bots, w) in tables.iter_mut() {
                if raikan::engine::check_legal(&variant, state, action).is_err() {
                    seen_by_one.push(Vec::new());
                    continue;
                }
                let events = step(&variant, state, w, action);
                for event in &events {
                    for (s, bot) in bots.iter_mut().enumerate() {
                        bot.observe(&event.observed_by(Seat(s as u8)));
                    }
                }
                seen_by_one.push(events.iter().map(|e| e.observed_by(Seat(1))).collect());
            }
            if seen_by_one[0].is_empty() || seen_by_one[0] != seen_by_one[1] {
                break; // seat 1's observations differ from here on
            }
            let (state_a, bots_a, _) = &tables[0];
            let (state_b, bots_b, _) = &tables[1];
            if own_beliefs(&bots_a[0], state_a) != own_beliefs(&bots_b[0], state_b) {
                continue;
            }
            let pa = bots_a[0].view().perspective(Seat(1));
            let pb = bots_b[0].view().perspective(Seat(1));
            for card in state_a.dealt().iter() {
                assert_eq!(
                    pa.note(card),
                    pb.note(card),
                    "seed {seed}, turn {}: card #{}",
                    state_a.turn,
                    card.0
                );
            }
            assert_eq!(
                pa.interpretations, pb.interpretations,
                "seed {seed}, turn {}",
                state_a.turn
            );
            compared += 1;
        }
    }
    assert!(compared > 100, "only {compared} positions compared");
}
