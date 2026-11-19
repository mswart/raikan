//! The new engine must reproduce the old one: the legacy bot played through
//! `agent::run_game` produces the same deck and the same actions as `game::Game::run`.
//!
//! `RAIKAN_EQUIV_GAMES` sets the seeds per player count (default 25; use 1000 with
//! `--release` for the full gate).

use raikan::adapters::legacy::LegacyAgent;
use raikan::agent::{Agent, run_game};
use raikan::engine::{EndReason, Replay, ReplayAction, shuffled_world};
use raikan::game::{self, PlayerStrategy};
use raikan::hyphenated::HyphenatedPlayer;
use raikan::variants::Variant;

fn old_game(seats: usize, seed: u64) -> (game::GameState, u8, Replay) {
    let mut bots: Vec<HyphenatedPlayer> =
        (0..seats).map(|_| HyphenatedPlayer::new(false)).collect();
    let mut players: Vec<&mut dyn PlayerStrategy> = bots
        .iter_mut()
        .map(|b| b as &mut dyn PlayerStrategy)
        .collect();
    let mut game = game::Game::new(&mut players, false, seed);
    game.run(&mut players);
    let replay = Replay::from_url(&game.replay_url()).expect("old replay URL parses");
    (game.state, game.status.score, replay)
}

#[test]
fn legacy_bot_plays_identical_games_on_new_engine() {
    let games: u64 = std::env::var("RAIKAN_EQUIV_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let variant = Variant::no_variant();
    for seats in 2..=5usize {
        for seed in 0..games {
            let (old_state, old_score, old) = old_game(seats, seed);

            let world = shuffled_world(&variant, seed);
            let mut agents: Vec<LegacyAgent> =
                (0..seats).map(|_| LegacyAgent::hyphenated()).collect();
            let mut refs: Vec<&mut dyn Agent> =
                agents.iter_mut().map(|a| a as &mut dyn Agent).collect();
            let new = run_game(&variant, &world, &mut refs);

            let context = format!("{seats} seats, seed {seed}");
            // the old export only contains drawn cards (plus the rest on a loss)
            assert_eq!(
                old.deck.as_slice(),
                &new.replay.deck.as_slice()[..old.deck.len()],
                "deck differs: {context}"
            );
            let old_actions: Vec<_> = old.game_actions().collect();
            let new_actions: Vec<_> = new.replay.game_actions().collect();
            // Old-engine bug: a winning play that draws the last card turns `Won` back into
            // `Final(n)`, so the old game continues for extra turns after reaching 25.
            let old_actions =
                if new.score == variant.max_score() && old_actions.len() > new_actions.len() {
                    &old_actions[..new_actions.len()]
                } else {
                    &old_actions[..]
                };
            assert_eq!(old_actions, &new_actions[..], "actions differ: {context}");

            let new_end = new.replay.actions.iter().find_map(|a| match a {
                ReplayAction::GameOver { reason, .. } => Some(*reason),
                _ => None,
            });
            match old_state {
                game::GameState::Won() | game::GameState::Finished() => {
                    assert_eq!(new_end, Some(EndReason::Normal), "{context}");
                    assert_eq!(old_score, new.score, "{context}");
                }
                game::GameState::Lost() => {
                    assert_eq!(new_end, Some(EndReason::Strikeout), "{context}")
                }
                game::GameState::Invalid() => {
                    assert_eq!(new_end, Some(EndReason::Illegal), "{context}")
                }
                ref other => panic!("unexpected final state {other:?}: {context}"),
            }
        }
    }
}
