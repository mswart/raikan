mod card_states;
mod fundamentals;
pub mod interpretation;
mod line;
mod search;
mod slot;
mod stall;

use crate::card_quantum::Variant;
use crate::game;

use slog;

pub use line::Line;
pub use line::LineScore;
pub use slot::{Slot, SlotId};

impl std::fmt::Debug for HyphenatedPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        for (pos, slot) in self.line.hands.iter_hand(0) {
            if pos > 0 {
                f.write_str(", ")?;
            }
            std::fmt::Display::fmt(&slot.quantum, f)?;
            if slot.clued {
                f.write_str("'")?;
            }
            if slot.trash {
                f.write_str("kt")?;
            }
            if slot.play {
                f.write_str("!")?;
            }
        }
        f.write_str("]")?;
        Ok(())
    }
}

pub struct HyphenatedPlayer {
    debug: bool,
    variant: Variant,
    turn: u8,
    line: line::Line,
    logger: slog::Logger,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self::_new(debug, slog::Logger::root(slog::Discard, slog::o!()))
    }

    fn _new(debug: bool, logger: slog::Logger) -> Self {
        Self {
            debug,
            variant: Variant {},
            turn: 0,
            line: line::Line::new(4, 0),
            logger,
        }
    }

    pub fn with_logger(logger: slog::Logger) -> Self {
        Self::_new(false, logger)
    }

    pub fn line(&self) -> line::Line {
        self.line.clone()
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, num_players: u8, own_player: u8) {
        self.variant = Variant {};
        self.turn = 0;
        self.line = line::Line::with_logger(
            num_players,
            own_player,
            self.logger.new(slog::o!("turn" => "0")),
        );
    }

    fn drawn(&mut self, player: usize, card: game::Card) {
        self.line.drawn(player, card);
    }

    fn own_drawn(&mut self) {
        self.line.own_drawn();
    }

    fn played(&mut self, player: usize, pos: usize, card: game::Card, successful: bool) {
        self.line.played(player, pos, card, successful);
        self.turn += 1;
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.line.discarded(player, pos, card);
        self.turn += 1;
    }

    fn clued(&mut self, who: usize, whom: usize, clue: game::Clue, touched: game::PositionSet) {
        self.line.clued(who, whom, clue, touched);
        self.turn += 1;
    }

    fn act(&mut self, status: &game::GameStatus) -> game::Move {
        self.line.context.clues = status.clues;
        search::choose_action(&self.line, status, self.debug)
    }
}
