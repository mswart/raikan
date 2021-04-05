use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::card_quantum::{CardQuantum, Variant};
use crate::game::{self, CardPlayState, PositionSet};

struct OwnHand {
    quantum: CardQuantum,
}

impl std::fmt::Debug for OwnHand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.quantum, f)
    }
}

struct ForeignHand {
    card: game::Card,
    clued: bool,
}

impl std::fmt::Debug for ForeignHand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.card, f)
    }
}

pub struct HyphenatedPlayer {
    play_queue: VecDeque<u8>,
    debug: bool,
    hand: VecDeque<OwnHand>,
    hands: Vec<VecDeque<ForeignHand>>,
    clued_cards: BTreeSet<game::Card>,
    variant: Variant,
    tracked_cards: BTreeMap<game::Card, u8>,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            play_queue: VecDeque::with_capacity(5),
            debug,
            clued_cards: BTreeSet::new(),
            hand: VecDeque::new(),
            hands: Vec::new(),
            variant: Variant {},
            tracked_cards: BTreeMap::new(),
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
            let cards = game.num_hand_cards(player);
            for pos in 0..cards {
                let card = game.player_card(pos, player);
                let play_state = card.play_state(&game);
                if game.card_cluded(pos as u8, player) {
                    continue;
                }
                if self.clued_cards.contains(&card) {
                    continue;
                };
                if let CardPlayState::Playable() = play_state {
                    // check color clue:
                    let mut valid_rank = true;
                    let mut valid_suite = true;
                    for other_pos in 0..pos {
                        let other_card = game.player_card(other_pos, player);
                        if other_card.suite == card.suite {
                            valid_suite = false;
                        }
                        if other_card.rank == card.rank {
                            valid_rank = false;
                        }
                    }
                    if valid_rank {
                        return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
                    }
                    if valid_suite {
                        return Some(game::Move::Clue(
                            player as u8,
                            game::Clue::Color(card.suite.clue_color()),
                        ));
                    }
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

    fn track_card(&mut self, card: game::Card) {
        let count = self
            .tracked_cards
            .entry(card)
            .and_modify(|e| *e += 1)
            .or_insert(1);
        if *count == card.suite.card_count(card.rank) {
            for hand in self.hand.iter_mut() {
                // a card is lost -> updated maximal possible score based on remaining cards
                hand.quantum.remove_card(&card);
            }
        }
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, game: &game::Game) {
        self.variant = Variant {};
        self.hand.clear();
        self.hands.clear();
        for _ in 0..game.num_players() - 1 {
            self.hands.push(VecDeque::new());
        }
        self.clued_cards.clear();
    }

    fn drawn(&mut self, player: usize, card: game::Card) {
        self.hands[player - 1].push_front(ForeignHand {
            card: card,
            clued: false,
        });
        self.track_card(card);
    }

    fn own_drawn(&mut self) {
        let mut hand = OwnHand {
            quantum: CardQuantum::new(self.variant),
        };
        for (card, count) in self.tracked_cards.iter() {
            if *count == card.suite.card_count(card.rank) {
                // a card is lost -> updated maximal possible score based on remaining cards
                hand.quantum.remove_card(card);
            }
        }
        self.hand.push_front(hand);

        if self.debug {
            println!("Own drawn: {:?}", self.hand);
        }
    }

    fn played(
        &mut self,
        player: usize,
        pos: usize,
        _card: game::Card,
        _successful: bool,
        _blind: bool,
    ) {
        if player == 0 {
            self.hand.remove(pos);
        } else {
            self.hands[player - 1].remove(pos);
        }
    }

    fn discarded(&mut self, player: usize, pos: usize, _card: game::Card) {
        if player == 0 {
            self.hand.remove(pos);
        } else {
            self.hands[player - 1].remove(pos);
        }
    }

    fn clued(
        &mut self,
        _who: usize,
        whom: usize,
        clue: game::Clue,
        touched: game::PositionSet,
        previously_clued: game::PositionSet,
        _game: &game::Game,
    ) {
        if whom != 0 {
            // somebody else was clued -> remember which cards are clued
            for pos in (touched - previously_clued).iter() {
                let a = self.hands[whom - 1].get(pos as usize).expect(&format!(
                    "static player count: {} / {:?}",
                    pos,
                    self.hands[whom - 1]
                ));
                self.clued_cards.insert(a.card);
            }
            return;
        }
        for pos in 0..touched.max() {
            match clue {
                game::Clue::Rank(rank) => self.hand[pos as usize]
                    .quantum
                    .limit_by_rank(rank as usize, touched.contains(pos)),
                game::Clue::Color(color) => self.hand[pos as usize]
                    .quantum
                    .limit_by_suite(&color.suite(), touched.contains(pos)),
            }
        }
        let old_chop = (!previously_clued).last().unwrap_or(0);
        let mut potential_safe = !previously_clued.is_full() && touched.contains(old_chop);
        // self.chop = self.chop(0, game) as i8;
        if self.debug {
            println!(
                "Got clued with {:?}; touched {:?}; previously clued {:?}; potential safe {}: hand {:?}",
                clue, touched, previously_clued, potential_safe, self.hand,
            );
        }
        if let game::Clue::Rank(rank) = clue {
            potential_safe = false;
            if rank == 1 {
                for pos in touched.iter() {
                    if !self.play_queue.contains(&pos) {
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
        while let Some(pos) = self.play_queue.pop_front() {
            // check whether card is clearly trash by now
            let mut skip = false;
            let min_played_rank = game.min_played_rank();
            for rank in 1..=min_played_rank {
                if game.card_must_rank(0, pos, rank) {
                    skip = true;
                }
            }
            if !skip {
                return game::Move::Play(pos);
            }
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
