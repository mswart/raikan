use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::game::{self, Card, CardPlayState};
use crate::{
    card_quantum::{CardQuantum, Variant},
    PositionSet,
};

struct OwnSlot {
    quantum: CardQuantum,
    play: bool,
    trash: bool,
    clued: bool,
}

impl std::fmt::Debug for OwnSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.quantum, f)
    }
}

struct ForeignSlot {
    card: game::Card,
    clued: bool,
    delayed: bool,
}

impl std::fmt::Debug for ForeignSlot {
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
    hand: VecDeque<OwnSlot>,
    hands: Vec<VecDeque<ForeignSlot>>,
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
        for (pos, slot) in self.hands[player - 1].iter().enumerate().rev() {
            // println!("asdf {}", pos);
            if !slot.clued {
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
            for (pos, slot) in self.hand.iter().enumerate() {
                if slot.quantum.is_rank(*rank) {
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
            'hand_pos: for (pos, slot) in self.hands[player - 1].iter().enumerate() {
                if slot.clued && !slot.delayed {
                    continue;
                }
                if self.clued_cards.contains(&slot.card) && !slot.delayed {
                    continue;
                };
                for own_slot in self.hand.iter() {
                    if own_slot.clued && own_slot.quantum.contains(&slot.card) {
                        // card could be on our hand clued - continue
                        continue 'hand_pos;
                    }
                }
                if let CardPlayState::Playable() = slot.card.play_state(&game) {
                    let mut valid_rank = true;
                    let mut valid_suit = true;
                    for other_pos in 0..pos {
                        let other_card = self.hands[player - 1][other_pos].card;
                        if other_card.suit == slot.card.suit {
                            valid_suit = false;
                        }
                        if other_card.rank == slot.card.rank {
                            valid_rank = false;
                        }
                    }
                    for other_pos in pos..self.hands[player - 1].len() {
                        if self.hands[player - 1][other_pos].clued {
                            continue;
                        }
                        let other_card = self.hands[player - 1][other_pos].card;
                        if other_pos != pos && other_card == slot.card {
                            // we would clue a card twice
                            continue 'hand_pos;
                        }
                        if !self.clued_cards.contains(&other_card) {
                            continue;
                        }
                        if other_card.suit == slot.card.suit {
                            valid_suit = false;
                        }
                        if other_card.rank == slot.card.rank {
                            valid_rank = false;
                        }
                    }
                    if valid_rank && !(valid_suit && slot.card.rank == 5) {
                        return Some(game::Move::Clue(
                            player as u8,
                            game::Clue::Rank(slot.card.rank),
                        ));
                    }
                    if valid_suit {
                        return Some(game::Move::Clue(
                            player as u8,
                            game::Clue::Color(slot.card.suit.clue_color()),
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
                if card.rank == 5 || card.rank == 2 {
                    return Some(game::Move::Clue(player as u8, game::Clue::Rank(card.rank)));
                }
                let mut rank_score = 0;
                let mut color_score = 0;
                for (i, slot) in self.hands[player - 1].iter().enumerate() {
                    if i == chop {
                        continue;
                    }
                    let effect = match slot.card.play_state(game) {
                        CardPlayState::Trash() => -5,
                        CardPlayState::Dead() => -5,
                        CardPlayState::Critical() => 30,
                        CardPlayState::Normal() => {
                            if self.clued_cards.contains(&slot.card) {
                                -10
                            } else {
                                10
                            }
                        }
                        CardPlayState::Playable() => {
                            if self.clued_cards.contains(&slot.card) {
                                -20
                            } else {
                                20
                            }
                        }
                    };
                    if slot.card.rank == card.rank {
                        rank_score += effect;
                    }
                    if slot.card.suit == card.suit {
                        color_score += effect;
                    }
                }

                return Some(game::Move::Clue(
                    player as u8,
                    if rank_score > color_score {
                        game::Clue::Rank(card.rank)
                    } else {
                        game::Clue::Color(card.suit.clue_color())
                    },
                ));
            }
        }
        None
    }

    fn play(&mut self, game: &game::Game) -> Option<game::Move> {
        for pos in 0..game.num_hand_cards(0) {
            if self.hand[pos as usize].trash {
                continue;
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
            if self.hand[pos as usize].trash {
                self.hand[pos as usize].play = false;
            }
            if self.hand[pos as usize].play {
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
        if *count == card.suit.card_count(card.rank) {
            // all instances of card are tracked (elsewhere!), card cannot be in our hand
            for slot in self.hand.iter_mut() {
                slot.quantum.remove_card(&card);
            }
        }
    }

    /// Evaluate the best opening move (primarily which ones to clue first)
    /// As usual, we try to be efficient with the clues but allow progress (letting one
    /// player play all ones might produce a pace issue)
    fn opening_score(&self, preivous_suits: PositionSet) -> Option<(u8, game::Move)> {
        let mut best_score = 0;
        let mut best_opening = game::Move::Discard(self.hand.len() as u8 - 1);
        // collect the overall visible number of ones:
        let mut visible_ones = game::PositionSet::new(self.variant.len() as u8);
        for hand in self.hands.iter() {
            for slot in hand.iter() {
                if slot.card.rank != 1 {
                    continue;
                }
                let index = self.variant.suit_index(&slot.card.suit) as u8;
                visible_ones.add(index);
            }
        }
        // 1. check one clues on all players:
        'player: for (player, hand) in self.hands.iter().enumerate() {
            let mut touched_suits = preivous_suits;
            for slot in hand.iter() {
                if slot.card.rank != 1 {
                    continue;
                }
                let index = self.variant.suit_index(&slot.card.suit) as u8;
                if touched_suits.contains(index) {
                    continue 'player;
                }
                touched_suits.add(index);
            }
            let mut score = (touched_suits.len() - preivous_suits.len()) * 10;
            if score == 0 {
                continue;
            }
            // check whether the remaining ones are still practical:

            if let Some((further_score, _opening)) = self.opening_score(touched_suits) {
                score += further_score - 1;
            }
            if self.debug {
                println!(
                    "[{:?}] player {} with ones: {}",
                    preivous_suits, player, score
                );
            }

            if score > best_score {
                best_score = score;
                best_opening = game::Move::Clue(player as u8 + 1, game::Clue::Rank(1));
            }
        }
        // 2. check for color clues:
        for (player, hand) in self.hands.iter().enumerate() {
            'clue_color: for (index, suit) in self.variant.suits().iter().enumerate() {
                if !visible_ones.contains(index as u8) {
                    continue;
                }
                if preivous_suits.contains(index as u8) {
                    continue;
                }
                let mut score = 10;
                let mut touched = PositionSet::new(6);
                for player_pos in hand.iter() {
                    if *suit != player_pos.card.suit {
                        continue;
                    }
                    if touched.len() == 0 && player_pos.card.rank != 1 {
                        continue 'clue_color;
                    }
                    if touched.contains(player_pos.card.rank) {
                        continue 'clue_color;
                    };
                    touched.add(player_pos.card.rank);
                }
                if touched.len() == 0 {
                    continue;
                }
                score += touched.len() - 1;
                if touched.contains(5) {
                    score += 5;
                }
                if self.debug {
                    println!(
                        "[{:?}] player {} with {}: {}",
                        preivous_suits, player, suit, score
                    );
                }
                if score >= best_score && best_score != 30 {
                    best_score = score;
                    best_opening =
                        game::Move::Clue(player as u8 + 1, game::Clue::Color(suit.clue_color()));
                }
            }
        }
        if best_score > 0 {
            Some((best_score, best_opening))
        } else {
            None
        }
    }

    // /// Evaluate the best opening move (primarily which ones to clue first)
    // /// As usual, we try to be efficient with the clues but allow progress (letting one
    // /// player play all ones might produce a pace issue)
    // fn opening(&self, game:) -> game::Move {
    //     if let Some((_score, opening)) =
    //         self.opening_score(PositionSet::new(self.variant.len() as u8))
    //     {
    //         opening
    //     } else {
    //         // look for critical cards on chop:
    //         if let Some(safe_clue) = self.give_save_clue(game) {
    //             return safe_clue;
    //         }
    //         game::Move::Discard(self.hand.len() as u8 - 1)
    //     }
    // }
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
        self.hands[player - 1].push_front(ForeignSlot {
            card: card,
            clued: false,
            delayed: false,
        });
        self.track_card(card);
    }

    fn own_drawn(&mut self) {
        let mut hand = OwnSlot {
            quantum: CardQuantum::new(self.variant),
            play: false,
            trash: false,
            clued: false,
        };
        for (card, count) in self.tracked_cards.iter() {
            if *count == card.suit.card_count(card.rank) {
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
        who: usize,
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
                if who > 0 {
                    for own_hand in self.hand.iter_mut() {
                        if own_hand.clued {
                            own_hand.quantum.remove_card(&a.card);
                        }
                    }
                }
                if pos as i8 == chop {
                    // check whether it can be a safe clue.
                    match clue {
                        game::Clue::Rank(5) => a.delayed = true,
                        game::Clue::Rank(1) => a.delayed = false,
                        game::Clue::Rank(rank) => {
                            for &suit in game.suits.iter() {
                                if (Card { suit, rank }.play_state(game)
                                    == game::CardPlayState::Critical())
                                {
                                    a.delayed = true;
                                    break;
                                }
                            }
                        }
                        game::Clue::Color(color) => {
                            for rank in 2..4 {
                                if (Card {
                                    suit: color.suit(),
                                    rank,
                                }
                                .play_state(game)
                                    == game::CardPlayState::Critical())
                                {
                                    a.delayed = true;
                                    break;
                                }
                            }
                        }
                    }
                } else if !first && clue != game::Clue::Rank(1) {
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
                    .limit_by_suit(&color.suit(), touched.contains(pos)),
            }
        }
        let old_chop = (!previously_clued)
            .last()
            .unwrap_or(self.hand.len() as u8 - 1);
        let mut potential_safe = !previously_clued.is_full() && touched.contains(old_chop);
        // check whether it is actually a safe clue?
        if potential_safe && clue != game::Clue::Rank(5) {
            let safe_worthy = self.hand[old_chop as usize].quantum.iter().any(|card| {
                card.rank != 5 && card.play_state(game) == game::CardPlayState::Critical()
            });
            if !safe_worthy {
                potential_safe = false;
            }
        }
        if self.debug {
            println!(
                "Got clued with {:?}; touched {:?}; previously clued {:?}; potential safe {}: hand {:?}",
                clue, touched, previously_clued, potential_safe, self.hand,
            );
        }
        if clue == game::Clue::Rank(1) {
            for pos in touched.iter() {
                self.hand[pos as usize].play = true;
            }
            return;
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
        if self.turn == 0 {
            if let Some((_score, opening)) =
                self.opening_score(PositionSet::new(self.variant.len() as u8))
            {
                return opening;
            }
        }
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
