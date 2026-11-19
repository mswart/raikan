//! Deterministic end-to-end benchmark, with optional allocation counting.
//! cargo bench --bench simulation -- 100 4 [--allocations] [--bot new|legacy|old]
//!
//! `new` is the H-Group bot of the new architecture, `legacy` the old bot on the new
//! engine, `old` the old bot on the old engine (the historical checksum).

use raikan::{
    adapters::legacy::LegacyAgent,
    agent::{Agent, BotAgent, run_game},
    engine::shuffled_world,
    game::{Game, PlayerStrategy},
    hyphenated::HyphenatedPlayer,
    variants::Variant,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

struct CountingAllocator;
static COUNT: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

fn count(size: usize) {
    if COUNT.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
    }
}

// All allocations are delegated unchanged to the system allocator. Counters
// include reallocations and requested bytes, not peak live memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let mut args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--bench")
        .collect();
    let bot = match args.iter().position(|arg| arg == "--bot") {
        Some(index) => {
            let bot = args.get(index + 1).expect("--bot needs a value").clone();
            args.drain(index..=index + 1);
            bot
        }
        None => "old".to_string(),
    };
    assert!(
        ["new", "legacy", "old"].contains(&bot.as_str()),
        "unknown bot {bot}"
    );
    let games = args
        .first()
        .map_or(100, |s| s.parse::<u64>().expect("game count"));
    let seats = args
        .get(1)
        .map_or(4, |s| s.parse::<usize>().expect("player count"));
    assert!(games > 0 && (2..=5).contains(&seats));
    let allocations = args.iter().any(|arg| arg == "--allocations");
    let mut bots: Vec<_> = (0..seats).map(|_| HyphenatedPlayer::new(false)).collect();
    let mut players: Vec<&mut dyn PlayerStrategy> = bots
        .iter_mut()
        .map(|bot| bot as &mut dyn PlayerStrategy)
        .collect();
    let variant = Variant::no_variant();
    let mut checksum = 0xcbf29ce484222325u64;
    let mut turns = 0u64;
    COUNT.store(allocations, Ordering::Relaxed);
    let start = Instant::now();
    for seed in 0..games {
        // The complete replay, not just the score, detects policy changes.
        let url = if bot == "old" {
            let mut game = Game::new(&mut players, false, seed);
            game.run(&mut players);
            turns += u64::from(game.status.turn);
            game.replay_url()
        } else {
            let mut agents: Vec<Box<dyn Agent>> = (0..seats)
                .map(|_| -> Box<dyn Agent> {
                    if bot == "new" {
                        Box::new(BotAgent::default())
                    } else {
                        Box::new(LegacyAgent::hyphenated())
                    }
                })
                .collect();
            let mut refs: Vec<&mut dyn Agent> = agents
                .iter_mut()
                .map(|a| &mut **a as &mut dyn Agent)
                .collect();
            let result = run_game(&variant, &shuffled_world(&variant, seed), &mut refs);
            turns += u64::from(result.state.turn);
            result.replay.to_url()
        };
        for byte in url.bytes() {
            checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    let elapsed = start.elapsed();
    COUNT.store(false, Ordering::Relaxed);
    println!(
        "bot={bot} games={games} players={seats} turns={turns} elapsed={:.6}s games/s={:.2} checksum={checksum:016x}",
        elapsed.as_secs_f64(),
        games as f64 / elapsed.as_secs_f64()
    );
    if allocations {
        println!(
            "allocations={} requested_bytes={}",
            ALLOCATIONS.load(Ordering::Relaxed),
            BYTES.load(Ordering::Relaxed)
        );
    }
}
