//! While the legacy bot exists it is the reference: on a corpus of seeds, every seat's notes
//! (beliefs, play/trash/clued/delay/promise flags) and every decision of the new bot must
//! equal the legacy bot's at every event. `RAIKAN_EQUIV_GAMES` sets the seeds per player
//! count (default 15).

use raikan::adapters::legacy::LegacyAgent;
use raikan::agent::{Agent, BotAgent};
use raikan::domain::Seat;
use raikan::engine::{deal, shuffled_world, step};
use raikan::variants::Variant;

/// Seeds whose games diverge for a documented reason (see the commit introducing the
/// conventions layer): (players, seed).
const KNOWN_DIVERGENCES: &[(u8, u64)] = &[];

fn lockstep(players: u8, seed: u64) -> Result<(), String> {
    let variant = Variant::no_variant();
    let world = shuffled_world(&variant, seed);
    let mut legacy: Vec<LegacyAgent> = (0..players).map(|_| LegacyAgent::hyphenated()).collect();
    let mut new: Vec<BotAgent> = (0..players).map(|_| BotAgent::default()).collect();
    for s in 0..players {
        legacy[s as usize].start(&variant, players, Seat(s));
        new[s as usize].start(&variant, players, Seat(s));
    }
    let (mut state, events) = deal(&variant, players, &world);
    let mut pending = events;
    loop {
        for event in &pending {
            for s in 0..players as usize {
                let seen = event.observed_by(Seat(s as u8));
                legacy[s].observe(&seen);
                new[s].observe(&seen);
                if let Some(difference) = legacy[s].compare_notes(new[s].view()) {
                    return Err(format!(
                        "turn {}, view of seat {s}: {difference}",
                        state.turn
                    ));
                }
            }
        }
        if state.is_over() {
            return Ok(());
        }
        let seat = state.current.index();
        let expected = legacy[seat].choose_action(&state);
        let actual = new[seat].choose_action(&state);
        if expected != actual {
            return Err(format!(
                "turn {} seat {seat}: legacy {expected:?}, new {actual:?}",
                state.turn
            ));
        }
        pending = step(&variant, &mut state, &world, expected).to_vec();
    }
}

#[test]
fn notes_and_decisions_match_the_legacy_bot() {
    let games: u64 = std::env::var("RAIKAN_EQUIV_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    let mut failures = Vec::new();
    // 5 players are excluded: the legacy bot's card stamps collide there (documented bug).
    for players in 2..=4u8 {
        for seed in 0..games {
            if KNOWN_DIVERGENCES.contains(&(players, seed)) {
                continue;
            }
            if let Err(error) = lockstep(players, seed) {
                failures.push(format!("{players} players, seed {seed}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
