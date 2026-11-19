//! Golden corpus: the complete replays of seeded self-play games, folded into one checksum
//! per player count. They were recorded when the new bot reproduced the legacy bot
//! (identical replays for 2-4 players; see docs/parity.md), so any change of a decision
//! shows up here. After an intended behaviour change, update the checksums with
//! `RAIKAN_GOLDEN_PRINT=1 cargo test --release --test golden_selfplay -- --nocapture`.

use raikan::agent::{Agent, BotAgent, run_game};
use raikan::engine::shuffled_world;
use raikan::variants::Variant;

const GAMES: u64 = 20;

/// (variant, players, FNV-1a checksum of the replay URLs of seeds 0..GAMES)
const EXPECTED: &[(&str, u8, u64)] = &[
    ("No Variant", 2, 0x810671752c95aa3e),
    ("No Variant", 3, 0x6be2e0922f04aa3a),
    ("No Variant", 4, 0x988cba98f5713313),
    ("No Variant", 5, 0x626e725596fcca12),
    ("Rainbow (6 Suits)", 3, 0x033491b95415cbfc),
    ("Rainbow (6 Suits)", 4, 0x31d702e236310a61),
];

fn checksum(variant: &str, players: u8) -> u64 {
    let variant = Variant::by_name(variant).unwrap();
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
    let actual: Vec<(&str, u8, u64)> = EXPECTED
        .iter()
        .map(|&(variant, players, _)| (variant, players, checksum(variant, players)))
        .collect();
    if std::env::var_os("RAIKAN_GOLDEN_PRINT").is_some() {
        for (variant, players, sum) in &actual {
            println!("    ({variant:?}, {players}, 0x{sum:016x}),");
        }
    }
    assert_eq!(actual, EXPECTED);
}
