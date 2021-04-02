mod dump_strategies;
mod game;
mod hyphenated;

fn main() {
    let players: Vec<&dyn game::PlayerStrategy> = vec![
        &hyphenated::HyphenatedPlayer {},
        &hyphenated::HyphenatedPlayer {},
        &hyphenated::HyphenatedPlayer {},
    ];
    let mut game = game::Game::new(players);
    game.run(true);
}
