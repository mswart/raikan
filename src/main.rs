use std::env;

// mod dump_strategies;
mod game;
mod hyphenated;

fn main() {
    let args: Vec<String> = env::args().collect();
    let stats = args.contains(&"stats".to_string());

    let mut alice = hyphenated::HyphenatedPlayer::new(!stats);
    let mut bob = hyphenated::HyphenatedPlayer::new(!stats);
    let mut carl = hyphenated::HyphenatedPlayer::new(!stats);
    let mut players: Vec<&mut dyn game::PlayerStrategy> = Vec::new();
    players.push(&mut alice);
    players.push(&mut bob);
    players.push(&mut carl);
    if stats {
        run_stats(&mut players);
    } else {
        let mut game = game::Game::new(&mut players, true);
        game.run(&mut players);
    }
}

fn run_stats(players: &mut Vec<&mut dyn game::PlayerStrategy>) {
    let mut lost_games = 0;
    let mut finished_games = 0;
    let mut lost_scores: usize = 0;
    let mut finished_scores: usize = 0;

    for i in 0..1_000 {
        let mut game = game::Game::new(players, false);
        game.run(players);
        println!(
            "Game gained {}/{} due to {:?}",
            game.score, game.max_score, game.state
        );
        match game.state {
            game::GameState::Lost() => {
                lost_games += 1;
                lost_scores += game.score as usize;
            }
            game::GameState::Finished() => {
                finished_games += 1;
                finished_scores += game.score as usize;
            }
            _ => unimplemented!("Should not happen as final game score"),
        }
    }

    println!(
        "Lost {} games with ~{} scores",
        lost_games,
        lost_scores as f64 / lost_games as f64
    );

    println!(
        "Finished {} games with ~{} scores",
        finished_games,
        finished_scores as f64 / finished_games as f64
    );
    println!(
        "Overall {} games with ~{} score",
        lost_games + finished_games,
        finished_scores as f64 / (lost_games + finished_games) as f64
    );
}
