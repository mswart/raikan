use std::collections::VecDeque;

use crate::game::{self, CardPlayState};
use crate::{
    card_quantum::{CardQuantum, Variant},
    PositionSet,
};

use super::card_states::CardStates;
use super::slot::Slot;

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

#[derive(PartialEq, Eq, Clone)]
pub struct Line {
    pub hands: Hands,
    turn: i8,
    variant: Variant,
    pub card_states: CardStates,
    score: u8,
    own_player: u8,
    callbacks: VecDeque<Callback>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Hands {
    pub num_players: u8,
    hand_sizes: [u8; 6],
    max_hand_size: u8,
    hand_slots: [u8; 20],
    slots: [Slot; 20],
    next_slot: Option<u8>,
    used_slots: u8,
}

impl Hands {
    fn insert_slot(&mut self, player: usize, slot: Slot) {
        let slot_index = if let Some(slot_index) = self.next_slot {
            self.next_slot = None;
            slot_index
        } else {
            self.used_slots += 1;
            self.used_slots - 1
        };
        self.slots[slot_index as usize] = slot;
        // update hands
        let offset = player * self.max_hand_size as usize;
        for i in (offset..offset + self.hand_sizes[player] as usize).rev() {
            self.hand_slots[i as usize + 1] = self.hand_slots[i as usize];
        }
        self.hand_slots[offset] = slot_index;
        self.hand_sizes[player] += 1;
    }

    fn remove_slot(&mut self, player: usize, pos: u8) -> usize {
        let offset = player * self.max_hand_size as usize;
        let old_pos = self.hand_slots[offset + pos as usize];
        self.next_slot = Some(old_pos);
        for i in offset + pos as usize..offset + self.hand_sizes[player] as usize {
            self.hand_slots[i as usize] = self.hand_slots[i as usize + 1];
        }
        self.hand_sizes[player] -= 1;
        old_pos as usize
    }

    pub fn iter_hand(&self, player: u8) -> HandIterator {
        HandIterator {
            hands: self,
            player,
            next_pos: 0,
            back_pos: self.hand_sizes[player as usize],
        }
    }

    fn iter_hand_mut(&mut self, player: u8) -> HandMutIterator {
        let back_pos = self.hand_sizes[player as usize];
        HandMutIterator {
            slots: &mut self.slots,
            hand_slots: &self.hand_slots,
            next_pos: 0,
            back_pos,
            offset: player * self.max_hand_size,
        }
    }

    pub fn slot(&self, player: u8, pos: u8) -> &Slot {
        &self.slots
            [self.hand_slots[player as usize * self.max_hand_size as usize + pos as usize] as usize]
    }

    pub fn slot_mut(&mut self, player: u8, pos: u8) -> &mut Slot {
        &mut self.slots
            [self.hand_slots[player as usize * self.max_hand_size as usize + pos as usize] as usize]
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Callback {
    trigger_card: i8,
    target_card: i8,
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Line (turn: {})\n", self.turn))?;
        for player in 0..self.hands.num_players {
            f.write_fmt(format_args!(
                "P{} [",
                (self.own_player + player) % self.hands.num_players
            ))?;
            for (pos, slot) in self.hands.iter_hand(player) {
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
                    f.write_str("🗑 ")?;
                } else if slot.play {
                    f.write_str("! ")?;
                } else {
                    f.write_str("  ")?;
                }
                f.write_fmt(format_args!(" {:>3}", slot.turn))?;
            }
            f.write_str("]\n")?;
        }
        f.write_str(" card states: ")?;
        std::fmt::Debug::fmt(&self.card_states, f)?;
        f.write_str("\n")?;
        Ok(())
    }
}

pub struct HandIterator<'a> {
    hands: &'a Hands,
    next_pos: u8,
    back_pos: u8,
    player: u8,
}

impl<'a> Iterator for HandIterator<'a> {
    type Item = (u8, &'a Slot);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_pos >= self.back_pos {
            return None;
        }
        let offset = self.player * self.hands.max_hand_size;
        let result = Some((
            self.next_pos,
            &self.hands.slots
                [self.hands.hand_slots[offset as usize + self.next_pos as usize] as usize],
        ));
        self.next_pos += 1;
        result
    }
}

impl<'a> DoubleEndedIterator for HandIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.back_pos <= self.next_pos {
            return None;
        }
        let offset = self.player * self.hands.max_hand_size;
        self.back_pos -= 1;
        let result = Some((
            self.back_pos,
            &self.hands.slots
                [self.hands.hand_slots[offset as usize + self.back_pos as usize] as usize],
        ));
        result
    }
}

pub struct HandMutIterator<'a> {
    next_pos: u8,
    back_pos: u8,
    offset: u8,
    slots: &'a mut [Slot],
    hand_slots: &'a [u8],
}

impl<'a> Iterator for HandMutIterator<'a> {
    type Item = (u8, &'a mut Slot);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_pos >= self.back_pos {
            return None;
        }
        unsafe {
            let ptr = self
                .slots
                .as_mut_ptr()
                .add(self.hand_slots[self.offset as usize + self.next_pos as usize] as usize);
            let result = Some((self.next_pos, &mut *ptr));
            self.next_pos += 1;
            result
        }
    }
}

impl<'a> DoubleEndedIterator for HandMutIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.back_pos <= self.next_pos {
            return None;
        }
        unsafe {
            let ptr = self
                .slots
                .as_mut_ptr()
                .add(self.hand_slots[self.offset as usize + self.back_pos as usize] as usize);
            self.back_pos -= 1;
            let result = Some((self.next_pos, &mut *ptr));
            result
        }
    }
}

impl Line {
    pub fn new(num_players: u8, own_player: u8) -> Self {
        let variant = Variant {};
        let empty_slot = Slot {
            quantum: CardQuantum::new(variant),
            play: false,
            trash: false,
            clued: false,
            card: game::Card {
                suit: variant.suits()[0],
                rank: 0,
            },
            locked: false,
            fixed: false,
            turn: -100,
            delayed: 0,
            callbacks: false,
        };
        Self {
            hands: Hands {
                num_players,
                hand_sizes: [0; 6],
                max_hand_size: match num_players {
                    2 => 5,
                    3 => 5,
                    4 => 4,
                    5 => 4,
                    6 => 3,
                    _ => unimplemented!(),
                },

                hand_slots: [0; 20],
                slots: [empty_slot; 20],
                next_slot: None,
                used_slots: 0,
            },
            turn: -16,
            variant: Variant {},
            card_states: CardStates::new(),
            score: 0,
            own_player,
            callbacks: VecDeque::new(),
        }
    }

    pub fn score(&self, extra_error: u8) -> LineScore {
        let mut discard_risks = 0;
        let mut clued = 0;
        let mut play = 0;
        let mut errors = extra_error;
        if cfg!(debug_assertions) && extra_error > 0 {
            println!("error {extra_error}: initial error passed in",);
        }
        let mut bonus = 0;
        for player in 1..self.hands.num_players {
            let mut queued_actions = 0;
            let mut chop = true;
            let mut discard_risk = 0;
            for (_pos, slot) in self.hands.iter_hand(player).rev() {
                let card_state = self.card_states[&slot.card];
                if slot.clued {
                    clued += 1;
                    if slot.play {
                        play += 1;
                        queued_actions += 1;
                    }
                    if slot.trash {
                        if let Some(error) = match card_state.play {
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
                            let mut duplicated_self = false;
                            if let Some((locked_player, turn)) = card_state.locked {
                                if player as u8 == locked_player
                                    && slot.turn != turn
                                    && slot.quantum.size() == 0
                                {
                                    duplicated_self = true;
                                }
                            }
                            if !duplicated_self {
                                if cfg!(debug_assertions) {
                                    println!(
                                    "Error 2: trash card {:?} ({card_state:?}, {}) is NOT trash",
                                    slot.card, slot.quantum
                                );
                                }
                                errors += error;
                            }
                        }
                    } else {
                        if let Some(error) = match card_state.play {
                            CardPlayState::Trash() => Some(2),
                            CardPlayState::Dead() => Some(2),
                            _ => None,
                        } {
                            if cfg!(debug_assertions) {
                                println!(
                                    "Error 2: clued card {:?} ({card_state:?}, {}) is trash",
                                    slot.card, slot.quantum
                                );
                            }
                            errors += error;
                        }
                    }
                } else if chop {
                    chop = false;
                    if card_state.clued.is_none() {
                        match card_state.play {
                            CardPlayState::Critical() => discard_risk -= 5,
                            CardPlayState::CriticalPlayable() => discard_risk -= 5,
                            CardPlayState::Playable() => discard_risk -= 2,
                            _ => {}
                        }
                    }
                }
                if slot.play {
                    if let Some(error) = match card_state.play {
                        CardPlayState::Playable() => None,
                        CardPlayState::CriticalPlayable() => None,
                        CardPlayState::Critical() => Some(3),
                        CardPlayState::Normal() => Some(2),
                        CardPlayState::Dead() => Some(1),
                        CardPlayState::Trash() => Some(1),
                    } {
                        if cfg!(debug_assertions) {
                            println!(
                                "Error {error}: {:?} card ({:?}; {}) marked as to play",
                                card_state.play, slot.card, slot.quantum
                            );
                        }
                        errors += error;
                    }
                }
                if !slot.trash
                    && (card_state.locked.unwrap_or((player as u8, slot.turn))
                        == (player as u8, slot.turn)
                        || slot.quantum.size() > 0)
                    && !slot.quantum.contains(&slot.card)
                {
                    let error = match card_state.play {
                        CardPlayState::Playable() => 2,
                        CardPlayState::Critical() => 3,
                        CardPlayState::CriticalPlayable() => 3,
                        CardPlayState::Normal() => 2,
                        CardPlayState::Dead() => 1,
                        CardPlayState::Trash() => 1,
                    };
                    if cfg!(debug_assertions) {
                        println!(
                            "Error {error}: {card_state:?} card {:?} is not contained in its quantum {}",
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
        let mut quantum = CardQuantum::new(self.variant);
        for (card, state) in self.card_states.iter() {
            if state.tracked_count == card.suit.card_count(card.rank)
                && !state.tracked_places.contains(&(player as i8))
            {
                // player sees all instances of this card
                quantum.remove_card(&card, false);
            }
        }
        self.hands.insert_slot(
            player,
            Slot {
                quantum,
                card,
                clued: false,
                play: false,
                trash: false,
                locked: false,
                fixed: false,
                turn: self.turn as i8,
                delayed: 0,
                callbacks: false,
            },
        );
        if self.turn < 0 {
            self.turn += 1;
        }
        self.track_card(card, player as i8, -2);
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
            turn: self.turn as i8,
            delayed: 0,
            callbacks: false,
        };
        if self.turn < 0 {
            self.turn += 1;
        }
        for (card, state) in self.card_states.iter() {
            if state.tracked_count == card.suit.card_count(card.rank) {
                hand.quantum.remove_card(&card, false);
            }
        }
        self.hands.insert_slot(0, hand);
    }

    pub fn played(
        &mut self,
        player: usize,
        pos: usize,
        card: game::Card,
        successful: bool,
        _blind: bool,
    ) {
        self.turn += 1;
        let slot_index = self.hands.remove_slot(player, pos as u8);
        if player == 0 {
            self.track_card(card, -1, -2);
            if self.hands.slots[slot_index as usize].clued && successful {
                self.card_states[&card].clued = Some(255);
                for (_pos, slot) in self.hands.iter_hand_mut(0) {
                    slot.quantum.remove_card(&card, true);
                }
            }
        } else {
            self.track_card(card, -1, player as i8);
        }
        if successful {
            self.score += 1;
            self.card_states[&card].clued = Some(255);
            self.card_states.played(&card);
        } else {
            if self.hands.slots[slot_index as usize].quantum.size() == 1 {
                self.card_states[&self.hands.slots[slot_index as usize]
                    .quantum
                    .iter()
                    .nth(0)
                    .expect("asd")]
                    .clued = None;
            }
            self.card_states[&card].clued = None;
            self.card_states.discarded(&card);
        }
        for i in (0..self.callbacks.len()).rev() {
            if self.callbacks[i].trigger_card == self.hands.slots[slot_index as usize].turn {
                for player in 0..self.hands.num_players {
                    for (_pos, slot) in self.hands.iter_hand_mut(player) {
                        if slot.turn == self.callbacks[i].target_card {
                            slot.delayed -= 1;
                            if card.rank < 5 {
                                slot.quantum.add_card(
                                    &game::Card {
                                        rank: card.rank + 1,
                                        suit: card.suit,
                                    },
                                    true,
                                );
                            }
                            if slot.delayed == 0 {
                                slot.update_slot_attributes(&self.card_states);
                            }
                        }
                    }
                }
                self.callbacks.remove(i);
            }
        }
        for player in 0..self.hands.num_players {
            for (_pos, slot) in self.hands.iter_hand_mut(player) {
                slot.update_slot_attributes(&self.card_states);
            }
        }
    }

    pub fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.turn += 1;
        self.card_states.discarded(&card);
        let slot_index = self.hands.remove_slot(player, pos as u8);
        if self.hands.slots[slot_index].clued && player > 0 {
            self.card_states[&self.hands.slots[slot_index].card].clued = None;
        }
        if player == 0 {
            self.track_card(card, -1, -2);
        } else {
            self.track_card(card, -1, player as i8);
        }
    }

    fn foreign_chop(&self, player: usize) -> i8 {
        for (pos, slot) in self.hands.iter_hand(player as u8).rev() {
            if !slot.clued {
                return pos as i8;
            }
        }
        return -1;
    }

    fn track_card(&mut self, card: game::Card, place: i8, old_place: i8) {
        let state = &mut self.card_states[&card];
        for place_slot in state.tracked_places.iter_mut() {
            if *place_slot == old_place {
                *place_slot = place;
                break;
            }
        }
        if old_place == -2 {
            state.tracked_count += 1
        }
        let state = &self.card_states[&card];
        if state.tracked_count == card.suit.card_count(card.rank) {
            // all instances of card are tracked (elsewhere!), update card quantum accordingly
            for player in 0..self.hands.num_players {
                if !state.tracked_places.contains(&(player as i8)) {
                    // player actually sees all tracked cards
                    for (_pos, slot) in self.hands.iter_hand_mut(player) {
                        if slot.card != card {
                            slot.quantum.remove_card(&card, false);
                            slot.update_slot_attributes(&self.card_states);
                        }
                    }
                }
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
        for pos in 0..self.hands.hand_sizes[whom] {
            let slot = self.hands.slot_mut(whom as u8, pos);
            let old_size = slot.quantum.size();
            let previsous_first_quantum_card = slot.quantum.iter().nth(0);
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
                if old_size == 1 {
                    self.card_states[&previsous_first_quantum_card.expect("size was tested")]
                        .clued = None;
                }
                slot.fixed = true;
            }
            if old_size != 1 && slot.quantum.size() == 1 {
                let card = slot.quantum.iter().nth(0).expect("we checked the size");
                if slot.clued || newly_clued.contains(pos as u8) {
                    if whom == 0 || slot.card == card {
                        self.card_states[&card].clued = Some(255);
                        self.card_states[&card].locked = Some((whom as u8, slot.turn));
                    }
                    if !slot.play {
                        slot.locked = true;
                    }
                }

                slot.update_slot_attributes(&self.card_states);
                for other_pos in 0..self.hands.hand_sizes[whom] {
                    if other_pos != pos {
                        self.hands
                            .slot_mut(whom as u8, other_pos)
                            .quantum
                            .remove_card(&card, true);
                    }
                }
            }
        }
        if newly_clued.is_empty() {
            let focus = touched.first().expect("empty clues are not implemented");
            let slot = self.hands.slot_mut(whom as u8, focus);
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
            if !slot.fixed {
                slot.play = true;
            }
            return error;
        }

        let old_chop = self.foreign_chop(whom);

        let mut potential_safe = false;
        let focus = if old_chop >= 0 && touched.contains(old_chop as u8) {
            let chop_slot = self.hands.slot_mut(whom as u8, old_chop as u8);
            // check whether it can be a safe clue.
            for potential_card in chop_slot.quantum.clone().iter() {
                match self.card_states[&potential_card].play {
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
            let slot = self.hands.slot_mut(whom as u8, pos);
            slot.clued = true;
            if !slot.locked {
                for (card, state) in self.card_states.iter_clued() {
                    if state.clued != Some(whom as u8) {
                        slot.quantum.remove_card(&card, true);
                    }
                }
            }
            if pos == focus {
                if potential_safe {
                    for potential_card in slot.quantum.clone().iter() {
                        match self.card_states[&potential_card].play {
                            game::CardPlayState::Normal() => {
                                slot.quantum.remove_card(&potential_card, true)
                            }
                            game::CardPlayState::Dead() => {
                                slot.quantum.remove_card(&potential_card, true);
                            }
                            game::CardPlayState::Trash() => {
                                slot.quantum.remove_card(&potential_card, true);
                            }
                            _ => {}
                        }
                    }
                } else {
                    slot.play = true;
                    for potential_card in self.hands.slot(whom as u8, pos).quantum.clone().iter() {
                        match self.card_states[&potential_card].play {
                            game::CardPlayState::Playable() => {}
                            game::CardPlayState::CriticalPlayable() => {}
                            game::CardPlayState::Trash() => self
                                .hands
                                .slot_mut(whom as u8, pos)
                                .quantum
                                .remove_card(&potential_card, true),
                            game::CardPlayState::Dead() => self
                                .hands
                                .slot_mut(whom as u8, pos)
                                .quantum
                                .remove_card(&potential_card, true),
                            _ => {
                                let mut all_connecting_cards = true;
                                for previous_rank in (1..potential_card.rank).rev() {
                                    let previous_card = game::Card {
                                        rank: previous_rank,
                                        suit: potential_card.suit,
                                    };
                                    let previous_state = self.card_states[&previous_card];
                                    match previous_state.play {
                                        game::CardPlayState::Trash() => {}
                                        game::CardPlayState::Dead() => {
                                            all_connecting_cards = false;
                                            break;
                                        }
                                        _ => {
                                            if previous_state.clued.unwrap_or(whom as u8)
                                                == whom as u8
                                            {
                                                let mut delayed = 0;
                                                for (other_pos, other_slot) in
                                                    self.hands.iter_hand(whom as u8)
                                                {
                                                    if other_pos == pos {
                                                        continue;
                                                    }
                                                    if other_slot.clued
                                                        && other_slot
                                                            .quantum
                                                            .contains(&previous_card)
                                                    {
                                                        self.callbacks.push_front(Callback {
                                                            trigger_card: other_slot.turn,
                                                            target_card: self
                                                                .hands
                                                                .slot(whom as u8, pos)
                                                                .turn,
                                                        });
                                                        delayed += 1;
                                                    }
                                                }
                                                if delayed > 0 {
                                                    self.hands.slot_mut(whom as u8, pos).delayed +=
                                                        delayed;
                                                }
                                                all_connecting_cards = false;
                                                break;
                                            }
                                        }
                                    }
                                }
                                if !all_connecting_cards {
                                    self.hands
                                        .slot_mut(whom as u8, pos)
                                        .quantum
                                        .remove_card(&potential_card, true)
                                }
                            }
                        }
                    }
                }
                let slot = self.hands.slot_mut(whom as u8, pos);
                if slot.quantum.size() == 1 {
                    let card = slot
                        .quantum
                        .clone()
                        .iter()
                        .nth(0)
                        .expect("We checked the size");
                    // for self mode
                    if whom == 0 || card == slot.card {
                        self.card_states[&card].clued = Some(255);
                        self.card_states[&card].locked = Some((whom as u8, slot.turn));
                        if whom == 0 {
                            for player in 1..self.hands.num_players {
                                if player == who as u8 {
                                    continue;
                                }
                                for (_pos, slot) in self.hands.iter_hand_mut(player) {
                                    if slot.clued {
                                        slot.quantum.remove_card(&card, true);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let slot = self.hands.slot_mut(whom as u8, pos);
            slot.update_slot_attributes(&self.card_states);
            if pos == focus && slot.trash {
                error += 5;
            }
            if whom != 0 {
                let card = slot.card.clone();
                for player in 0..self.hands.num_players {
                    if player == who as u8 || player == whom as u8 {
                        continue;
                    }
                    for (_pos, slot) in self.hands.iter_hand_mut(player) {
                        if slot.clued {
                            slot.quantum.remove_card(&card, true);
                        }
                    }
                }
                if self.card_states[&card].clued.is_none() {
                    self.card_states[&card].clued = Some(whom as u8);
                }
            }
        }
        error
    }

    pub fn clue(&mut self, whom: usize, clue: game::Clue) -> Option<LineScore> {
        let mut touched = PositionSet::new(self.hands.hand_sizes[whom]);
        let mut previously_clued = PositionSet::new(self.hands.hand_sizes[whom]);
        for (pos, slot) in self.hands.iter_hand_mut(whom as u8) {
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

    pub fn discard(&mut self) -> game::Move {
        // look for trash
        let mut chop = -1;
        for (pos, slot) in self.hands.iter_hand_mut(0) {
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
            for (pos, slot) in self.hands.iter_hand_mut(0) {
                if slot.quantum.is_rank(*rank) {
                    return game::Move::Discard(pos as u8);
                }
            }
        }
        // nothing clear found; drop newest card
        game::Move::Discard(0)
    }

    pub fn play(&mut self) -> Option<game::Move> {
        for (pos, slot) in self.hands.iter_hand_mut(0) {
            if slot.trash {
                continue;
            }
            if slot.clued {
                slot.update_slot_attributes(&self.card_states);
            }
            if slot.play {
                return Some(game::Move::Play(pos as u8));
            }
        }
        None
    }
}
