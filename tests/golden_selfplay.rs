//! Golden corpus: the complete replays of seeded self-play games, folded into one checksum
//! per player count. They were recorded when the new bot reproduced the legacy bot
//! (identical replays for 2-4 players; see docs/parity.md), so any change of a decision
//! shows up here. After an intended behaviour change, update the checksums with
//! `RAIKAN_GOLDEN_PRINT=1 cargo test --release --test golden_selfplay -- --nocapture`.

use raikan::agent::{Agent, BotAgent, run_game};
use raikan::engine::shuffled_world;
use raikan::variants::Variant;

const GAMES: u64 = 20;

/// (players, FNV-1a checksum of the replay URLs of seeds 0..GAMES)
const EXPECTED: &[(u8, u64)] = &[
    (2, 0x810671752c95aa3e),
    (3, 0x6be2e0922f04aa3a),
    (4, 0x988cba98f5713313),
    (5, 0x626e725596fcca12),
];

fn checksum(players: u8) -> u64 {
    let variant = Variant::no_variant();
    let mut checksum = 0xcbf29ce484222325u64;
    for seed in 0..GAMES {
        let mut bots: Vec<BotAgent> = (0..players).map(|_| BotAgent::default()).collect();
        let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
        let result = run_game(&variant, &shuffled_world(&variant, seed), &mut refs);
        for byte in result.replay.to_url().bytes() {
            checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    checksum
}

#[test]
fn selfplay_replays_are_unchanged() {
    let actual: Vec<(u8, u64)> = EXPECTED
        .iter()
        .map(|&(players, _)| (players, checksum(players)))
        .collect();
    if std::env::var_os("RAIKAN_GOLDEN_PRINT").is_some() {
        for (players, sum) in &actual {
            println!("    ({players}, 0x{sum:016x}),");
        }
    }
    assert_eq!(actual, EXPECTED);
}
