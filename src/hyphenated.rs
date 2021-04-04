use std::collections::VecDeque;

use crate::game::{self, CardPlayState};

pub struct HyphenatedPlayer {
    chop: i8,
    play_queue: VecDeque<u8>,
    debug: bool,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            chop: 0,
            play_queue: VecDeque::with_capacity(5),
            debug: debug,
        }
    }

    fn chop(&self, player: usize, game: &game::Game) -> i8 {
        let mut chop: i8 = game.num_hand_cards(player) as i8 - 1;
        while chop >= 0 && game.card_cluded(chop as u8, player) {
            chop -= 1
        }
        chop
    }

    fn discard(&mut self, game: &game::Game) -> game::Move {
        let chop = self.chop(0, game);
        assert!(
            self.chop == chop,
            "cached chop = {}; actual chop = {}",
            self.chop,
            chop
        );
        if chop >= 0 {
            while self.chop > 0 && game.card_cluded((self.chop - 1) as u8, 0) {
                self.chop -= 1;
            }
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for pos in 0..game.num_hand_cards(0) {
                if game.card_must_rank(0, pos, *rank) {
                    if self.chop < pos as i8 {
                        self.chop += 1;
                    } else {
                        // chop might move up to 0 if we have clued cards to the right:
                        self.chop = pos as i8;
                        while self.chop > 0 && game.card_cluded((self.chop - 1) as u8, 0) {
                            self.chop -= 1;
                        }
                    }
                    return game::Move::Discard(pos);
                }
            }
        }
        self.chop = 0;
        // nothing clear found; drop newest card
        game::Move::Discard(0)
    }

    fn give_play_clue(&self, game: &game::Game) -> Option<game::Move> {
        if game.clues == 0 {
            return None;
        }
        for player in 1..game.num_players() as usize {
            for pos in 0..game.num_hand_cards(player) {
                let card = game.player_card(pos, player);
                let play_state = card.play_state(&game);
                if game.card_cluded(pos as u8, player) {
                    continue;
                }
                if let CardPlayState::Playable() = play_state {
                    // return game::Move::Clue(player as u8, game::Clue::Color(card.suite.clue_color()));
                    return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
                }
            }
        }
        None
    }

    fn give_save_clue(&self, game: &game::Game) -> Option<game::Move> {
        if game.clues == 0 {
            return None;
        }
        for player in 1..game.num_players() as usize {
            let mut chop = game.num_hand_cards(player) - 1;
            while game.card_cluded(chop, player) {
                if chop == 0 {
                    break;
                }
                chop -= 1
            }
            if game.card_cluded(chop, player) {
                break;
            }
            let card = game.player_card(chop, player);
            if let CardPlayState::Critical() = card.play_state(&game) {
                // return game::Move::Clue(player as u8, game::Clue::Color(card.suite.clue_color()));
                return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
            }
        }
        None
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, game: &game::Game) {
        self.chop = self.chop(0, game) as i8;
    }

    fn clued(&mut self, clue: game::Clue, touched: u8, previously_clued: u8, game: &game::Game) {
        let old_chop = self.chop;
        self.chop = 4;
        while self.chop >= 0 && (1 << self.chop) & touched > 0
            || (1 << self.chop) & previously_clued > 0
        {
            self.chop -= 1;
        }
        let potential_safe = (1 << old_chop) & touched > 0;
        // self.chop = self.chop(0, game) as i8;
        if self.debug {
            println!(
                "Got clued with {:?}; touched {:b}; previously clued {:b}; chop {}=>{} (potential safe {})",
                clue, touched, previously_clued, old_chop, self.chop, potential_safe
            );
        }
        if let game::Clue::Rank(rank) = clue {
            if rank == 1 {
                for pos in 0..4 {
                    if (1 << pos) & touched > 0 && !self.play_queue.contains(&pos) {
                        self.play_queue.push_back(pos);
                    }
                }
                return;
            }
        }
        if !potential_safe {
            self.play_queue.push_back(touched.trailing_zeros() as u8);
        }
    }

    fn act(&mut self, game: &game::Game) -> game::Move {
        if let Some(pos) = self.play_queue.pop_front() {
            if pos as i8 > self.chop {
                self.chop += 1;
            }
            return game::Move::Play(pos);
        }
        // look for critical cards on chop:
        if let Some(play_move) = self.give_save_clue(game) {
            return play_move;
        }
        // look for potential play clues
        if let Some(play_move) = self.give_play_clue(game) {
            return play_move;
        }
        self.discard(game)
    }
}
