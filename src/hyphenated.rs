use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::card_quantum::{CardQuantum, Variant};
use crate::game::{self, CardPlayState};

struct OwnHand {
    quantum: CardQuantum,
    play: bool,
    trash: bool,
    clued: bool,
}

impl std::fmt::Debug for OwnHand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.quantum, f)
    }
}

struct ForeignHand {
    card: game::Card,
    clued: bool,
    delayed: bool,
}

impl std::fmt::Debug for ForeignHand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.card, f)
    }
}

impl std::fmt::Debug for HyphenatedPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for card in self.hand.iter() {
            std::fmt::Display::fmt(&card.quantum, f)?;
            f.write_str(", ")?;
        }
        Ok(())
    }
}

pub struct HyphenatedPlayer {
    debug: bool,
    hand: VecDeque<OwnHand>,
    hands: Vec<VecDeque<ForeignHand>>,
    clued_cards: BTreeSet<game::Card>,
    variant: Variant,
    tracked_cards: BTreeMap<game::Card, u8>,
    turn: u8,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            clued_cards: BTreeSet::new(),
            hand: VecDeque::new(),
            hands: Vec::new(),
            variant: Variant {},
            tracked_cards: BTreeMap::new(),
            turn: 0,
        }
    }

    fn chop(&self) -> i8 {
        for (pos, hand) in self.hand.iter().enumerate().rev() {
            // println!("asdf {}", pos);
            if !hand.clued {
                return pos as i8;
            }
        }
        return -1;
        // let mut chop: i8 = game.num_hand_cards(player) as i8 - 1;
        // while chop >= 0 && game.card_cluded(chop as u8, player) {
        //     chop -= 1
        // }
        // chop
    }

    fn foreign_chop(&self, player: usize) -> i8 {
        for (pos, hand) in self.hands[player - 1].iter().enumerate().rev() {
            // println!("asdf {}", pos);
            if !hand.clued {
                return pos as i8;
            }
        }
        return -1;
    }

    fn discard(&mut self) -> game::Move {
        // look for trash
        for pos in 0..self.hand.len() {
            if self.hand[pos].trash {
                return game::Move::Discard(pos as u8);
            }
        }
        let chop = self.chop();

        if chop >= 0 {
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for (pos, hand_card) in self.hand.iter().enumerate() {
                if hand_card.quantum.is_rank(*rank) {
                    return game::Move::Discard(pos as u8);
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
            'hand_pos: for (pos, hand) in self.hands[player - 1].iter().enumerate() {
                if hand.clued && !hand.delayed {
                    continue;
                }
                if self.clued_cards.contains(&hand.card) && !hand.delayed {
                    continue;
                };
                for own_hand in self.hand.iter() {
                    if own_hand.clued && own_hand.quantum.contains(&hand.card) {
                        // card could be on our hand clued - continue
                        continue 'hand_pos;
                    }
                }
                if let CardPlayState::Playable() = hand.card.play_state(&game) {
                    let mut valid_rank = true;
                    let mut valid_suite = true;
                    for other_pos in 0..pos {
                        let other_card = self.hands[player - 1][other_pos].card;
                        if other_card.suite == hand.card.suite {
                            valid_suite = false;
                        }
                        if other_card.rank == hand.card.rank {
                            valid_rank = false;
                        }
                    }
                    for other_pos in pos..self.hands[player - 1].len() {
                        if self.hands[player - 1][other_pos].clued {
                            continue;
                        }
                        let other_card = self.hands[player - 1][other_pos].card;
                        if !self
                            .clued_cards
                            .contains(&self.hands[player - 1][other_pos].card)
                        {
                            continue;
                        }
                        if other_card.suite == hand.card.suite {
                            valid_suite = false;
                        }
                        if other_card.rank == hand.card.rank {
                            valid_rank = false;
                        }
                    }
                    if valid_rank && !(valid_suite && hand.card.rank == 5) {
                        return Some(game::Move::Clue(
                            player as u8,
                            game::Clue::Rank(hand.card.rank),
                        ));
                    }
                    if valid_suite {
                        return Some(game::Move::Clue(
                            player as u8,
                            game::Clue::Color(hand.card.suite.clue_color()),
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
        'player: for player in 1..game.num_players() as usize {
            let mut chop = self.hands[player - 1].len() - 1;
            while self.hands[player - 1][chop].clued {
                if chop == 0 {
                    continue 'player;
                }
                chop -= 1
            }
            let card = self.hands[player - 1][chop].card;
            if let CardPlayState::Critical() = card.play_state(&game) {
                if card.rank == 5 {
                    return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
                }
                return Some(game::Move::Clue(
                    player as u8,
                    game::Clue::Color(card.suite.clue_color()),
                ));
            }
        }
        None
    }

    fn play(&mut self, game: &game::Game) -> Option<game::Move> {
        for pos in 0..game.num_hand_cards(0) {
            if self.hand[pos as usize].play {
                return Some(game::Move::Play(pos));
            }
            if self.hand[pos as usize].clued {
                let mut previous_play_state = None;
                for card in self.hand[pos as usize].quantum.iter() {
                    let play_state = card.play_state(game);
                    if let Some(last_play_state) = previous_play_state {
                        if last_play_state != play_state {
                            previous_play_state = None;
                            break;
                        }
                    } else {
                        previous_play_state = Some(play_state);
                    }
                }
                if let Some(play_state) = previous_play_state {
                    if self.debug {
                        println!(
                            "Pos {} only {:?} possible (quantum {})",
                            pos, play_state, self.hand[pos as usize].quantum,
                        );
                    }
                    match play_state {
                        game::CardPlayState::Playable() => return Some(game::Move::Play(pos)),
                        game::CardPlayState::Trash() => self.hand[pos as usize].trash = true,
                        game::CardPlayState::Dead() => self.hand[pos as usize].trash = true,
                        _ => {}
                    }
                }
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
            // all instances of card are tracked (elsewhere!), card cannot be in our hand
            for hand in self.hand.iter_mut() {
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
        self.turn = 0;
    }

    fn drawn(&mut self, player: usize, card: game::Card) {
        self.hands[player - 1].push_front(ForeignHand {
            card: card,
            clued: false,
            delayed: false,
        });
        self.track_card(card);
    }

    fn own_drawn(&mut self) {
        let mut hand = OwnHand {
            quantum: CardQuantum::new(self.variant),
            play: false,
            trash: false,
            clued: false,
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
        card: game::Card,
        _successful: bool,
        _blind: bool,
    ) {
        self.turn += 1;
        if player == 0 {
            self.hand.remove(pos);
            self.track_card(card);
        } else {
            self.hands[player - 1].remove(pos);
        }
        self.turn += 1;
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.turn += 1;
        if player == 0 {
            self.hand.remove(pos);
            self.track_card(card);
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
        game: &game::Game,
    ) {
        self.turn += 1;
        if whom != 0 {
            // somebody else was clued -> remember which cards are clued
            let chop = self.foreign_chop(whom);
            let mut first = chop == -1 || !touched.contains(chop as u8);
            for pos in (touched - previously_clued).iter() {
                let a = self.hands[whom - 1]
                    .get_mut(pos as usize)
                    .expect("own and game state out of sync");
                a.clued = true;
                self.clued_cards.insert(a.card);
                for own_hand in self.hand.iter_mut() {
                    if own_hand.clued {
                        own_hand.quantum.remove_card(&a.card);
                    }
                }
                if !first || pos as i8 == chop {
                    a.delayed = true;
                }
                first = false;
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
        let old_chop = (!previously_clued)
            .last()
            .unwrap_or(self.hand.len() as u8 - 1);
        let mut potential_safe = !previously_clued.is_full() && touched.contains(old_chop);
        // check whether it is actually a safe clue?
        if potential_safe && clue != game::Clue::Rank(5) {
            let safe = self.hand[old_chop as usize].quantum.iter().any(|card| {
                card.rank != 5 && card.play_state(game) == game::CardPlayState::Critical()
            });
            if !safe {
                potential_safe = false;
            }
        }
        if self.debug {
            println!(
                "Got clued with {:?}; touched {:?}; previously clued {:?}; potential safe {}: hand {:?}",
                clue, touched, previously_clued, potential_safe, self.hand,
            );
        }
        if let game::Clue::Rank(rank) = clue {
            if rank != 5 {
                potential_safe = false;
            }
            if rank == 1 {
                for pos in touched.iter() {
                    self.hand[pos as usize].play = true;
                    self.hand[pos as usize].clued = true;
                }
                return;
            }
        }
        if !potential_safe {
            self.hand[touched.first().expect("asdf") as usize].play = true;
        }
        for pos in touched.iter() {
            let mut hand_pos = &mut self.hand[pos as usize];
            hand_pos.clued = true;
            for card in self.clued_cards.iter() {
                hand_pos.quantum.remove_card(card);
            }
        }
    }

    fn act(&mut self, game: &game::Game) -> game::Move {
        if let Some(play_move) = self.play(game) {
            return play_move;
        }
        // look for potential play clues
        if let Some(play_clue) = self.give_play_clue(game) {
            return play_clue;
        }
        // look for critical cards on chop:
        if let Some(safe_clue) = self.give_save_clue(game) {
            return safe_clue;
        }
        self.discard()
    }
}
