//! `raikan` command line: self-play, statistics and regression diffs.
//!
//! ```text
//! raikan selfplay [SEED] [--players N] [--variant NAME] [--level L] [--trace FILE.jsonl]
//! raikan stats [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L]
//! raikan support                    # H-Group levels 1-20: required conventions and coverage
//! raikan diff OLD.list NEW.list [--level L]
//! raikan explain REPLAY --turn T [--candidate ACTION]... [--all] [--world W]
//!                [--card CARD [--viewer SEAT]] [--json] [--level L]
//!     REPLAY: a hanab.live replay JSON file, a replay URL (or a file holding one), or
//!     `seed:N` for a self-play game with the given options. ACTION: `play 2`,
//!     `discard 4`, `clue Bob red` (slots count from the newest card). CARD: `#17` or
//!     `Bob:2`.
//!
//! search options (every command that plays):
//!   --search off|fast|full  --simulations N  --worlds W  --margin M  --search-seed S
//! ```

use std::io::{self, BufRead};
use std::thread;

use raikan::agent::{Agent, BotAgent, Config, GameResult, replay_prefix, run_game};
use raikan::engine::{EndReason, Replay, ReplayAction, shuffled_world};
use raikan::search::SearchConfig;
use raikan::variants::Variant;

#[derive(Clone, Debug)]
struct Options {
    players: u8,
    games: u64,
    seed_start: u64,
    variant: String,
    level: u8,
    search: SearchConfig,
    trace: Option<String>,
    positional: Vec<String>,
    // explain
    turn: Option<usize>,
    candidates: Vec<String>,
    all: bool,
    world: Option<usize>,
    card: Option<String>,
    viewer: Option<String>,
    json: bool,
}

fn parse_options(args: &[String]) -> Options {
    let mut options = Options {
        players: 4,
        games: 100_000,
        seed_start: 0,
        variant: "No Variant".to_string(),
        level: Config::default().max_level,
        search: SearchConfig::default(),
        trace: None,
        positional: Vec::new(),
        turn: None,
        candidates: Vec::new(),
        all: false,
        world: None,
        card: None,
        viewer: None,
        json: false,
    };
    let (mut simulations, mut worlds, mut margin) = (None, None, None);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .unwrap_or_else(|| usage(&format!("{arg} needs a value")))
                .clone()
        };
        match arg.as_str() {
            "--players" => {
                options.players = value()
                    .parse()
                    .unwrap_or_else(|_| usage("invalid player count"))
            }
            "--games" => {
                options.games = value()
                    .parse()
                    .unwrap_or_else(|_| usage("invalid game count"))
            }
            "--seed-start" => {
                options.seed_start = value().parse().unwrap_or_else(|_| usage("invalid seed"))
            }
            "--variant" => options.variant = value(),
            "--trace" => options.trace = Some(value()),
            "--level" => options.level = value().parse().unwrap_or_else(|_| usage("invalid level")),
            "--search" => {
                options.search.mode = value().parse().unwrap_or_else(|e: String| usage(&e))
            }
            "--simulations" => {
                simulations = Some(
                    value()
                        .parse()
                        .unwrap_or_else(|_| usage("invalid simulation count")),
                )
            }
            "--worlds" => {
                worlds = Some(
                    value()
                        .parse()
                        .unwrap_or_else(|_| usage("invalid world count")),
                )
            }
            "--margin" => {
                margin = Some(value().parse().unwrap_or_else(|_| usage("invalid margin")))
            }
            "--search-seed" => {
                options.search.seed =
                    Some(value().parse().unwrap_or_else(|_| usage("invalid seed")))
            }
            "--turn" => {
                options.turn = Some(value().parse().unwrap_or_else(|_| usage("invalid turn")))
            }
            "--candidate" => options.candidates.push(value()),
            "--all" => options.all = true,
            "--world" => {
                options.world = Some(value().parse().unwrap_or_else(|_| usage("invalid world")))
            }
            "--card" => options.card = Some(value()),
            "--viewer" => options.viewer = Some(value()),
            "--json" => options.json = true,
            other if other.starts_with("--") => usage(&format!("unknown option {other}")),
            other => options.positional.push(other.to_string()),
        }
    }
    // Budgets override the defaults of the chosen mode, whatever the option order.
    let seed = options.search.seed;
    options.search = SearchConfig::new(options.search.mode);
    options.search.seed = seed;
    if let Some(simulations) = simulations {
        options.search.simulations = simulations;
    }
    if let Some(worlds) = worlds {
        options.search.worlds = worlds;
    }
    if let Some(margin) = margin {
        options.search.blueprint_margin = margin;
    }
    if !(2..=6).contains(&options.players) {
        usage("players must be 2..=6");
    }
    options
}

fn usage(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!("usage:");
    eprintln!("  raikan selfplay [SEED] [--players N] [--variant NAME] [--level L]");
    eprintln!(
        "  raikan stats [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L]"
    );
    eprintln!("  raikan diff OLD.list NEW.list [--level L]");
    eprintln!(
        "  raikan explain REPLAY --turn T [--candidate ACTION]... [--all] [--world W] [--card CARD [--viewer SEAT]] [--json]"
    );
    eprintln!(
        "  search: --search off|fast|full [--simulations N] [--worlds W] [--margin M] [--search-seed S]"
    );
    std::process::exit(2)
}

fn variant(options: &Options) -> Variant {
    Variant::by_name(&options.variant).unwrap_or_else(|err| usage(&err.to_string()))
}

fn config(options: &Options) -> Config {
    Config {
        max_level: options.level,
        search: options.search.clone(),
        ..Config::default()
    }
}

fn make_agents(options: &Options, seats: u8) -> Vec<Box<dyn Agent>> {
    (0..seats)
        .map(|_| Box::new(BotAgent::new(config(options))) as Box<dyn Agent>)
        .collect()
}

fn play(options: &Options, variant: &Variant, seed: u64) -> GameResult {
    let world = shuffled_world(variant, seed);
    let mut agents = make_agents(options, options.players);
    let mut refs: Vec<&mut dyn Agent> = agents
        .iter_mut()
        .map(|a| &mut **a as &mut dyn Agent)
        .collect();
    let result = run_game(variant, &world, &mut refs);
    drop(refs);
    result
}

fn end_reason(result: &GameResult) -> EndReason {
    result
        .replay
        .actions
        .iter()
        .find_map(|a| match a {
            ReplayAction::GameOver { reason, .. } => Some(*reason),
            _ => None,
        })
        .unwrap_or(EndReason::Illegal)
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("selfplay", &args[..]),
    };
    match command {
        "stats" => run_stats(&parse_options(rest)),
        "selfplay" => selfplay(&parse_options(rest)),
        "diff" | "debug_reg" => {
            let options = parse_options(rest);
            if options.positional.len() != 2 {
                usage("diff needs two list files");
            }
            diff(&options, &options.positional[0], &options.positional[1])?;
        }
        "explain" => explain(&parse_options(rest)),
        "support" => print!("{}", raikan::conventions::catalogue::support_matrix()),
        "-h" | "--help" | "help" => usage("help requested"),
        // `raikan SEED` keeps working as a single self-play game
        other if other.parse::<u64>().is_ok() => selfplay(&parse_options(&args)),
        other => usage(&format!("unknown command {other}")),
    }
    Ok(())
}

fn selfplay(options: &Options) {
    let seed = match options.positional.first() {
        Some(seed) => seed.parse().unwrap_or_else(|_| usage("invalid seed")),
        None => rand::random(),
    };
    let variant = variant(options);
    let result = if let Some(path) = &options.trace {
        let world = shuffled_world(&variant, seed);
        let mut bots: Vec<BotAgent> = (0..options.players)
            .map(|_| {
                let mut bot = BotAgent::new(config(options));
                bot.enable_trace();
                bot
            })
            .collect();
        let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
        let result = run_game(&variant, &world, &mut refs);
        drop(refs);
        let mut lines = Vec::new();
        for bot in &mut bots {
            lines.extend(bot.take_trace());
        }
        std::fs::write(path, lines.join("\n") + "\n")
            .unwrap_or_else(|e| usage(&format!("cannot write {path}: {e}")));
        result
    } else {
        play(options, &variant, seed)
    };
    println!(
        "seed {seed}: score {} ({:?})",
        result.score,
        end_reason(&result)
    );
    println!("replay JSON: {}", result.replay.to_json());
    println!("Replay url: {}", result.replay.to_url());
}

#[derive(Default, Clone)]
struct Stats {
    games: usize,
    invalid: usize,
    lost: usize,
    finished: usize,
    won: usize,
    score_sum: usize,
    score_sq_sum: f64,
    dist: Vec<usize>,
    blind_plays: usize,
    strikes: usize,
}

impl Stats {
    fn add(&mut self, other: &Stats) {
        self.games += other.games;
        self.invalid += other.invalid;
        self.lost += other.lost;
        self.finished += other.finished;
        self.won += other.won;
        self.score_sum += other.score_sum;
        self.score_sq_sum += other.score_sq_sum;
        if self.dist.len() < other.dist.len() {
            self.dist.resize(other.dist.len(), 0);
        }
        for (i, n) in other.dist.iter().enumerate() {
            self.dist[i] += n;
        }
        self.blind_plays += other.blind_plays;
        self.strikes += other.strikes;
    }
}

fn run_stats(options: &Options) {
    let variant = variant(options);
    let max = variant.max_score();
    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as u64;
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let options = options.clone();
            let variant = variant.clone();
            thread::spawn(move || {
                let mut stats = Stats {
                    dist: vec![0; max as usize + 1],
                    ..Stats::default()
                };
                let mut lines = Vec::new();
                for i in (0..options.games).filter(|i| i % threads == t) {
                    let seed = options.seed_start + i;
                    let result = play(&options, &variant, seed);
                    let state = &result.state;
                    stats.games += 1;
                    stats.blind_plays += result.blind_plays as usize;
                    stats.strikes += state.strikes as usize;
                    let reachable = variant.max_reachable_score(&state.discarded);
                    let (label, score, shown_max) = match end_reason(&result) {
                        EndReason::Illegal => {
                            stats.invalid += 1;
                            ("Invalid", 0, 0)
                        }
                        EndReason::Strikeout => {
                            stats.lost += 1;
                            ("Lost", 0, 0)
                        }
                        EndReason::Normal if result.score == max => {
                            stats.won += 1;
                            ("Won", result.score, max)
                        }
                        EndReason::Normal => {
                            stats.finished += 1;
                            ("Finished", result.score, reachable)
                        }
                    };
                    stats.score_sum += score as usize;
                    stats.score_sq_sum += (score as f64).powi(2);
                    stats.dist[score as usize] += 1;
                    lines.push((
                        i,
                        format!(
                            "{seed} {label} {score} {shown_max} {} {} {}",
                            state.turn,
                            result.blind_plays,
                            result.replay.to_url()
                        ),
                    ));
                }
                (stats, lines)
            })
        })
        .collect();
    let mut totals = Stats {
        dist: vec![0; max as usize + 1],
        ..Stats::default()
    };
    let mut lines = Vec::new();
    for handle in handles {
        let (stats, mut part) = handle.join().expect("worker thread panicked");
        totals.add(&stats);
        lines.append(&mut part);
    }
    lines.sort_by_key(|(i, _)| *i);
    for (_, line) in &lines {
        println!("{line}");
    }
    let n = totals.games.max(1) as f64;
    let mean = totals.score_sum as f64 / n;
    let stddev = (totals.score_sq_sum / n - mean * mean).max(0.0).sqrt();
    let pct = |count: usize| count as f64 * 100.0 / n;
    eprintln!(
        "{} games, {} players, {}, level {}",
        totals.games, options.players, variant.name, options.level
    );
    eprintln!("Invalid {:.2}% ({})", pct(totals.invalid), totals.invalid);
    eprintln!("Lost {:.2}% ({})", pct(totals.lost), totals.lost);
    eprintln!(
        "Finished {:.2}% ({})",
        pct(totals.finished),
        totals.finished
    );
    eprintln!("Won {:.2}% ({})", pct(totals.won), totals.won);
    eprintln!("score dist: {:?}", totals.dist);
    eprintln!(
        "Overall mean score {mean:.3} (sd {stddev:.2}, se {:.3}); {} blind plays, {} strikes",
        stddev / n.sqrt(),
        totals.blind_plays,
        totals.strikes
    );
}

fn diff(options: &Options, old: &str, new: &str) -> io::Result<()> {
    println!("old: {old}");
    println!("new: {new}");
    let old_reader = io::BufReader::new(std::fs::File::open(old)?);
    let new_reader = io::BufReader::new(std::fs::File::open(new)?);
    for (old_line, new_line) in std::iter::zip(old_reader.lines(), new_reader.lines()) {
        let (old_line, new_line) = (old_line?, new_line?);
        if old_line == new_line {
            continue;
        }
        let old_parts: Vec<&str> = old_line.split(' ').collect();
        let new_parts: Vec<&str> = new_line.split(' ').collect();
        let (Some(old_score), Some(new_score)) = (old_parts.get(2), new_parts.get(2)) else {
            continue;
        };
        if old_score.parse::<u8>().unwrap_or(0) <= new_score.parse::<u8>().unwrap_or(0) {
            continue;
        }
        println!(
            "seed {}: {} {} -> {} {}",
            old_parts[0], old_parts[1], old_score, new_parts[1], new_score
        );
        let old_replay = Replay::from_url(old_parts.last().unwrap()).map_err(io::Error::other)?;
        let new_replay = Replay::from_url(new_parts.last().unwrap()).map_err(io::Error::other)?;
        let old_actions: Vec<_> = old_replay.game_actions().collect();
        let new_actions: Vec<_> = new_replay.game_actions().collect();
        let turn = old_actions
            .iter()
            .zip(&new_actions)
            .take_while(|(a, b)| a == b)
            .count();
        let seat = (turn % new_replay.num_seats() as usize) as u8;
        println!(
            "first diverging turn {turn} (seat {seat}): old {:?}, new {:?}",
            old_actions.get(turn),
            new_actions.get(turn)
        );
        let mut agents: Vec<BotAgent> = (0..new_replay.num_seats())
            .map(|_| BotAgent::new(config(options)))
            .collect();
        let mut refs: Vec<&mut dyn Agent> =
            agents.iter_mut().map(|a| a as &mut dyn Agent).collect();
        replay_prefix(&new_replay, turn, &mut refs).map_err(io::Error::other)?;
        println!("{:?}", agents[seat as usize].view());
        println!("old replay: {}", old_parts.last().unwrap());
        println!("new replay: {}", new_parts.last().unwrap());
        break;
    }
    Ok(())
}

/// A replay from a JSON file, a URL (or a file holding one), or `seed:N` (self-play).
fn load_replay(options: &Options, spec: &str) -> Replay {
    if let Some(seed) = spec.strip_prefix("seed:") {
        let seed = seed.parse().unwrap_or_else(|_| usage("invalid seed"));
        return play(options, &variant(options), seed).replay;
    }
    let text = if spec.starts_with("http") {
        spec.to_string()
    } else {
        std::fs::read_to_string(spec).unwrap_or_else(|e| usage(&format!("cannot read {spec}: {e}")))
    };
    let text = text.trim();
    let parsed = if text.starts_with('{') {
        Replay::from_json(text)
    } else {
        Replay::from_url(text)
    };
    parsed.unwrap_or_else(|e| usage(&format!("cannot parse replay {spec}: {e}")))
}

fn explain(options: &Options) {
    let [spec] = &options.positional[..] else {
        usage("explain needs one replay");
    };
    let turn = options
        .turn
        .unwrap_or_else(|| usage("explain needs --turn T"));
    let replay = load_replay(options, spec);
    let mut request = raikan::adapters::explain::Request::new(turn);
    request.candidates = options.candidates.clone();
    request.all = options.all;
    request.world = options.world;
    request.card = options.card.clone();
    request.viewer = options.viewer.clone();
    let explanation = raikan::adapters::explain::explain(&replay, &config(options), &request)
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1)
        });
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&explanation).expect("explanation serialises")
        );
    } else {
        print!("{}", explanation.render());
    }
}
