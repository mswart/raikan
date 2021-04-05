use std::collections::VecDeque;

use crate::game::{self, CardPlayState, PositionSet};

pub struct HyphenatedPlayer {
    play_queue: VecDeque<u8>,
    debug: bool,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            play_queue: VecDeque::with_capacity(5),
            debug,
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
        for trash_rank in 1..=game.min_played_rank() {
            for pos in 0..game.num_hand_cards(0) {
                if game.card_must_rank(0, pos, trash_rank) {
                    return game::Move::Discard(pos);
                }
            }
        }
        let chop = self.chop(0, game);
        if chop >= 0 {
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for pos in 0..game.num_hand_cards(0) {
                if game.card_must_rank(0, pos, *rank) {
                    return game::Move::Discard(pos);
                }
            }
        }
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
                return Some(game::Move::Clue(
                    player as u8,
                    game::Clue::Color(card.suite.clue_color()),
                ));
                // return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
            }
        }
        None
    }

    fn playable_safe(&self, game: &game::Game) -> Option<game::Move> {
        let min_play_rank = game.min_played_rank();
        for pos in 0..game.num_hand_cards(0) {
            if game.card_must_rank(0, pos, min_play_rank + 1) {
                return Some(game::Move::Play(pos));
            }
        }
        None
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, _game: &game::Game) {}

    fn clued(
        &mut self,
        clue: game::Clue,
        touched: PositionSet,
        previously_clued: PositionSet,
        _game: &game::Game,
    ) {
        let old_chop = (!previously_clued).last().unwrap_or(0);
        let potential_safe = !previously_clued.is_full() && touched.contains(old_chop);
        // self.chop = self.chop(0, game) as i8;
        if self.debug {
            println!(
                "Got clued with {:?}; touched {:?}; previously clued {:?}; potential safe {}",
                clue, touched, previously_clued, potential_safe
            );
        }
        if let game::Clue::Rank(rank) = clue {
            if rank == 1 {
                for pos in 0..4 {
                    if touched.contains(pos) && !self.play_queue.contains(&pos) {
                        self.play_queue.push_back(pos);
                    }
                }
                return;
            }
        }
        if !potential_safe {
            self.play_queue.push_back(touched.first().expect("asdf"));
        }
    }

    fn act(&mut self, game: &game::Game) -> game::Move {
        if let Some(pos) = self.play_queue.pop_front() {
            return game::Move::Play(pos);
        }
        if let Some(play_move) = self.playable_safe(game) {
            return play_move;
        }
        // look for critical cards on chop:
        if let Some(safe_clue) = self.give_save_clue(game) {
            return safe_clue;
        }
        // look for potential play clues
        if let Some(play_clue) = self.give_play_clue(game) {
            return play_clue;
        }
        self.discard(game)
    }
}
