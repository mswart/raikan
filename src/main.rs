use rand::prelude::*;
use std::io::prelude::*;
use std::{env, io, ops::AddAssign, thread};

use hanabi::*;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.contains(&"stats".to_string()) {
        run_stats();
    } else if args.contains(&"debug_reg".to_string()) {
        debug_regressions(&args[2], &args[3])?;
    } else if args.contains(&"webclient".to_string()) {
        webclient::run();
    } else {
        let mut alice = hyphenated::HyphenatedPlayer::new(true);
        let mut bob = hyphenated::HyphenatedPlayer::new(true);
        let mut carl = hyphenated::HyphenatedPlayer::new(true);
        let mut daniel = hyphenated::HyphenatedPlayer::new(true);
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
    Ok(())
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
    finished_dist: [usize; 26],
    finished_max_dist: [usize; 26],
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
            finished_dist: [0; 26],
            finished_max_dist: [0; 26],
            won_games: 0,
        }
    }

    fn median(&self) -> (f64, f64) {
        let num_median = self.finished_games / 2;
        let mut num_seen = 0;
        let mut max_seen = 0;
        let mut score_median = 0.0;
        let mut max_median = 0.0;
        for i in 0..=25 {
            if num_median > num_seen + self.finished_dist[i] {
                num_seen += self.finished_dist[i];
            } else if score_median == 0.0 {
                score_median =
                    i as f64 + ((num_median - num_seen) as f64 / self.finished_dist[i] as f64);
            }
            if num_median > max_seen + self.finished_max_dist[i] {
                max_seen += self.finished_max_dist[i];
            } else if max_median == 0.0 {
                max_median =
                    i as f64 + ((num_median - max_seen) as f64 / self.finished_max_dist[i] as f64);
            }
        }

        (score_median, max_median)
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
        for i in 0..=25 {
            self.finished_dist[i] += other.finished_dist[i];
            self.finished_max_dist[i] += other.finished_max_dist[i];
        }
    }
}

fn run_stats() {
    let mut totals = Stats::new();

    let total = 100_000;

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
                match game.state {
                    game::GameState::Lost() => {
                        results.lost_games += 1;
                        results.lost_scores += game.status.score as usize;
                        results.lost_max_scores += game.status.max_score as usize;
                        println!("{i} Lost 0 0 {} {}", game.status.turn, game.replay_url());
                    }
                    game::GameState::Finished() => {
                        results.finished_games += 1;
                        results.finished_scores += game.status.score as usize;
                        results.finished_dist[game.status.score as usize] += 1;
                        results.finished_score_intergrals += game.score_integral as usize;
                        results.finished_max_scores += game.status.max_score as usize;
                        results.finished_max_dist[game.status.max_score as usize] += 1;
                        println!(
                            "{i} Finished {} {} {} {}",
                            game.status.score,
                            game.status.max_score,
                            game.status.turn,
                            game.replay_url()
                        );
                    }
                    game::GameState::Won() => {
                        results.won_games += 1;
                        results.finished_scores += game.status.score as usize;
                        results.finished_max_scores += game.status.max_score as usize;
                        println!("{i} Won 25 25 {} {}", game.status.turn, game.replay_url());
                    }
                    game::GameState::Invalid() => {
                        results.invalid_games += 1;
                        results.invalid_scores += game.status.score as usize;
                        results.invalid_max_scores += game.status.max_score as usize;
                        println!("{i} Invalid 0 0 {} {}", game.status.turn, game.replay_url());
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

    eprintln!("\r{}/{} games simulated", total, total);

    eprintln!(
        "Invalid {:.2}% ({}) games with ~{:.2}/{:.2} scores",
        (totals.invalid_games * 100) as f64 / total as f64,
        totals.invalid_games,
        totals.invalid_scores as f64 / totals.invalid_games as f64,
        totals.invalid_max_scores as f64 / totals.invalid_games as f64,
    );

    eprintln!(
        "Lost {:.2}% ({}) games with ~{:.2}/{:.2} scores",
        (totals.lost_games * 100) as f64 / total as f64,
        totals.lost_games,
        totals.lost_scores as f64 / totals.lost_games as f64,
        totals.lost_max_scores as f64 / totals.lost_games as f64,
    );

    let median = totals.median();
    eprintln!(
        "Finished {} games with ~{:.2}/{:.2} scores (~{:.2} integral) => \n dist: {:?}\n  max: {:?}\n => {:.2} / {:.2}",
        totals.finished_games,
        totals.finished_scores as f64 / (totals.finished_games + totals.won_games) as f64,
        totals.finished_max_scores as f64 / (totals.finished_games + totals.won_games) as f64,
        totals.finished_score_intergrals as f64 / totals.finished_games as f64,
        totals.finished_dist,
        totals.finished_max_dist,
        median.0,
        median.1,
    );
    eprintln!(
        "Won {:.2}% {} games",
        (totals.won_games * 100) as f64 / total as f64,
        totals.won_games
    );
    eprintln!(
        "Overall {} games with ~{:.2} score",
        totals.invalid_games + totals.lost_games + totals.finished_games + totals.won_games,
        totals.finished_scores as f64
            / (totals.invalid_games + totals.lost_games + totals.finished_games + totals.won_games)
                as f64
    );
}

fn debug_regressions(old: &String, new: &String) -> io::Result<()> {
    println!("old: {old}");
    println!("new: {new}");
    let old_file = std::fs::File::open(old)?;
    let new_file = std::fs::File::open(new)?;
    let old_reader = io::BufReader::new(old_file);
    let new_reader = io::BufReader::new(new_file);

    for (old_line, new_line) in std::iter::zip(old_reader.lines(), new_reader.lines()) {
        let old_line = old_line?;
        let new_line = new_line?;
        if old_line == new_line {
            continue;
        }
        let old_parts: Vec<&str> = old_line.split(" ").collect();
        let new_parts: Vec<&str> = new_line.split(" ").collect();
        if new_parts[1] != "Finished" {
            continue;
        }
        if old_parts[2].parse::<u8>().unwrap() > new_parts[2].parse::<u8>().unwrap() {
            println!(
                "{} {} ({}/{}) vs {} ({}/{}) => {}",
                old_parts[0],
                old_parts[1],
                old_parts[2],
                old_parts[3],
                new_parts[1],
                new_parts[2],
                new_parts[3],
                new_parts[5],
            );
            let mut old_replay = old_parts[5][31..].split(",");
            let mut new_replay = new_parts[5][31..].split(",");
            let old_deck = old_replay.nth(0).unwrap();
            let new_deck = new_replay.nth(0).unwrap();
            let old_actions = old_replay.nth(0).unwrap();
            let new_actions = new_replay.nth(0).unwrap();
            let old_options = old_replay.nth(0).unwrap();
            let new_options = new_replay.nth(0).unwrap();
            assert_eq!(old_deck, new_deck);
            assert_ne!(old_actions, new_actions);
            let num_players = new_deck
                .chars()
                .nth(0)
                .expect("deck should be non-empty")
                .to_digit(10)
                .expect("player count must be a number") as u8;
            assert_eq!(num_players, 4);
            let mut old_h1 = hyphenated::HyphenatedPlayer::new(false);
            let mut old_h2 = hyphenated::HyphenatedPlayer::new(false);
            let mut old_h3 = hyphenated::HyphenatedPlayer::new(false);
            let mut old_h4 = hyphenated::HyphenatedPlayer::new(false);
            let mut old_players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
            old_players.push(&mut old_h1);
            old_players.push(&mut old_h2);
            old_players.push(&mut old_h3);
            old_players.push(&mut old_h4);
            let mut new_h1 = hyphenated::HyphenatedPlayer::new(false);
            let mut new_h2 = hyphenated::HyphenatedPlayer::new(false);
            let mut new_h3 = hyphenated::HyphenatedPlayer::new(false);
            let mut new_h4 = hyphenated::HyphenatedPlayer::new(false);
            let mut new_players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
            new_players.push(&mut new_h1);
            new_players.push(&mut new_h2);
            new_players.push(&mut new_h3);
            new_players.push(&mut new_h4);

            let mut unchanged = 0;
            for (old_action, new_action) in std::iter::zip(old_actions.chars(), new_actions.chars())
            {
                if old_action == new_action {
                    unchanged += 1;
                } else {
                    break;
                }
            }
            println!("unchanged: {unchanged}");

            let turn = unchanged / 2;

            game::Game::from_replay(turn, old_deck, old_actions, old_options, &mut old_players);
            let target_player = (turn - 1) % num_players;
            println!("target player: {target_player}");

            println!("Old replay: {}", old_parts[5]);
            let old_line = match target_player {
                0 => old_h1.line(),
                1 => old_h2.line(),
                2 => old_h3.line(),
                _ => old_h4.line(),
            };
            println!("Old line {:?}", old_line);

            println!("New replay: {}", new_parts[5]);
            game::Game::from_replay(turn, new_deck, new_actions, new_options, &mut new_players);
            let new_line = match target_player {
                0 => new_h1.line(),
                1 => new_h2.line(),
                2 => new_h3.line(),
                _ => new_h4.line(),
            };
            println!("new line {:?}", new_line);
            break;
        }
    }
    Ok(())
}
