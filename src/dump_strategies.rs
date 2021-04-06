use crate::game;

use rand::thread_rng;
use rand::{seq::SliceRandom, Rng};

pub struct DiscardPlayer;

impl std::fmt::Debug for DiscardPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Discard")
    }
}

impl game::PlayerStrategy for DiscardPlayer {
    fn init(&mut self, _game: &game::Game) {}
    fn act(&mut self, game: &game::Game) -> game::Move {
        game::Move::Discard(game.num_hand_cards(0) - 1)
    }

    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {}

    fn played(
        &mut self,
        _player: usize,
        _pos: usize,
        _card: game::Card,
        _successful: bool,
        _blind: bool,
    ) {
    }

    fn discarded(&mut self, _player: usize, _pos: usize, _card: game::Card) {}
    fn clued(
        &mut self,
        _who: usize,
        _whom: usize,
        _clue: game::Clue,
        _touched: game::PositionSet,
        _previously_clued: game::PositionSet,
        _game: &game::Game,
    ) {
    }
}

pub struct PlayPlayer;

impl std::fmt::Debug for PlayPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("play")
    }
}

impl game::PlayerStrategy for PlayPlayer {
    fn init(&mut self, _game: &game::Game) {}

    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {}
    fn played(
        &mut self,
        _player: usize,
        _pos: usize,
        _card: game::Card,
        _successful: bool,
        _blind: bool,
    ) {
    }

    fn discarded(&mut self, _player: usize, _pos: usize, _card: game::Card) {}
    fn clued(
        &mut self,
        _who: usize,
        _whom: usize,
        _clue: game::Clue,
        _touched: game::PositionSet,
        _previously_clued: game::PositionSet,
        _game: &game::Game,
    ) {
    }

    fn act(&mut self, game: &game::Game) -> game::Move {
        game::Move::Play(game.num_hand_cards(0) - 1)
    }
}

pub struct RandCluePlayer;

impl std::fmt::Debug for RandCluePlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("rand_clue")
    }
}

impl game::PlayerStrategy for RandCluePlayer {
    fn init(&mut self, _game: &game::Game) {}
    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {}

    fn played(
        &mut self,
        _player: usize,
        _pos: usize,
        _card: game::Card,
        _successful: bool,
        _blind: bool,
    ) {
    }

    fn discarded(&mut self, _player: usize, _pos: usize, _card: game::Card) {}
    fn clued(
        &mut self,
        _who: usize,
        _whom: usize,
        _clue: game::Clue,
        _touched: game::PositionSet,
        _previously_clued: game::PositionSet,
        _game: &game::Game,
    ) {
    }

    fn act(&mut self, _game: &game::Game) -> game::Move {
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
