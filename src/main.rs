// mod dump_strategies;
mod game;
mod hyphenated;

fn main() {
    let mut alice = hyphenated::HyphenatedPlayer::default();
    let mut bob = hyphenated::HyphenatedPlayer::default();
    let mut carl = hyphenated::HyphenatedPlayer::default();
    let mut players: Vec<&mut dyn game::PlayerStrategy> = Vec::new();
    players.push(&mut alice);
    players.push(&mut bob);
    players.push(&mut carl);
    let mut game = game::Game::new(&mut players);
    game.run(&mut players, true);
}
