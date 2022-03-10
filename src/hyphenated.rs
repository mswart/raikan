use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::game::{self, CardPlayState};
use crate::{
    card_quantum::{CardQuantum, Variant},
    PositionSet,
};

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

#[derive(PartialEq, Eq, Clone)]
pub struct Slot {
    pub card: game::Card,
    pub clued: bool,
    pub play: bool,
    pub trash: bool,
    pub quantum: CardQuantum,
    locked: bool,
    pub fixed: bool,
}

impl Slot {
    fn update_slot_attributes(&mut self, play_states: &PlayStates) {
        let mut all_trash = true;
        let mut all_playable = true;
        for card in self.quantum.iter() {
            match play_states[&card] {
                game::CardPlayState::Playable() => all_trash = false,
                game::CardPlayState::CriticalPlayable() => all_trash = false,
                game::CardPlayState::Critical() => {
                    all_trash = false;
                    all_playable = false;
                    break;
                }
                game::CardPlayState::Normal() => {
                    all_trash = false;
                    all_playable = false;
                    break;
                }
                game::CardPlayState::Trash() => all_playable = false,
                game::CardPlayState::Dead() => all_playable = false,
            }
        }
        if all_playable {
            self.play = true;
        }
        if all_trash {
            self.trash = true;
            self.play = false;
        }
    }
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.card, f)?;
        if self.clued {
            f.write_str("'")?;
        }
        if self.trash {
            f.write_str("kt")?;
        }
        if self.play {
            f.write_str("!")?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Ord, Eq, Clone, Debug)]
pub struct LineScore {
    discard_risks: i8,
    score: u8,
    clued: u8,
    play: u8,
    errors: u8,
    bonus: u8,
}

impl PartialOrd for LineScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.score.cmp(&other.score) {
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Equal => {}
        }
        match (self.discard_risks as i32
            + self.play as i32
            + self.clued as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10)
            .cmp(
                &(other.discard_risks as i32
                    + other.play as i32
                    + other.clued as i32 * 2
                    + other.bonus as i32
                    - other.errors as i32 * 10),
            ) {
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Equal => {}
        }
        Some(self.play.cmp(&other.play))
    }
}

impl LineScore {
    pub fn zero() -> Self {
        Self {
            score: 0,
            clued: 0,
            play: 0,
            discard_risks: 0,
            errors: 0,
            bonus: 0,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    pub fn bad() -> Self {
        Self {
            score: 0,
            clued: 0,
            play: 0,
            discard_risks: 0,
            errors: 20,
            bonus: 0,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PlayStates {
    variant: Variant,
    states: [CardPlayState; 25],
    first_one_discard: [bool; 5],
}

impl PlayStates {
    fn new() -> Self {
        let v = Variant {};
        let mut states = [CardPlayState::Normal(); 25];
        for suit in v.suits().iter() {
            states[v.suit_index(suit) * 5] = CardPlayState::Playable();
            states[v.suit_index(suit) * 5 + 5 - 1] = CardPlayState::Critical();
        }
        Self {
            variant: v,
            states,
            first_one_discard: [false; 5],
        }
    }

    fn played(&mut self, card: &game::Card) {
        let offset = self.variant.suit_index(&card.suit) * 5;
        self.states[offset + card.rank as usize - 1] = CardPlayState::Trash();
        if card.rank == 5 {
            return;
        }
        match self.states[offset + card.rank as usize] {
            CardPlayState::Normal() => {
                self.states[offset + card.rank as usize] = CardPlayState::Playable()
            }
            CardPlayState::Critical() => {
                self.states[offset + card.rank as usize] = CardPlayState::CriticalPlayable()
            }
            _ => {}
        }
    }

    fn discarded(&mut self, card: &game::Card) {
        let offset = self.variant.suit_index(&card.suit) * 5;
        if card.rank == 1 && !self.first_one_discard[self.variant.suit_index(&card.suit)] {
            self.first_one_discard[self.variant.suit_index(&card.suit)] = true;
            return;
        }
        match self.states[offset + card.rank as usize - 1] {
            CardPlayState::Normal() => {
                self.states[offset + card.rank as usize - 1] = CardPlayState::Critical()
            }
            CardPlayState::Playable() => {
                self.states[offset + card.rank as usize - 1] = CardPlayState::CriticalPlayable()
            }
            CardPlayState::Critical() => {
                for higher_rank in card.rank..=5 {
                    self.states[offset + higher_rank as usize - 1] = CardPlayState::Dead();
                }
            }
            CardPlayState::CriticalPlayable() => {
                for higher_rank in card.rank..=5 {
                    self.states[offset + higher_rank as usize - 1] = CardPlayState::Dead();
                }
            }
            _ => {}
        }
    }
}

impl core::ops::Index<&game::Card> for PlayStates {
    type Output = CardPlayState;

    fn index(&self, card: &game::Card) -> &Self::Output {
        &self.states[self.variant.suit_index(&card.suit) * 5 + card.rank as usize - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let suit = game::Suit::Red();
        let p = PlayStates::new();
        assert_eq!(p[&game::Card { rank: 1, suit }], CardPlayState::Playable());
        assert_eq!(p[&game::Card { rank: 2, suit }], CardPlayState::Normal());
        assert_eq!(p[&game::Card { rank: 3, suit }], CardPlayState::Normal());
        assert_eq!(p[&game::Card { rank: 4, suit }], CardPlayState::Normal());
        assert_eq!(p[&game::Card { rank: 5, suit }], CardPlayState::Critical());
    }

    #[test]
    fn play_card() {
        let suit = game::Suit::Red();
        let mut p = PlayStates::new();
        assert_eq!(p[&game::Card { rank: 2, suit }], CardPlayState::Normal());
        p.played(&game::Card { rank: 1, suit });
        assert_eq!(p[&game::Card { rank: 1, suit }], CardPlayState::Trash());
        assert_eq!(p[&game::Card { rank: 2, suit }], CardPlayState::Playable());
        p.played(&game::Card { rank: 2, suit });
        assert_eq!(p[&game::Card { rank: 3, suit }], CardPlayState::Playable());
        p.played(&game::Card { rank: 3, suit });
        assert_eq!(p[&game::Card { rank: 4, suit }], CardPlayState::Playable());
        p.played(&game::Card { rank: 4, suit });
        assert_eq!(
            p[&game::Card { rank: 5, suit }],
            CardPlayState::CriticalPlayable()
        );
    }

    #[test]
    fn discard_card() {
        let suit = game::Suit::Blue();
        let mut p = PlayStates::new();
        p.discarded(&game::Card { rank: 3, suit });
        assert_eq!(p[&game::Card { rank: 3, suit }], CardPlayState::Critical());
        p.discarded(&game::Card { rank: 3, suit });
        assert_eq!(p[&game::Card { rank: 3, suit }], CardPlayState::Dead());
        assert_eq!(p[&game::Card { rank: 4, suit }], CardPlayState::Dead());
        assert_eq!(p[&game::Card { rank: 5, suit }], CardPlayState::Dead());
    }

    #[test]
    fn play_critical() {
        let suit = game::Suit::Yellow();
        let mut p = PlayStates::new();
        p.discarded(&game::Card { rank: 2, suit });
        assert_eq!(p[&game::Card { rank: 2, suit }], CardPlayState::Critical());
        p.played(&game::Card { rank: 1, suit });
        assert_eq!(
            p[&game::Card { rank: 2, suit }],
            CardPlayState::CriticalPlayable()
        );
        p.played(&game::Card { rank: 2, suit });
        assert_eq!(p[&game::Card { rank: 2, suit }], CardPlayState::Trash());
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Line (turn: {})\n", self.turn))?;
        for (player, hand) in self.hands.iter().enumerate() {
            f.write_str("[")?;
            for (pos, slot) in hand.iter().enumerate() {
                if pos > 0 {
                    f.write_str(", ")?;
                }
                if player > 0 {
                    std::fmt::Debug::fmt(&slot.card, f)?;
                    f.write_str(" ")?;
                } else {
                    f.write_str("?? ")?;
                }
                std::fmt::Display::fmt(&slot.quantum, f)?;
                if slot.clued {
                    f.write_str("'")?;
                } else {
                    f.write_str(" ")?;
                }
                if slot.trash {
                    f.write_str("kt")?;
                } else if slot.play {
                    f.write_str("! ")?;
                } else {
                    f.write_str("  ")?;
                }
            }
            f.write_str("]\n")?;
        }
        f.write_str(" clued cards: ")?;
        std::fmt::Debug::fmt(&self.clued_cards, f)?;
        f.write_str("\n")?;
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone)]
pub struct Line {
    pub hands: Vec<VecDeque<Slot>>,
    clued_cards: BTreeSet<game::Card>,
    tracked_cards: BTreeMap<game::Card, u8>,
    turn: u8,
    variant: Variant,
    play_states: PlayStates,
    score: u8,
}

impl Line {
    pub fn new(num_players: u8) -> Self {
        let mut hands = Vec::new();
        for _ in 0..num_players {
            hands.push(VecDeque::new());
        }
        Self {
            clued_cards: BTreeSet::new(),
            hands,
            tracked_cards: BTreeMap::new(),
            turn: 0,
            variant: Variant {},
            play_states: PlayStates::new(),
            score: 0,
        }
    }

    fn score(&self, extra_error: u8) -> LineScore {
        let mut discard_risks = 0;
        let mut clued = 0;
        let mut play = 0;
        let mut errors = extra_error;
        if cfg!(debug_assertions) && extra_error > 0 {
            println!("error {extra_error}: initial error passed in",);
        }
        let mut bonus = 0;
        for hand in self.hands.iter().skip(1) {
            let mut queued_actions = 0;
            let mut chop = true;
            let mut discard_risk = 0;
            for slot in hand.iter().rev() {
                let play_state = self.play_states[&slot.card];
                if slot.clued {
                    clued += 1;
                    if slot.play {
                        play += 1;
                        queued_actions += 1;
                    }
                    if slot.trash {
                        if let Some(error) = match play_state {
                            CardPlayState::Trash() => {
                                queued_actions += 1;
                                None
                            }
                            CardPlayState::Dead() => {
                                queued_actions += 1;
                                None
                            }
                            CardPlayState::Critical() => Some(3),
                            CardPlayState::CriticalPlayable() => Some(3),
                            CardPlayState::Playable() => Some(2),
                            CardPlayState::Normal() => Some(1),
                        } {
                            if cfg!(debug_assertions) {
                                println!(
                                    "Error 2: trash card {:?} ({}) is NOT trash",
                                    slot.card, slot.quantum
                                );
                            }
                            errors += error;
                        }
                    } else {
                        if let Some(error) = match play_state {
                            CardPlayState::Trash() => Some(2),
                            CardPlayState::Dead() => Some(2),
                            _ => None,
                        } {
                            if cfg!(debug_assertions) {
                                println!(
                                    "Error 2: clued card {:?} ({}) is trash",
                                    slot.card, slot.quantum
                                );
                            }
                            errors += error;
                        }
                    }
                } else if chop {
                    chop = false;
                    if !self.clued_cards.contains(&slot.card) {
                        match play_state {
                            CardPlayState::Critical() => discard_risk -= 5,
                            CardPlayState::CriticalPlayable() => discard_risk -= 5,
                            CardPlayState::Playable() => discard_risk -= 2,
                            _ => {}
                        }
                    }
                }
                if slot.play {
                    if let Some(error) = match play_state {
                        CardPlayState::Playable() => None,
                        CardPlayState::CriticalPlayable() => None,
                        CardPlayState::Critical() => Some(3),
                        CardPlayState::Normal() => Some(2),
                        CardPlayState::Dead() => Some(1),
                        CardPlayState::Trash() => Some(1),
                    } {
                        if cfg!(debug_assertions) {
                            println!(
                                "Error {error}: {play_state:?} card ({:?}; {}) marked as to play",
                                slot.card, slot.quantum
                            );
                        }
                        errors += error;
                    }
                }
                if !slot.trash && !slot.quantum.contains(&slot.card) {
                    let error = match play_state {
                        CardPlayState::Playable() => 2,
                        CardPlayState::Critical() => 3,
                        CardPlayState::CriticalPlayable() => 3,
                        CardPlayState::Normal() => 2,
                        CardPlayState::Dead() => 1,
                        CardPlayState::Trash() => 1,
                    };
                    if cfg!(debug_assertions) {
                        println!(
                            "Error {error}: {play_state:?} card {:?} is not contained in its quantum {}",
                            slot.card, slot.quantum
                        );
                    }
                    errors += error;
                }
                if slot.quantum.size() == 1 {
                    bonus += 1;
                }
            }
            if discard_risk != 0 && queued_actions < 1 {
                discard_risks += discard_risk;
            }
        }
        LineScore {
            score: self.score,
            clued,
            play,
            discard_risks,
            errors,
            bonus,
        }
    }

    pub fn drawn(&mut self, player: usize, card: game::Card) {
        self.hands[player].push_front(Slot {
            quantum: CardQuantum::new(self.variant),
            card: card,
            clued: false,
            play: false,
            trash: false,
            locked: false,
            fixed: false,
        });
        self.track_card(card);
    }

    pub fn own_drawn(&mut self) {
        let mut hand = Slot {
            quantum: CardQuantum::new(self.variant),
            play: false,
            trash: false,
            clued: false,
            card: game::Card {
                suit: self.variant.suits()[0],
                rank: 0,
            },
            locked: false,
            fixed: false,
        };
        for (card, count) in self.tracked_cards.iter() {
            if *count == card.suit.card_count(card.rank) {
                // a card is lost -> updated maximal possible score based on remaining cards
                hand.quantum.remove_card(card, false);
            }
        }
        self.hands[0].push_front(hand);
    }

    fn played(
        &mut self,
        player: usize,
        pos: usize,
        card: game::Card,
        successful: bool,
        _blind: bool,
    ) {
        self.turn += 1;
        let removed = self.hands[player].remove(pos).expect("Game ensures this");
        if player == 0 {
            self.track_card(card);
            if removed.clued && successful {
                self.clued_cards.insert(card);
                for slot in self.hands[0].iter_mut() {
                    slot.quantum.remove_card(&card, true);
                }
            }
        }
        if successful {
            self.score += 1;
            self.play_states.played(&card);
        } else {
            self.clued_cards.remove(&card);
            self.play_states.discarded(&card);
        }
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.turn += 1;
        self.play_states.discarded(&card);
        self.hands[player].remove(pos);
        if player == 0 {
            self.track_card(card);
        }
    }

    fn foreign_chop(&self, player: usize) -> i8 {
        for (pos, slot) in self.hands[player].iter().enumerate().rev() {
            if !slot.clued {
                return pos as i8;
            }
        }
        return -1;
    }

    fn track_card(&mut self, card: game::Card) {
        let count = self
            .tracked_cards
            .entry(card)
            .and_modify(|e| *e += 1)
            .or_insert(1);
        if *count == card.suit.card_count(card.rank) {
            // all instances of card are tracked (elsewhere!), card cannot be in our hand
            for slot in self.hands[0].iter_mut() {
                slot.quantum.remove_card(&card, false);
            }
        }
    }

    pub fn clued(
        &mut self,
        who: usize,
        whom: usize,
        clue: game::Clue,
        touched: game::PositionSet,
        previously_clued: game::PositionSet,
    ) -> u8 {
        self.turn += 1;
        let mut error = 0;
        let newly_clued = touched - previously_clued;
        for pos in 0..self.hands[whom].len() {
            let slot = &mut self.hands[whom][pos];
            let old_size = slot.quantum.size();
            match clue {
                game::Clue::Rank(rank) => slot
                    .quantum
                    .limit_by_rank(rank as usize, touched.contains(pos as u8)),
                game::Clue::Color(color) => slot
                    .quantum
                    .limit_by_suit(&color.suit(), touched.contains(pos as u8)),
            }
            if old_size != 0 && slot.quantum.size() == 0 && slot.quantum.hard_size() == 1 {
                slot.quantum.soft_clear();
                slot.fixed = true;
            }
            if old_size != 1 && slot.quantum.size() == 1 {
                let card = slot.quantum.iter().nth(0).expect("we checked the size");
                if slot.clued || newly_clued.contains(pos as u8) {
                    self.clued_cards.insert(card);
                    if !slot.play {
                        slot.locked = true;
                    }
                }

                slot.update_slot_attributes(&self.play_states);
                for other_pos in 0..self.hands[whom].len() {
                    if other_pos != pos {
                        self.hands[whom][other_pos].quantum.remove_card(&card, true);
                    }
                }
            }
        }
        if newly_clued.is_empty() {
            let focus = touched.first().expect("empty clues are not implemented");
            let slot = &mut self.hands[whom][focus as usize];
            if slot.play && !slot.locked && !slot.fixed {
                // useless reclue
                if cfg!(debug_assertions) {
                    println!(
                        "Error 1: focused card {:?} ({}) already has play clue",
                        slot.card, slot.quantum,
                    );
                }
                error += 1;
            }
            slot.play = true;
            return error;
        }

        let old_chop = self.foreign_chop(whom);

        let mut potential_safe = false;
        let focus = if old_chop >= 0 && touched.contains(old_chop as u8) {
            let chop_slot = &mut self.hands[whom][old_chop as usize];
            // check whether it can be a safe clue.
            for potential_card in chop_slot.quantum.clone().iter() {
                match self.play_states[&potential_card] {
                    game::CardPlayState::Critical() => {
                        if potential_card.rank == 5 && clue != game::Clue::Rank(5) {
                            chop_slot.quantum.remove_card(&potential_card, true);
                            // 5 will only be safed via rank
                        } else {
                            potential_safe = true
                        }
                    }
                    game::CardPlayState::Dead() => {
                        chop_slot.quantum.remove_card(&potential_card, true);
                    }
                    game::CardPlayState::Trash() => {
                        chop_slot.quantum.remove_card(&potential_card, true);
                    }
                    game::CardPlayState::Normal() => {
                        chop_slot.quantum.remove_card(&potential_card, true);
                    }
                    _ => {}
                }
            }
            old_chop as u8
        } else {
            touched
                .first()
                .expect("We have check previously that touched must contain something")
        };

        // somebody else was clued -> remember which cards are clued
        for pos in (touched - previously_clued).iter_first(focus) {
            let slot = self.hands[whom]
                .get_mut(pos as usize)
                .expect("own and game state out of sync");
            slot.clued = true;
            if !slot.locked {
                for card in self.clued_cards.iter() {
                    slot.quantum.remove_card(card, true);
                }
            }
            if pos == focus {
                if !potential_safe {
                    slot.play = true;
                    for potential_card in slot.quantum.clone().iter() {
                        match self.play_states[&potential_card] {
                            game::CardPlayState::Playable() => {}
                            game::CardPlayState::CriticalPlayable() => {}
                            _ => slot.quantum.remove_card(&potential_card, true),
                        }
                    }
                }
                if slot.quantum.size() == 1 {
                    // for self mode
                    self.clued_cards.insert(
                        slot.quantum
                            .clone()
                            .iter()
                            .nth(0)
                            .expect("We checked the size"),
                    );
                }
            }
            slot.update_slot_attributes(&self.play_states);
            if pos == focus && slot.trash {
                error += 5;
            }
            if who > 0 && whom != 0 {
                let card = slot.card.clone();
                for own_hand in self.hands[0].iter_mut() {
                    if own_hand.clued {
                        own_hand.quantum.remove_card(&card, true);
                    }
                }
            }
            if whom != 0 {
                self.clued_cards.insert(self.hands[whom][pos as usize].card);
            }
        }
        error
    }

    pub fn clue(&mut self, whom: usize, clue: game::Clue) -> Option<LineScore> {
        let mut touched = PositionSet::new(self.hands[whom].len() as u8);
        let mut previously_clued = PositionSet::new(self.hands[whom].len() as u8);
        for (pos, slot) in self.hands[whom].iter().enumerate() {
            if slot.card.affected(clue) {
                touched.add(pos as u8);
            }
            if slot.clued {
                previously_clued.add(pos as u8);
            }
        }
        if touched.is_empty() {
            return None;
        }
        let error = self.clued(0, whom, clue, touched, previously_clued);
        Some(self.score(error))
    }

    fn discard(&mut self) -> game::Move {
        // look for trash
        let mut chop = -1;
        for (pos, slot) in self.hands[0].iter().enumerate() {
            if slot.trash {
                return game::Move::Discard(pos as u8);
            }
            if !slot.clued {
                chop = pos as i8;
            }
        }

        if chop >= 0 {
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for (pos, slot) in self.hands[0].iter().enumerate() {
                if slot.quantum.is_rank(*rank) {
                    return game::Move::Discard(pos as u8);
                }
            }
        }
        // nothing clear found; drop newest card
        game::Move::Discard(0)
    }

    fn play(&mut self) -> Option<game::Move> {
        for (pos, slot) in self.hands[0].iter_mut().enumerate() {
            if slot.trash {
                continue;
            }
            if slot.clued {
                slot.update_slot_attributes(&self.play_states);
            }
            if slot.trash {
                slot.play = false;
            }
            if slot.play {
                return Some(game::Move::Play(pos as u8));
            }
        }
        None
    }
}

pub struct HyphenatedPlayer {
    debug: bool,
    variant: Variant,
    turn: u8,
    line: Line,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            variant: Variant {},
            turn: 0,
            line: Line::new(0),
        }
    }

    pub fn line(&self) -> Line {
        self.line.clone()
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, num_players: u8) {
        self.variant = Variant {};
        self.turn = 0;
        self.line = Line::new(num_players);
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
