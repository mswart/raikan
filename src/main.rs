use std::{env, ops::AddAssign, thread};

use hanabi::*;
use rand::prelude::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    let stats = args.contains(&"stats".to_string());

    if stats {
        run_stats();
    } else {
        let mut alice = hyphenated::HyphenatedPlayer::new(!stats);
        let mut bob = hyphenated::HyphenatedPlayer::new(!stats);
        let mut carl = hyphenated::HyphenatedPlayer::new(!stats);
        let mut daniel = hyphenated::HyphenatedPlayer::new(!stats);
        let mut players: Vec<&mut dyn game::PlayerStrategy> = Vec::new();
        players.push(&mut alice);
        players.push(&mut bob);
        players.push(&mut carl);
        players.push(&mut daniel);
        let seed;
        println!("args: {:?}", args);
        if args.len() > 1 {
            seed = args[1].parse().expect("Invalid seed format");
        } else {
            let mut seed_rng = rand::thread_rng();
            seed = seed_rng.gen();
        }
        let mut game = game::Game::new(&mut players, true, seed);
        game.run(&mut players);
        println!("Used seed {}", seed);
        game.print_replay();
    }
}

struct Stats {
    invalid_games: usize,
    invalid_scores: usize,
    invalid_max_scores: usize,
    lost_games: usize,
    lost_scores: usize,
    lost_max_scores: usize,
    finished_games: usize,
    finished_scores: usize,
    finished_score_intergrals: usize,
    finished_max_scores: usize,
    won_games: usize,
}

impl Stats {
    fn new() -> Self {
        Self {
            invalid_games: 0,
            invalid_scores: 0,
            invalid_max_scores: 0,
            lost_games: 0,
            lost_scores: 0,
            lost_max_scores: 0,
            finished_games: 0,
            finished_scores: 0,
            finished_score_intergrals: 0,
            finished_max_scores: 0,
            won_games: 0,
        }
    }
}

impl AddAssign for Stats {
    fn add_assign(&mut self, other: Self) {
        self.invalid_games += other.invalid_games;
        self.invalid_scores += other.invalid_scores;
        self.invalid_max_scores += other.invalid_max_scores;
        self.lost_games += other.lost_games;
        self.lost_scores += other.lost_scores;
        self.lost_max_scores += other.lost_max_scores;
        self.finished_games += other.finished_games;
        self.finished_scores += other.finished_scores;
        self.finished_score_intergrals += other.finished_score_intergrals;
        self.finished_max_scores += other.finished_max_scores;
        self.won_games += other.won_games;
    }
}

fn run_stats() {
    let mut totals = Stats::new();

    let total = 100_000;

    print!("0/{} games simulated", total);

    let thread_count = if let Ok(num) = thread::available_parallelism() {
        num.get()
    } else {
        1
    };

    let mut threads = Vec::new();

    for t in 0..thread_count {
        let thread = thread::spawn(move || {
            let mut results = Stats::new();
            let mut alice = hyphenated::HyphenatedPlayer::new(false);
            let mut bob = hyphenated::HyphenatedPlayer::new(false);
            let mut carl = hyphenated::HyphenatedPlayer::new(false);
            let mut daniel = hyphenated::HyphenatedPlayer::new(false);
            let mut players: Vec<&mut dyn game::PlayerStrategy> = Vec::new();
            players.push(&mut alice);
            players.push(&mut bob);
            players.push(&mut carl);
            players.push(&mut daniel);
            for i in 0..total {
                if i % thread_count != t {
                    continue;
                }
                let mut game = game::Game::new(&mut players, false, i as u64);
                game.run(&mut players);
                if i % 1_000 == 0 {
                    print!("\r{}/{} games simulated", i, total);
                }
                match game.state {
                    game::GameState::Lost() => {
                        results.lost_games += 1;
                        results.lost_scores += game.score as usize;
                        results.lost_max_scores += game.max_score as usize;
                    }
                    game::GameState::Finished() => {
                        results.finished_games += 1;
                        results.finished_scores += game.score as usize;
                        results.finished_score_intergrals += game.score_integral as usize;
                        results.finished_max_scores += game.max_score as usize;
                    }
                    game::GameState::Won() => {
                        results.won_games += 1;
                        results.finished_scores += game.score as usize;
                        results.finished_max_scores += game.max_score as usize;
                    }
                    game::GameState::Invalid() => {
                        results.invalid_games += 1;
                        results.invalid_scores += game.score as usize;
                        results.invalid_max_scores += game.max_score as usize;
                    }
                    _ => unimplemented!("Should not happen as final game score"),
                }
            }
            results
        });
        threads.push(thread);
    }
    for thread in threads.into_iter() {
        let result = thread.join().unwrap();
        totals += result;
    }

    println!("\r{}/{} games simulated", total, total);

    println!(
        "Invalid {:.2}% ({}) games with ~{:.2}/{:.2} scores",
        (totals.invalid_games * 100) as f64 / total as f64,
        totals.invalid_games,
        totals.invalid_scores as f64 / totals.invalid_games as f64,
        totals.invalid_max_scores as f64 / totals.invalid_games as f64,
    );

    println!(
        "Lost {:.2}% ({}) games with ~{:.2}/{:.2} scores",
        (totals.lost_games * 100) as f64 / total as f64,
        totals.lost_games,
        totals.lost_scores as f64 / totals.lost_games as f64,
        totals.lost_max_scores as f64 / totals.lost_games as f64,
    );

    println!(
        "Finished {} games with ~{:.2}/{:.2} scores (~{:.2} integral)",
        totals.finished_games,
        totals.finished_scores as f64 / (totals.finished_games + totals.won_games) as f64,
        totals.finished_max_scores as f64 / (totals.finished_games + totals.won_games) as f64,
        totals.finished_score_intergrals as f64 / totals.finished_games as f64,
    );
    println!(
        "Won {:.2}% {} games",
        (totals.won_games * 100) as f64 / total as f64,
        totals.won_games
    );
    println!(
        "Overall {} games with ~{:.2} score",
        totals.invalid_games + totals.lost_games + totals.finished_games + totals.won_games,
        totals.finished_scores as f64
            / (totals.invalid_games + totals.lost_games + totals.finished_games + totals.won_games)
                as f64
    );
}
