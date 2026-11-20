//! Golden corpus: the complete replays of seeded self-play games, folded into one checksum
//! per player count. First recorded when the new bot reproduced the legacy bot
//! (identical replays for 2-4 players; see docs/parity.md); re-recorded after each
//! intended behaviour change (last: level 4 chop moves), so any
//! change of a decision shows up here. After an intended behaviour change, update the checksums with
//! `RAIKAN_GOLDEN_PRINT=1 cargo test --release --test golden_selfplay -- --nocapture`.

use raikan::agent::{Agent, BotAgent, run_game};
use raikan::engine::shuffled_world;
use raikan::variants::Variant;

const GAMES: u64 = 12;

/// (variant, players, FNV-1a checksum of the replay URLs of seeds 0..GAMES)
const EXPECTED: &[(&str, u8, u64)] = &[
    ("No Variant", 2, 0xf4c3b4b8b7d6ef5c),
    ("No Variant", 3, 0xe8ffc04104f6be63),
    ("No Variant", 4, 0x315b5ac6bec94bc5),
    ("No Variant", 5, 0x5d78e45ab5b84b93),
    ("Rainbow (6 Suits)", 3, 0x74c82ff3d4943407),
    ("Rainbow (6 Suits)", 4, 0x7bc13902982c5550),
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
