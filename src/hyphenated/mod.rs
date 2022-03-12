mod card_states;
mod line;
mod slot;

use crate::card_quantum::Variant;
use crate::game;

pub use line::Line;
pub use line::LineScore;

impl std::fmt::Debug for HyphenatedPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        for (pos, slot) in self.line.hands[0].iter().enumerate() {
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
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            variant: Variant {},
            turn: 0,
            line: line::Line::new(0, 0),
        }
    }

    pub fn line(&self) -> line::Line {
        self.line.clone()
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, num_players: u8, own_player: u8) {
        self.variant = Variant {};
        self.turn = 0;
        self.line = line::Line::new(num_players, own_player);
    }

    fn drawn(&mut self, player: usize, card: game::Card) {
        self.line.drawn(player, card);
    }

    fn own_drawn(&mut self) {
        self.line.own_drawn();
    }

    fn played(
        &mut self,
        player: usize,
        pos: usize,
        card: game::Card,
        successful: bool,
        blind: bool,
    ) {
        self.line.played(player, pos, card, successful, blind);
        self.turn += 1;
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.line.discarded(player, pos, card);
        self.turn += 1;
    }

    fn clued(
        &mut self,
        who: usize,
        whom: usize,
        clue: game::Clue,
        touched: game::PositionSet,
        previously_clued: game::PositionSet,
    ) {
        self.line.clued(who, whom, clue, touched, previously_clued);
        self.turn += 1;
    }

    fn act(&mut self, status: &game::GameStatus) -> game::Move {
        if let Some(play_move) = self.line.play() {
            return play_move;
        }
        if status.clues == 0 {
            return self.line.discard();
        }
        // compare clues:
        let mut best_score = if status.clues == 8 {
            LineScore::bad()
        } else {
            self.line.score(0)
        };
        let mut best_move = self.line.discard();
        if self.debug {
            println!("discarding score: {:?}", best_score);
        }
        for player in 1..self.line.hands.len() as u8 {
            for suit in self.variant.suits().iter() {
                let clue = game::Clue::Color(suit.clue_color());
                if let Some(score) = self.line.clone().clue(player as usize, clue) {
                    if self.debug {
                        println!("considered cluing {:?} to {player} with {:?}", clue, score);
                    }
                    if score > best_score {
                        best_move = game::Move::Clue(player, clue);
                        best_score = score;
                    }
                }
            }
            for rank in 1..=5 {
                let clue = game::Clue::Rank(rank);
                if let Some(score) = self.line.clone().clue(player as usize, clue) {
                    if self.debug {
                        println!("considered cluingg {:?} to {player} with {:?}", clue, score);
                    }
                    if score > best_score {
                        best_move = game::Move::Clue(player, clue);
                        best_score = score;
                    }
                }
            }
        }
        return best_move;
    }
}
