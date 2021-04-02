use crate::game;

use rand::thread_rng;
use rand::{seq::SliceRandom, Rng};

pub struct DiscardPlayer;

impl game::PlayerStrategy for DiscardPlayer {
    fn act(&self, game: &game::Game) -> game::Move {
        game::Move::Discard(game.num_hand_cards(0) - 1)
    }
}

pub struct PlayPlayer;

impl game::PlayerStrategy for PlayPlayer {
    fn act(&self, game: &game::Game) -> game::Move {
        game::Move::Play(game.num_hand_cards(0) - 1)
    }
}

pub struct RandCluePlayer;

impl game::PlayerStrategy for RandCluePlayer {
    fn act(&self, _game: &game::Game) -> game::Move {
        let mut rng = thread_rng();
        if rng.gen_bool(0.3) {
            game::Move::Clue(0, game::Clue::Rank(rng.gen_range(1..=5)))
        } else {
            let colors = [
                game::ClueColor::Blue(),
                game::ClueColor::Green(),
                game::ClueColor::Yellow(),
                game::ClueColor::Red(),
                game::ClueColor::Purple(),
            ];
            game::Move::Clue(
                0,
                game::Clue::Color(*colors.choose(&mut rng).expect("asdf")),
            )
        }
    }
}
