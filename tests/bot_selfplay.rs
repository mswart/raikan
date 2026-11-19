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
