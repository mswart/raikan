use std::collections::VecDeque;

use hanabi::game;

pub struct InstructedPlayer {
    actions: VecDeque<game::Move>,
    default: Option<game::Move>,
}

impl InstructedPlayer {
    pub fn new() -> Self {
        Self {
            actions: VecDeque::new(),
            default: None,
        }
    }

    pub fn with_default(default: game::Move) -> Self {
        Self {
            actions: VecDeque::new(),
            default: Some(default),
        }
    }

    pub fn add(&mut self, action: game::Move) {
        self.actions.push_back(action);
    }
}

impl game::PlayerStrategy for InstructedPlayer {
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
        self.actions.pop_front().unwrap_or_else(|| {
            self.default
                .expect("Player should be given enough instructions or a default")
        })
    }
}
