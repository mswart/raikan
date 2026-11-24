//! `raikan` command line: self-play, statistics and regression diffs.
//!
//! ```text
//! raikan selfplay [SEED] [--players N] [--variant NAME] [--level L] [--trace FILE.jsonl]
//! raikan stats [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L]
//! raikan support                    # H-Group levels 1-20: required conventions and coverage
//! raikan diff OLD.list NEW.list [--level L]
//! raikan fit-value [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L]
//!                  [--out data/value-weights.json]
//!     Fit the search's leaf value weights against blueprint self-play played to the end.
//! raikan explain REPLAY [--turn T] [--candidate ACTION]... [--all] [--world W]
//!                [--card CARD [--viewer SEAT]] [--json] [--level L]
//!     REPLAY: a hanab.live replay JSON file, a replay URL (or a file holding one), a
//!     hanab.live game id or replay URL (downloaded; `#T` gives the turn), or
//!     `seed:N` for a self-play game with the given options. ACTION: `play 2`,
//!     `discard 4`, `clue Bob red` (slots count from the newest card). CARD: `#17` or
//!     `Bob:2`.
//!
//! search options (every command that plays):
//!   --search off|fast|full  --simulations N  --worlds W  --margin M  --min-gain G
//!   --to-end-pace P  --search-seed S  --spread P  --threads N  --time-limit-ms MS
//! ```

use std::io::{self, BufRead};
use std::thread;

use raikan::agent::{Agent, BotAgent, Config, GameResult, replay_prefix, run_game};
use raikan::domain::Seat;
use raikan::engine;
use raikan::engine::{EndReason, GameReference, Replay, ReplayAction, shuffled_world};
use raikan::search::{SearchConfig, value};
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
    out: Option<String>,
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
        out: None,
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
    let (mut spread, mut threads, mut time_limit) = (None, None, None);
    let (mut min_gain, mut to_end_pace) = (None, None);
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
            "--out" => options.out = Some(value()),
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
            "--min-gain" => {
                min_gain = Some(
                    value()
                        .parse()
                        .unwrap_or_else(|_| usage("invalid min gain")),
                )
            }
            "--to-end-pace" => {
                to_end_pace = Some(value().parse().unwrap_or_else(|_| usage("invalid pace")))
            }
            "--spread" => {
                spread = Some(value().parse().unwrap_or_else(|_| usage("invalid spread")))
            }
            "--threads" => {
                threads = Some(
                    value()
                        .parse()
                        .unwrap_or_else(|_| usage("invalid thread count")),
                )
            }
            "--time-limit-ms" => {
                time_limit = Some(std::time::Duration::from_millis(
                    value()
                        .parse()
                        .unwrap_or_else(|_| usage("invalid time limit")),
                ))
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
    if let Some(spread) = spread {
        options.search.partner_spread = spread;
    }
    if let Some(min_gain) = min_gain {
        options.search.min_gain = min_gain;
    }
    if let Some(to_end_pace) = to_end_pace {
        options.search.to_end_pace = to_end_pace;
    }
    if let Some(threads) = threads {
        options.search.threads = threads;
    }
    options.search.time_limit = time_limit;
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
        "  raikan fit-value [--players N] [--games G] [--seed-start S] [--variant NAME] [--level L] [--out FILE]"
    );
    eprintln!(
        "  raikan explain REPLAY [--turn T] [--candidate ACTION]... [--all] [--world W] [--card CARD [--viewer SEAT]] [--json]"
    );
    eprintln!(
        "  search: --search off|fast|full [--simulations N] [--worlds W] [--margin M] [--min-gain G] [--to-end-pace P] [--search-seed S] [--spread P] [--threads N] [--time-limit-ms MS]"
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
        "fit-value" => fit_value(&parse_options(rest)),
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
    search_overrides: usize,
    decisions: usize,
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
        self.search_overrides += other.search_overrides;
        self.decisions += other.decisions;
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
                    stats.search_overrides += result.search_overrides as usize;
                    stats.decisions += result.decisions as usize;
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
    if options.search.enabled() {
        eprintln!(
            "Search overrode the blueprint in {} of {} decisions ({:.1}%)",
            totals.search_overrides,
            totals.decisions,
            totals.search_overrides as f64 * 100.0 / totals.decisions.max(1) as f64
        );
    }
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

/// A replay from a JSON file, a URL (or a file holding one), a hanab.live game id or replay
/// URL (downloaded; with the turn of its `#T` fragment), or `seed:N` (self-play).
fn load_replay(options: &Options, spec: &str) -> (Replay, Option<usize>) {
    if let Some(seed) = spec.strip_prefix("seed:") {
        let seed = seed.parse().unwrap_or_else(|_| usage("invalid seed"));
        return (play(options, &variant(options), seed).replay, None);
    }
    if !std::path::Path::new(spec).exists()
        && let Some(game) = GameReference::parse(spec)
    {
        let json = download(&game.export_url())
            .unwrap_or_else(|e| usage(&format!("cannot download game {}: {e}", game.id)));
        let replay = Replay::from_json(&json)
            .unwrap_or_else(|e| usage(&format!("cannot parse game {}: {e}", game.id)));
        return (replay, game.turn);
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
    let replay = parsed.unwrap_or_else(|e| usage(&format!("cannot parse replay {spec}: {e}")));
    (replay, None)
}

#[cfg(feature = "fetch")]
fn download(url: &str) -> Result<String, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .and_then(|client| client.get(url).send())
        .and_then(|response| response.error_for_status())
        .map_err(|e| e.to_string())?;
    response.text().map_err(|e| e.to_string())
}

#[cfg(not(feature = "fetch"))]
fn download(url: &str) -> Result<String, String> {
    Err(format!(
        "built without the `fetch` feature; save {url} to a file and pass that instead"
    ))
}

fn explain(options: &Options) {
    let [spec] = &options.positional[..] else {
        usage("explain needs one replay");
    };
    let (replay, url_turn) = load_replay(options, spec);
    let turn = options
        .turn
        .or(url_turn)
        .unwrap_or_else(|| usage("explain needs --turn T (or a replay URL ending in #T)"));
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

/// Rows of one blueprint self-play game for the leaf value fit: the features of every
/// position the search would cut off (not in the end-game, where rollouts play to the
/// end), with the points the team still scored from there (a strikeout scores 0).
fn value_rows(
    options: &Options,
    variant: &Variant,
    seed: u64,
) -> Vec<([f64; value::NUM_FEATURES], f64)> {
    let world = shuffled_world(variant, seed);
    let blueprint = Config {
        search: SearchConfig::default(),
        ..config(options)
    };
    let mut bots: Vec<BotAgent> = (0..options.players)
        .map(|_| BotAgent::new(blueprint.clone()))
        .collect();
    for (s, bot) in bots.iter_mut().enumerate() {
        bot.start(variant, options.players, Seat(s as u8));
    }
    let (mut state, deal_events) = engine::deal(variant, options.players, &world);
    let feed = |bots: &mut [BotAgent], event: &engine::Event| {
        for (s, bot) in bots.iter_mut().enumerate() {
            bot.observe(&event.observed_by(Seat(s as u8)));
        }
    };
    for event in &deal_events {
        feed(&mut bots, event);
    }
    let cut_off = SearchConfig::new(raikan::search::SearchMode::Full);
    let mut positions = Vec::new();
    while !state.is_over() {
        if !raikan::search::plays_to_end(bots[0].view(), &cut_off) {
            let views: Vec<_> = bots.iter().map(|b| b.view().clone()).collect();
            let features = value::features(variant, &state, &world, &views);
            positions.push((features.vector(), state.score(variant) as f64));
        }
        let seat = state.current;
        let action = bots[seat.index()].choose_action(&state);
        for event in engine::step(variant, &mut state, &world, action) {
            feed(&mut bots, &event);
        }
    }
    let final_score = match state.status {
        engine::Status::Over(EndReason::Normal) => state.score(variant) as f64,
        _ => 0.0,
    };
    positions
        .into_iter()
        .map(|(features, score)| (features, final_score - score))
        .collect()
}

fn fit_value(options: &Options) {
    let variant = variant(options);
    let games = if options.games == 100_000 {
        5000
    } else {
        options.games
    };
    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as u64;
    type Row = ([f64; value::NUM_FEATURES], f64);
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let options = options.clone();
            let variant = variant.clone();
            thread::spawn(move || {
                let (mut train, mut test): (Vec<Row>, Vec<Row>) = (Vec::new(), Vec::new());
                for i in (0..games).filter(|i| i % threads == t) {
                    let rows = value_rows(&options, &variant, options.seed_start + i);
                    // Every fifth game is held out to judge the fit.
                    if i % 5 == 4 {
                        test.extend(rows);
                    } else {
                        train.extend(rows);
                    }
                }
                (train, test)
            })
        })
        .collect();
    let (mut train, mut test): (Vec<Row>, Vec<Row>) = (Vec::new(), Vec::new());
    for handle in handles {
        let (a, b) = handle.join().expect("worker thread panicked");
        train.extend(a);
        test.extend(b);
    }
    let fitted = value::Weights::from_vector(&value::fit(&train));
    // The hand-set weights estimate no constant; give them the best one for a fair test.
    let hand = {
        let mut w = value::Weights::HAND_SET;
        let mean_residual =
            train.iter().map(|(x, y)| y - predict(&w, x)).sum::<f64>() / train.len().max(1) as f64;
        w.constant = mean_residual;
        w
    };
    let mean = test.iter().map(|(_, y)| y).sum::<f64>() / test.len().max(1) as f64;
    let variance =
        test.iter().map(|(_, y)| (y - mean).powi(2)).sum::<f64>() / test.len().max(1) as f64;
    let mse = |w: &value::Weights| {
        test.iter()
            .map(|(x, y)| (y - predict(w, x)).powi(2))
            .sum::<f64>()
            / test.len().max(1) as f64
    };
    eprintln!(
        "{games} games, {} players, {}, level {}: {} training and {} held-out positions",
        options.players,
        variant.name,
        options.level,
        train.len(),
        test.len()
    );
    eprintln!(
        "held-out points still to score: mean {mean:.2}, sd {:.2}",
        variance.sqrt()
    );
    eprintln!(
        "held-out RMSE: fitted {:.3} (R² {:.3}), hand-set {:.3} (R² {:.3})",
        mse(&fitted).sqrt(),
        1.0 - mse(&fitted) / variance,
        mse(&hand).sqrt(),
        1.0 - mse(&hand) / variance
    );
    eprintln!("{:<16} {:>8} {:>8}", "feature", "fitted", "hand-set");
    for ((name, f), h) in value::FEATURE_NAMES
        .iter()
        .zip(fitted.vector())
        .zip(value::Weights::HAND_SET.vector())
    {
        eprintln!("{name:<16} {f:>8.3} {h:>8.3}");
    }
    let json = serde_json::to_string_pretty(&fitted).expect("weights serialise");
    match &options.out {
        Some(path) => {
            let mut file: value::WeightsFile = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            file.players.insert(options.players.to_string(), fitted);
            let text = serde_json::to_string_pretty(&file).expect("weights file serialises");
            std::fs::write(path, text + "\n")
                .unwrap_or_else(|e| usage(&format!("cannot write {path}: {e}")));
            eprintln!("wrote the {}-player weights to {path}", options.players);
        }
        None => println!("{json}"),
    }
}

fn predict(w: &value::Weights, x: &[f64; value::NUM_FEATURES]) -> f64 {
    w.vector().iter().zip(x).map(|(a, b)| a * b).sum()
}
