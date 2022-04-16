use crate::game;

use rand::thread_rng;
use rand::{seq::SliceRandom, Rng};

pub struct DiscardPlayer {
    num_hand_cards: u8,
}

impl std::fmt::Debug for DiscardPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Discard")
    }
}

impl game::PlayerStrategy for DiscardPlayer {
    fn init(&mut self, _num_players: u8, _own_index: u8) {}
    fn act(&mut self, _status: &game::GameStatus) -> game::Move {
        game::Move::Discard(self.num_hand_cards - 1)
    }

    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {
        self.num_hand_cards -= 1;
    }

    fn played(&mut self, player: usize, _pos: usize, _card: game::Card, _successful: bool) {
        if player == 0 {
            self.num_hand_cards -= 1;
        }
    }

    fn discarded(&mut self, player: usize, _pos: usize, _card: game::Card) {
        if player == 0 {
            self.num_hand_cards -= 1;
        }
    }
    fn clued(&mut self, _who: usize, _whom: usize, _clue: game::Clue, _touched: game::PositionSet) {
    }
}

pub struct PlayPlayer {
    num_hand_cards: u8,
}

impl std::fmt::Debug for PlayPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("play")
    }
}

impl game::PlayerStrategy for PlayPlayer {
    fn init(&mut self, _num_players: u8, _own_index: u8) {}

    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {
        self.num_hand_cards += 1;
    }
    fn played(&mut self, player: usize, _pos: usize, _card: game::Card, _successful: bool) {
        if player == 0 {
            self.num_hand_cards -= 1;
        }
    }

    fn discarded(&mut self, player: usize, _pos: usize, _card: game::Card) {
        if player == 0 {
            self.num_hand_cards -= 1;
        }
    }
    fn clued(&mut self, _who: usize, _whom: usize, _clue: game::Clue, _touched: game::PositionSet) {
    }

    fn act(&mut self, _status: &game::GameStatus) -> game::Move {
        game::Move::Play(self.num_hand_cards - 1)
    }
}

pub struct RandCluePlayer;

impl std::fmt::Debug for RandCluePlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("rand_clue")
    }
}

impl game::PlayerStrategy for RandCluePlayer {
    fn init(&mut self, _num_players: u8, _own_index: u8) {}
    fn drawn(&mut self, _player: usize, _card: game::Card) {}
    fn own_drawn(&mut self) {}

    fn played(&mut self, _player: usize, _pos: usize, _card: game::Card, _successful: bool) {}

    fn discarded(&mut self, _player: usize, _pos: usize, _card: game::Card) {}
    fn clued(&mut self, _who: usize, _whom: usize, _clue: game::Clue, _touched: game::PositionSet) {
    }

    fn act(&mut self, _status: &game::GameStatus) -> game::Move {
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
