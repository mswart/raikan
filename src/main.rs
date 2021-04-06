use std::env;

// mod dump_strategies;
mod card_quantum;
mod game;
mod hyphenated;
mod position_set;

fn main() {
    let args: Vec<String> = env::args().collect();
    let stats = args.contains(&"stats".to_string());

    let mut alice = hyphenated::HyphenatedPlayer::new(!stats);
    let mut bob = hyphenated::HyphenatedPlayer::new(!stats);
    let mut carl = hyphenated::HyphenatedPlayer::new(!stats);
    let mut daniel = hyphenated::HyphenatedPlayer::new(!stats);
    let mut players: Vec<&mut dyn game::PlayerStrategy> = Vec::new();
    players.push(&mut alice);
    players.push(&mut bob);
    players.push(&mut carl);
    players.push(&mut daniel);
    if stats {
        run_stats(&mut players);
    } else {
        let mut game = game::Game::new(&mut players, true);
        game.run(&mut players);
    }
}

fn run_stats(players: &mut Vec<&mut dyn game::PlayerStrategy>) {
    let mut lost_games = 0;
    let mut lost_scores: usize = 0;
    let mut lost_max_scores: usize = 0;
    let mut finished_games = 0;
    let mut finished_scores: usize = 0;
    let mut finished_max_scores: usize = 0;
    let mut won_games = 0;

    let total = 100_000;

    print!("0/{} games simulated", total);

    for i in 0..total {
        let mut game = game::Game::new(players, false);
        game.run(players);
        if i % 1_000 == 0 {
            print!("\r{}/{} games simulated", i, total);
        }
        // println!(
        //     "Game gained {}/{} due to {:?}",
        //     game.score, game.max_score, game.state
        // );
        match game.state {
            game::GameState::Lost() => {
                lost_games += 1;
                lost_scores += game.score as usize;
                lost_max_scores += game.max_score as usize;
            }
            game::GameState::Finished() => {
                finished_games += 1;
                finished_scores += game.score as usize;
                finished_max_scores += game.max_score as usize;
            }
            game::GameState::Won() => {
                won_games += 1;
                finished_scores += game.score as usize;
                finished_max_scores += game.max_score as usize;
            }
            _ => unimplemented!("Should not happen as final game score"),
        }
    }
    println!("\r{}/{} games simulated", total, total);

    println!(
        "Lost {:.2}% ({}) games with ~{:.2}/{:.2} scores",
        (lost_games * 100) as f64 / total as f64,
        lost_games,
        lost_scores as f64 / lost_games as f64,
        lost_max_scores as f64 / lost_games as f64,
    );

    println!(
        "Finished {} games with ~{:.2}/{:.2} scores",
        finished_games,
        finished_scores as f64 / (finished_games + won_games) as f64,
        finished_max_scores as f64 / (finished_games + won_games) as f64
    );
    println!("Won {} games", won_games);
    println!(
        "Overall {} games with ~{:.2} score",
        lost_games + finished_games + won_games,
        finished_scores as f64 / (lost_games + finished_games + won_games) as f64
    );
}
