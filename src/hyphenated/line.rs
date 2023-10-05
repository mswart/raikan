use std::collections::VecDeque;

use crate::game::{self, CardPlayState};
use crate::{
    card_quantum::{CardQuantum, Variant},
    PositionSet,
};

use super::card_states::CardStates;
use super::slot::Slot;

use slog;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct LineScore {
    discard_risks: i8,
    score: u8,
    clued: u8,
    finess: u8,
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
            + self.finess as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10)
            .cmp(
                &(other.discard_risks as i32
                    + other.play as i32
                    + other.clued as i32 * 2
                    + other.finess as i32 * 2
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
            finess: 0,
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
            finess: 0,
            play: 0,
            discard_risks: 0,
            errors: 20,
            bonus: 0,
        }
    }
}

#[derive(Clone)]
pub struct Line {
    pub hands: Hands,
    turn: i8,
    variant: Variant,
    pub card_states: CardStates,
    score: u8,
    own_player: u8,
    pub callbacks: VecDeque<Callback>,
    logger: slog::Logger,
}

impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.hands == other.hands
            && self.turn == other.turn
            && self.variant == other.variant
            && self.card_states == other.card_states
            && self.score == other.score
            && self.own_player == other.own_player
            && self.callbacks == other.callbacks
    }
}
impl Eq for Line {}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Hands {
    pub num_players: u8,
    pub hand_sizes: [u8; 6],
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
            self.hand_slots[i + 1] = self.hand_slots[i];
        }
        self.hand_slots[offset] = slot_index;
        self.hand_sizes[player] += 1;
    }

    fn remove_slot(&mut self, player: usize, pos: u8) -> usize {
        let offset = player * self.max_hand_size as usize;
        let old_pos = self.hand_slots[offset + pos as usize];
        self.next_slot = Some(old_pos);
        for i in offset + pos as usize..offset + self.hand_sizes[player] as usize {
            self.hand_slots[i] = self.hand_slots[i + 1];
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

    pub fn slot_index(&self, player: u8, pos: u8) -> u8 {
        self.hand_slots[(player * self.max_hand_size + pos) as usize]
    }

    pub fn slot(&self, player: u8, pos: u8) -> &Slot {
        &self.slots[self.slot_index(player, pos) as usize]
    }

    pub fn slot_mut(&mut self, player: u8, pos: u8) -> &mut Slot {
        &mut self.slots[self.slot_index(player, pos) as usize]
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Callback {
    WaitingPlay {
        delayed_slot: u8,
        pending_slot: u8,
    },
    PotentialPrompt {
        delayed_slot: u8,
        potential_player: u8,
    },
    PotentialFiness {
        delayed_slot: u8,
        pending_slot: u8,
        expected_card: game::Card,
    },
    Finess {
        delayed_slot: u8,
        pending_slot: u8,
    },
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
        Some((
            self.back_pos,
            &self.hands.slots
                [self.hands.hand_slots[offset as usize + self.back_pos as usize] as usize],
        ))
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
            Some((self.next_pos, &mut *ptr))
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PlayRelation {
    Normal(),
    WaitingPlay(),
    Prompt(),
    Finess(),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkCertainty {
    Prep(),
    Unambigious(),
    Ambigious(),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FirstAction {
    NonSelf(),
    SelfPrompt(),
    SelfFiness(),
}

#[derive(Clone, Debug)]
struct PlayEvaluation {
    who: usize,
    whom: usize,
    pos: u8,
    card: game::Card,
    places: [(u8, u8, PlayRelation); 5],
    pending_marks: bool,
    played_rank: u8,
    marked_cards: [PositionSet; 6],
    logger: slog::Logger,
}

impl PlayEvaluation {
    fn prep(line: &mut Line, who: usize, whom: usize, pos: u8, logger: slog::Logger) -> u8 {
        let mut evaluation = Self {
            who,
            whom,
            pos,
            card: line.hands.slot(whom as u8, pos).card,
            places: [(0, 0, PlayRelation::Normal()); 5],
            pending_marks: false,
            played_rank: 0,
            marked_cards: [PositionSet::new(6); 6],
            logger,
        };
        match evaluation.resolve(
            line,
            PositionSet::new(line.hands.hand_sizes[whom]),
            FirstAction::SelfFiness(),
        ) {
            Ok(()) => {
                evaluation.mark(line, MarkCertainty::Prep(), true);
                0
            }
            Err(_) => 2,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn test(
        line: &mut Line,
        card: game::Card,
        who: usize,
        whom: usize,
        pos: u8,
        premarked: PositionSet,
        first_action: FirstAction,
        logger: slog::Logger,
    ) -> Result<Self, bool> {
        let mut evaluation = Self {
            who,
            whom,
            pos,
            card,
            places: [(0, 0, PlayRelation::Normal()); 5],
            pending_marks: false,
            played_rank: 0,
            marked_cards: [PositionSet::new(6); 6],
            logger,
        };
        match evaluation.resolve(line, premarked, first_action) {
            Ok(_) => Ok(evaluation),
            Err(hard) => Err(!hard),
        }
    }

    fn empty() -> Self {
        PlayEvaluation {
            who: 0,
            whom: 0,
            pos: 0,
            card: game::Card {
                suit: game::Suit::Red(),
                rank: 0,
            },
            places: [(0, 0, PlayRelation::Normal()); 5],
            pending_marks: false,
            played_rank: 0,
            marked_cards: [PositionSet::new(6); 6],
            logger: slog::Logger::root(slog::Discard, slog::o!()),
        }
    }

    fn find_clued_card(
        &mut self,
        line: &mut Line,
        clued_player: u8,
        previous_card: game::Card,
        allowed_self_search: FirstAction,
    ) -> Option<bool> {
        if let Some((found_pos, slot)) =
            line.hands
                .iter_hand_mut(clued_player)
                .find(|(other_pos, slot)| {
                    !self.marked_cards[clued_player as usize].contains(*other_pos)
                                    && slot.clued
                                    && slot.card == previous_card // quantum.contains(&previous_card)
                                    && (slot.play || slot.delayed > 0)
                })
        {
            slog::debug!(
                self.logger,
                "Looking for {:?}: queued on {clued_player}'s hand: {slot:?}",
                previous_card
            );
            self.marked_cards[clued_player as usize].add(found_pos);
            self.places[previous_card.rank as usize - 1] =
                (clued_player, found_pos, PlayRelation::WaitingPlay());
            self.pending_marks = true;
            return Some(true);
        }
        if clued_player == self.who as u8 {
            if let Some((found_pos, slot)) =
                line.hands
                    .iter_hand_mut(clued_player)
                    .find(|(other_pos, slot)| {
                        !self.marked_cards[clued_player as usize].contains(*other_pos)
                            && slot.clued
                            && slot.quantum.contains(&previous_card)
                            && slot.quantum.size() == 1
                            && !slot.play
                            && slot.delayed == 0
                    })
            {
                slog::debug!(
                    self.logger,
                    "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                    previous_card
                );
                self.marked_cards[clued_player as usize].add(found_pos);
                self.places[previous_card.rank as usize - 1] =
                    (clued_player, found_pos, PlayRelation::Prompt());
                self.pending_marks = true;
                return Some(true);
            }
            slog::debug!(
                self.logger,
                "Looking for {:?}: not found: card is invisible (clued on clue givers hand); assume duplicate clue",
                previous_card
            );
            return None;
        }
        if clued_player == self.whom as u8 && allowed_self_search == FirstAction::NonSelf() {
            if let Some((found_pos, slot)) =
                line.hands
                    .iter_hand_mut(clued_player)
                    .find(|(other_pos, slot)| {
                        !self.marked_cards[clued_player as usize].contains(*other_pos)
                            && slot.clued
                            && slot.quantum.contains(&previous_card)
                            && slot.quantum.size() == 1
                            && !slot.play
                            && slot.delayed == 0
                    })
            {
                slog::debug!(
                    self.logger,
                    "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                    previous_card
                );
                self.marked_cards[clued_player as usize].add(found_pos);
                self.places[previous_card.rank as usize - 1] =
                    (clued_player, found_pos, PlayRelation::Prompt());
                self.pending_marks = true;
                return Some(true);
            }
            return Some(false);
        }
        if let Some((found_pos, slot)) =
            line.hands
                .iter_hand_mut(clued_player)
                .find(|(other_pos, slot)| {
                    !self.marked_cards[clued_player as usize].contains(*other_pos)
                        && slot.clued
                        && slot.quantum.contains(&previous_card)
                        && !slot.play
                        && slot.delayed == 0
                })
        {
            slog::debug!(
                self.logger,
                "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                previous_card
            );
            self.marked_cards[clued_player as usize].add(found_pos);
            self.places[previous_card.rank as usize - 1] =
                (clued_player, found_pos, PlayRelation::Prompt());
            self.pending_marks = true;
            return Some(true);
        }
        slog::debug!(
            self.logger,
            "Looking for {:?}: clued somewhere but not found",
            previous_card
        );
        // card not found but was marked as "clued" (probably clued card doubled used).
        Some(false)
    }

    fn resolve(
        &mut self,
        line: &mut Line,
        premarked: PositionSet,
        first_action: FirstAction,
    ) -> Result<(), bool> {
        self.marked_cards[self.whom] = premarked;
        self.marked_cards[self.whom].add(self.pos);
        self.places[self.card.rank as usize - 1] =
            (self.whom as u8, self.pos, PlayRelation::Normal());
        let mut allowed_self_search = first_action;
        match line.card_states[&self.card].play {
            game::CardPlayState::Playable() => Ok(()),
            game::CardPlayState::CriticalPlayable() => Ok(()),
            game::CardPlayState::Trash() => Err(true),
            game::CardPlayState::Dead() => Err(true),
            _ => {
                let mut all_connecting_cards = true;
                'rank_loop: for previous_rank in 1..self.card.rank {
                    let previous_card = game::Card {
                        rank: previous_rank,
                        suit: self.card.suit,
                    };
                    let previous_state = line.card_states[&previous_card];
                    if previous_state.play == game::CardPlayState::Trash() {
                        // card already played
                        self.played_rank = previous_rank;
                        continue;
                    }
                    // obvious place: everybody knows where it is
                    if previous_state.clued == Some(255) {
                        // known place for everybody
                        // need to add notify?
                        let (player, turn) = previous_state
                            .locked
                            .expect("clued = Some(255) should also lock the slot, or?");
                        for (pos, slot) in line.hands.iter_hand(player) {
                            if slot.turn == turn {
                                self.places[previous_rank as usize - 1] =
                                    (player, pos, PlayRelation::Prompt());
                                slog::debug!(
                                    self.logger,
                                    "Looking for {:?}: clued on {player}'s hand: {slot:?}",
                                    previous_card
                                );
                            }
                        }
                        self.pending_marks = true;
                        continue;
                    }

                    // card is clued (but player doesn't know for sure):
                    if let Some(clued_player) = previous_state.clued {
                        match self.find_clued_card(
                            line,
                            clued_player,
                            previous_card,
                            allowed_self_search,
                        ) {
                            Some(true) => continue,
                            Some(false) => return Err(false),
                            None => {}
                        }
                    }

                    // check finess positions:
                    for finess_player in (1..line.hands.num_players).rev() {
                        if finess_player == self.who as u8 {
                            // a clue giver does not know their own hand => they can't target their finess cards
                            continue;
                        }
                        if finess_player == self.whom as u8
                            && allowed_self_search != FirstAction::SelfFiness()
                        {
                            continue;
                        }
                        if line
                            .hands
                            .iter_hand_mut(finess_player)
                            .any(|(other_pos, slot)| {
                                !self.marked_cards[finess_player as usize].contains(other_pos)
                                    && slot.clued
                                    && slot.quantum.contains(&previous_card)
                                    && !matches!(
                                        line.card_states[&slot.card].play,
                                        CardPlayState::Playable()
                                            | CardPlayState::CriticalPlayable()
                                    )
                            })
                        {
                            // clued card would play first
                            continue;
                        }
                        // ensure how clued card would be played earlier:
                        for (other_pos, slot) in line.hands.iter_hand_mut(finess_player) {
                            if slot.clued
                                || self.marked_cards[finess_player as usize].contains(other_pos)
                            {
                                continue;
                            }
                            if slot.card != previous_card {
                                break;
                                // if line.card_states.play_quantum.contains(&slot.card) {
                                //     continue;
                                // } else {
                                //     break;
                                // }
                            }
                            slog::debug!(
                                self.logger,
                                "Looking for {:?}: found on {finess_player}'s finess position ({other_pos}): {slot:?}",
                                previous_card
                            );
                            self.marked_cards[finess_player as usize].add(other_pos);
                            self.places[previous_rank as usize - 1] =
                                (finess_player, other_pos, PlayRelation::Finess());
                            self.pending_marks = true;
                            allowed_self_search = FirstAction::SelfFiness();
                            continue 'rank_loop;
                        }
                    }
                    if let Some((found_pos, slot)) =
                        line.hands.iter_hand_mut(0).find(|(other_pos, slot)| {
                            !self.marked_cards[0_usize].contains(*other_pos)
                                && slot.clued
                                && slot.quantum.contains(&previous_card)
                                && (self.who != 0 || slot.quantum.size() == 1)
                                && (slot.play || slot.delayed > 0)
                        })
                    {
                        slog::debug!(
                            self.logger,
                            "Looking for {:?}: queued in our hand ({found_pos}): {slot:?}",
                            previous_card
                        );
                        self.marked_cards[0_usize].add(found_pos);
                        self.places[previous_rank as usize - 1] =
                            (0, found_pos, PlayRelation::WaitingPlay());
                        self.pending_marks = true;
                        continue;
                    }

                    if allowed_self_search != FirstAction::NonSelf() {
                        for (other_pos, slot) in line.hands.iter_hand_mut(0) {
                            if self.who == 0 && slot.quantum.size() != 1 {
                                continue;
                            }
                            if !slot.clued || self.marked_cards[0].contains(other_pos) {
                                continue;
                            }
                            if !slot.quantum.contains(&previous_card) {
                                continue;
                            }
                            slog::debug!(
                                self.logger,
                                "Looking for {:?}: self-prompted in our hand ({other_pos}): {slot:?} - {allowed_self_search:?}",
                                previous_card
                            );
                            self.marked_cards[0_usize].add(other_pos);
                            self.places[previous_rank as usize - 1] =
                                (0, other_pos, PlayRelation::Prompt());
                            self.pending_marks = true;
                            continue 'rank_loop;
                        }
                    } else {
                        for (other_pos, slot) in line.hands.iter_hand_mut(0) {
                            if self.marked_cards[0].contains(other_pos) {
                                continue;
                            }
                            if slot.quantum.size() != 1 || !slot.quantum.contains(&previous_card) {
                                continue;
                            }
                            self.marked_cards[0_usize].add(other_pos);
                            self.pending_marks = true;
                            if slot.clued {
                                slog::debug!(
                                    self.logger,
                                    "Looking for {:?}: known self-prompted in our hand ({other_pos}): {slot:?}",
                                    previous_card
                                );
                                self.places[previous_rank as usize - 1] =
                                    (0, other_pos, PlayRelation::Prompt());
                            } else {
                                slog::debug!(
                                    self.logger,
                                    "Looking for {:?}: finess in our hand ({other_pos}): {slot:?}",
                                    previous_card
                                );
                                self.places[previous_rank as usize - 1] =
                                    (0, other_pos, PlayRelation::Finess());
                            }
                            continue 'rank_loop;
                        }
                    }

                    if allowed_self_search == FirstAction::SelfFiness() && self.who > 0 {
                        for (other_pos, slot) in line.hands.iter_hand_mut(0) {
                            if slot.clued || self.marked_cards[0].contains(other_pos) {
                                continue;
                            }
                            if !slot.quantum.contains(&previous_card) {
                                break;
                            }
                            slog::debug!(
                                self.logger,
                                "Looking for {:?}: found on own finess position ({other_pos}): {slot:?}",
                                previous_card
                            );
                            self.marked_cards[0_usize].add(other_pos);
                            self.places[previous_rank as usize - 1] =
                                (0, other_pos, PlayRelation::Finess());
                            self.pending_marks = true;
                            continue 'rank_loop;
                        }
                    }
                    slog::debug!(self.logger, "Looking for {:?}: not found", previous_card);
                    all_connecting_cards = false;
                    break;
                }
                if !all_connecting_cards {
                    Err(false)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn mark(&self, line: &mut Line, certainty: MarkCertainty, correct: bool) {
        if !self.pending_marks {
            return;
        }
        slog::debug!(self.logger, "mark: {self:?}");
        for previous_rank in (self.played_rank + 1)..self.card.rank {
            let previous_card = game::Card {
                rank: previous_rank,
                suit: self.card.suit,
            };
            let (player, pos, relation) = self.places[previous_rank as usize - 1];
            let (next_player, next_pos, _) = self.places[previous_rank as usize];
            let found_slot = line.hands.slot_mut(player, pos);
            match relation {
                PlayRelation::Normal() => {
                    if certainty != MarkCertainty::Ambigious() {
                        found_slot.quantum.soft_clear();
                        found_slot.quantum.add_card(&previous_card, true);
                        found_slot.update_slot_attributes(&line.card_states);
                    }
                    if certainty == MarkCertainty::Prep() && player > 0 {
                        let focused_slot = line.hands.slot_mut(self.whom as u8, self.pos);
                        focused_slot.quantum.remove_card(&previous_card, true);
                    }
                }
                PlayRelation::WaitingPlay() => {
                    if certainty != MarkCertainty::Ambigious() {
                        found_slot.quantum.soft_clear();
                        found_slot.quantum.add_card(&previous_card, true);
                        if certainty == MarkCertainty::Prep() && player == 0 {
                            found_slot.promised = Some(line.turn);
                        }
                        found_slot.update_slot_attributes(&line.card_states);
                    }
                    if self.whom == 0 || certainty == MarkCertainty::Prep() {
                        line.callbacks.push_front(Callback::WaitingPlay {
                            delayed_slot: line.hands.slot_index(next_player, next_pos),
                            pending_slot: line.hands.slot_index(player, pos),
                        });
                        line.hands.slot_mut(next_player, next_pos).delayed += 1;
                    }
                    if certainty == MarkCertainty::Prep() {
                        let focused_slot = line.hands.slot_mut(self.whom as u8, self.pos);
                        focused_slot.quantum.remove_card(&previous_card, true);
                    }
                }
                PlayRelation::Prompt() => match certainty {
                    MarkCertainty::Prep() => {
                        if player != self.whom as u8 {
                            found_slot.quantum.soft_clear();
                            found_slot.quantum.add_card(&previous_card, true);
                        }
                        found_slot.update_slot_attributes(&line.card_states);
                        if self.whom == 0 {
                            line.callbacks.push_front(Callback::WaitingPlay {
                                delayed_slot: line.hands.slot_index(next_player, next_pos),
                                pending_slot: line.hands.slot_index(player, pos),
                            });
                            line.hands.slot_mut(next_player, next_pos).delayed += 1;
                        }
                        if player != self.whom as u8 {
                            let focused_slot = line.hands.slot_mut(self.whom as u8, self.pos);
                            focused_slot.quantum.remove_card(&previous_card, true);
                        }
                    }

                    MarkCertainty::Unambigious() => {
                        found_slot.quantum.soft_clear();
                        found_slot.quantum.add_card(&previous_card, true);
                        found_slot.update_slot_attributes(&line.card_states);
                        if self.whom == 0 {
                            line.callbacks.push_front(Callback::WaitingPlay {
                                delayed_slot: line.hands.slot_index(next_player, next_pos),
                                pending_slot: line.hands.slot_index(player, pos),
                            });
                            line.hands.slot_mut(next_player, next_pos).delayed += 1;
                        }
                    }
                    MarkCertainty::Ambigious() => {
                        line.callbacks.push_front(Callback::PotentialPrompt {
                            delayed_slot: line.hands.slot_index(next_player, next_pos),
                            potential_player: player,
                        });
                        line.hands.slot_mut(next_player, next_pos).delayed += 1;
                    }
                },
                PlayRelation::Finess() => match certainty {
                    MarkCertainty::Prep() => {
                        if player != self.whom as u8 {
                            found_slot.quantum.soft_clear();
                            found_slot.quantum.add_card(&previous_card, true);
                            found_slot.update_slot_attributes(&line.card_states);
                            found_slot.promised = Some(line.turn);
                            found_slot.play = true;
                            line.callbacks.push_front(Callback::PotentialFiness {
                                delayed_slot: line.hands.slot_index(next_player, next_pos),
                                pending_slot: line.hands.slot_index(player, pos),
                                expected_card: previous_card,
                            });
                            line.hands.slot_mut(next_player, next_pos).delayed += 1;
                        }
                    }
                    MarkCertainty::Unambigious() => {
                        found_slot.quantum.soft_clear();
                        found_slot.quantum.add_card(&previous_card, true);
                        found_slot.update_slot_attributes(&line.card_states);
                        found_slot.promised = Some(line.turn);
                        found_slot.play = true;
                        line.callbacks.push_front(Callback::PotentialFiness {
                            delayed_slot: line.hands.slot_index(next_player, next_pos),
                            pending_slot: line.hands.slot_index(player, pos),
                            expected_card: previous_card,
                        });
                        line.hands.slot_mut(next_player, next_pos).delayed += 1;
                    }
                    MarkCertainty::Ambigious() => {
                        if self.whom == 0 || correct {
                            if found_slot.promised.is_none() {
                                found_slot.promised = Some(line.turn);
                                found_slot.quantum.soft_clear();
                            }
                            found_slot.quantum.add_card(&previous_card, true);
                            found_slot.update_slot_attributes(&line.card_states);
                            line.callbacks.push_front(Callback::PotentialFiness {
                                delayed_slot: line.hands.slot_index(next_player, next_pos),
                                pending_slot: line.hands.slot_index(player, pos),
                                expected_card: previous_card,
                            });
                            line.hands.slot_mut(next_player, next_pos).delayed += 1;
                        }
                    }
                },
            }
        }
    }
}

impl Line {
    pub fn with_logger(num_players: u8, own_player: u8, logger: slog::Logger) -> Self {
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
            promised: None,
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
            logger,
        }
    }

    pub fn new(num_players: u8, own_player: u8) -> Self {
        Self::with_logger(
            num_players,
            own_player,
            slog::Logger::root(slog::Discard, slog::o!()),
        )
    }

    pub fn score(&self, extra_error: u8) -> LineScore {
        let mut discard_risks = 0;
        let mut clued = 0;
        let mut finess = 0;
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
                                if player == locked_player
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
                    } else if let Some(error) = match card_state.play {
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
                } else {
                    if chop {
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
                    if slot.promised.is_some() {
                        finess += 1;
                        if !slot.quantum.contains(&slot.card) {
                            if cfg!(debug_assertions) {
                                println!(
                                    "Error 2: wrong finess promised: {:?} {}",
                                    slot.card, slot.quantum
                                );
                            }
                            errors += 2;
                        }
                    }
                }
                if slot.play && slot.delayed == 0 {
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
                    && (card_state.locked.unwrap_or((player, slot.turn)) == (player, slot.turn)
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
            finess,
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
                turn: self.turn,
                delayed: 0,
                callbacks: false,
                promised: None,
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
            turn: self.turn,
            delayed: 0,
            callbacks: false,
            promised: None,
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

    fn stop_finess_due_to_misplay(&mut self, _player: usize) {
        let mut cleared = false;
        for (_pos, slot) in self.hands.iter_hand_mut(0) {
            if !slot.clued && slot.promised.is_some() {
                slot.promised = None;
                slot.quantum.reset_soft();
                slot.play = false;
                slot.update_slot_attributes(&self.card_states);
                // clear up callbacks?
                cleared = true;
            }
        }
        if cleared {
            return;
        }
        for (_pos, slot) in self.hands.iter_hand_mut(0) {
            if slot.clued && slot.quantum.interset(self.card_states.play_quantum) {
                slot.promised = None;
                slot.quantum.reset_soft();
                slot.play = false;
                slot.update_slot_attributes(&self.card_states);
                // clear up callbacks?
                return;
            }
        }
        // fix also other hands?
    }

    pub fn played(&mut self, player: usize, pos: usize, card: game::Card, successful: bool) {
        self.turn += 1;
        let slot_index = self.hands.remove_slot(player, pos as u8);
        let slot = self.hands.slots[slot_index];
        if player == 0 {
            self.track_card(card, -1, -2);
            if slot.clued && successful {
                self.card_states[&card].clued = Some(255);
            }
        } else {
            self.track_card(card, -1, player as i8);
        }
        let mut moved_promise = None;
        if successful {
            self.score += 1;
            self.card_states[&card].clued = Some(255);
            self.card_states.played(&card);
            for player in 0..self.hands.num_players {
                for (_pos, slot) in self.hands.iter_hand_mut(player) {
                    if !slot.clued || slot.locked {
                        continue;
                    }
                    slot.quantum.remove_card(&card, true);
                }
            }
            if let Some(turn) = slot.promised {
                // we are promised a card
                if slot.quantum.size() > 0 && !slot.quantum.contains(&card) && player == 0 {
                    // but we didn't play the promised card
                    let mut moved = false;
                    // first search for clued cards
                    if let Some((found_pos, next_slot)) = self
                        .hands
                        .iter_hand_mut(player as u8)
                        .find(|(_other_pos, next_slot)| {
                            next_slot.turn <= turn
                                && next_slot.clued
                                && next_slot.quantum.interset(slot.quantum)
                        })
                    {
                        slog::debug!(
                            self.logger,
                            "Promised {} but played {card:?}: expect card now at {found_pos} => {next_slot:?}",
                            slot.quantum,
                        );
                        next_slot.promised = Some(turn);
                        next_slot.quantum.soft_limit(slot.quantum);
                        moved = true;
                        moved_promise = Some(self.hands.slot_index(player as u8, found_pos));
                    }
                    if let Some((found_pos, next_slot)) = self
                        .hands
                        .iter_hand_mut(player as u8)
                        .find(|(_other_pos, next_slot)| {
                            next_slot.turn <= turn
                                && !next_slot.clued
                                && next_slot.quantum.interset(slot.quantum)
                        })
                    {
                        slog::debug!(
                            self.logger,
                            "Promised {} but played {card:?}: expect card now at {found_pos} => {next_slot:?}",
                            slot.quantum,
                        );
                        next_slot.promised = Some(turn);
                        next_slot.quantum.soft_limit(slot.quantum);
                        moved = true;
                        moved_promise = Some(self.hands.slot_index(player as u8, found_pos));
                    }
                    if !moved {
                        slog::warn!(
                            self.logger,
                            "Promised {} but played {card:?}: card can't even be one finess!!!",
                            slot.quantum,
                        );
                    }
                }
            }
        } else {
            if slot.quantum.size() == 1 {
                self.card_states[&slot.quantum.iter().next().expect("asd")].clued = None;
            }
            self.card_states[&card].clued = None;
            self.card_states.discarded(&card);
            // TODO: if prompt is misplayed, continue with finess
            // if let Some(turn) = slot.promised { }
            if !slot.clued && pos == 0 {
                self.stop_finess_due_to_misplay(player);
            }
        }

        let mut i = 0;
        while i < self.callbacks.len() {
            i += 1;
            match self.callbacks[i - 1] {
                Callback::WaitingPlay {
                    delayed_slot,
                    pending_slot,
                } => {
                    if delayed_slot as usize == slot_index {
                        i -= 1;
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        if let Some(new_slot_index) = moved_promise {
                            self.callbacks[i - 1] = Callback::WaitingPlay {
                                delayed_slot,
                                pending_slot: new_slot_index,
                            }
                        } else {
                            let slot = &mut self.hands.slots[delayed_slot as usize];
                            slot.delayed -= 1;
                            let next_card = game::Card {
                                rank: card.rank + 1,
                                suit: card.suit,
                            };
                            if slot.quantum.contains_hard(&next_card) {
                                slot.quantum.add_card(&next_card, true);
                            }
                            if slot.delayed == 0 {
                                slot.quantum.soft_limit(self.card_states.play_quantum);
                                slot.update_slot_attributes(&self.card_states);
                            }
                            i -= 1;
                            self.callbacks.remove(i);
                        }
                    }
                }
                Callback::PotentialPrompt {
                    delayed_slot,
                    potential_player,
                } => {
                    if delayed_slot as usize == slot_index {
                        i -= 1;
                        self.callbacks.remove(i);
                    } else if potential_player == player as u8 {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        i -= 1;
                        self.callbacks.remove(i);
                    }
                }
                Callback::PotentialFiness {
                    delayed_slot,
                    pending_slot,
                    expected_card,
                } => {
                    if delayed_slot as usize == slot_index {
                        i -= 1;
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        i -= 1;
                        self.callbacks.remove(i);
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        if card == expected_card {
                            slot.delayed -= 1;
                            let next_card = game::Card {
                                rank: card.rank + 1,
                                suit: card.suit,
                            };
                            if slot.quantum.contains_hard(&next_card) && card.rank < 5 {
                                slot.delayed = 0;
                                slot.update_slot_attributes(&self.card_states);
                                slot.quantum.soft_clear();
                                slot.quantum.add_card(&next_card, true);
                                if self.card_states[&next_card].clued.is_none() {
                                    self.card_states[&next_card].clued = Some(255);
                                    self.card_states[&next_card].locked = Some((0, slot.turn));
                                }
                                let mut j = 0;
                                while j < self.callbacks.len() {
                                    j += 1;
                                    match self.callbacks[j - 1] {
                                        Callback::WaitingPlay {
                                            delayed_slot: delayed_slot2,
                                            ..
                                        } => {
                                            if delayed_slot == delayed_slot2 {
                                                j -= 1;
                                                if j < i {
                                                    i -= 1;
                                                }
                                                self.callbacks.remove(j);
                                            };
                                        }
                                        Callback::PotentialPrompt {
                                            delayed_slot: delayed_slot2,
                                            ..
                                        } => {
                                            if delayed_slot == delayed_slot2 {
                                                j -= 1;
                                                if j < i {
                                                    i -= 1;
                                                }
                                                self.callbacks.remove(j);
                                            };
                                        }
                                        Callback::PotentialFiness {
                                            delayed_slot: delayed_slot2,
                                            pending_slot: pending_slot_index2,
                                            ..
                                        } => {
                                            if delayed_slot == delayed_slot2 {
                                                j -= 1;
                                                if j < i {
                                                    i -= 1;
                                                }
                                                self.callbacks.remove(j);
                                                let pending_slot = &mut self.hands.slots
                                                    [pending_slot_index2 as usize];
                                                if !pending_slot.clued
                                                    && pending_slot.promised.is_some()
                                                {
                                                    pending_slot.quantum.reset_soft();
                                                    pending_slot.promised = None;
                                                }
                                            };
                                        }
                                        Callback::Finess {
                                            delayed_slot: delayed_slot2,
                                            ..
                                        } => {
                                            if delayed_slot == delayed_slot2 {
                                                j -= 1;
                                                if j < i {
                                                    i -= 1;
                                                }
                                                self.callbacks.remove(j);
                                            };
                                        }
                                    };
                                }
                            }
                            // remove all other callbacks on this card
                        } else {
                            if slot.delayed > 0 {
                                slot.delayed -= 1;
                            }
                            if slot.delayed == 0 {
                                slot.update_slot_attributes(&self.card_states);
                            }
                        }
                    }
                }
                Callback::Finess {
                    delayed_slot,
                    pending_slot,
                } => {
                    if delayed_slot as usize == slot_index {
                        i -= 1;
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        if slot.delayed == 0 {
                            slot.update_slot_attributes(&self.card_states);
                        }
                        i -= 1;
                        self.callbacks.remove(i);
                    }
                }
            };
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
        for i in (0..self.callbacks.len()).rev() {
            match self.callbacks[i] {
                Callback::PotentialPrompt {
                    delayed_slot,
                    potential_player,
                } => {
                    if delayed_slot as usize == slot_index {
                        self.callbacks.remove(i);
                    } else if potential_player == player as u8 {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        self.callbacks.remove(i);
                    }
                }
                Callback::WaitingPlay {
                    delayed_slot,
                    pending_slot,
                } => {
                    if delayed_slot as usize == slot_index {
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        if slot.delayed == 0 {
                            slot.update_slot_attributes(&self.card_states);
                        }
                        self.callbacks.remove(i);
                    }
                }
                Callback::PotentialFiness {
                    delayed_slot,
                    pending_slot,
                    expected_card: _,
                } => {
                    if delayed_slot as usize == slot_index {
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        if slot.delayed == 0 {
                            slot.update_slot_attributes(&self.card_states);
                        }
                        self.callbacks.remove(i);
                    }
                }
                Callback::Finess {
                    delayed_slot,
                    pending_slot,
                } => {
                    if delayed_slot as usize == slot_index {
                        self.callbacks.remove(i);
                    } else if pending_slot as usize == slot_index {
                        let slot = &mut self.hands.slots[delayed_slot as usize];
                        slot.delayed -= 1;
                        if slot.delayed == 0 {
                            slot.update_slot_attributes(&self.card_states);
                        }
                        self.callbacks.remove(i);
                    }
                }
            };
        }
    }

    fn foreign_chop(&self, player: usize) -> i8 {
        for (pos, slot) in self.hands.iter_hand(player as u8).rev() {
            if !slot.clued {
                return pos as i8;
            }
        }
        -1
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
    ) -> u8 {
        self.turn += 1;
        for i in (0..self.callbacks.len()).rev() {
            if let Callback::PotentialPrompt {
                delayed_slot,
                potential_player,
            } = self.callbacks[i]
            {
                if potential_player == who as u8 {
                    let slot = &mut self.hands.slots[delayed_slot as usize];
                    slot.delayed -= 1;
                    self.callbacks.remove(i);
                }
            }
        }

        let old_chop = self.foreign_chop(whom);

        let mut error = 0;
        let mut newly_clued = touched;
        for pos in 0..self.hands.hand_sizes[whom] {
            let slot = self.hands.slot_mut(whom as u8, pos);
            if slot.clued {
                newly_clued.remove(pos);
            }
            let old_size = slot.quantum.size();
            let previsous_first_quantum_card = slot.quantum.iter().next();
            match clue {
                game::Clue::Rank(rank) => slot
                    .quantum
                    .limit_by_rank(rank as usize, touched.contains(pos)),
                game::Clue::Color(color) => slot
                    .quantum
                    .limit_by_suit(&color.suit(), touched.contains(pos)),
            }
            if old_size != 0 && slot.quantum.size() == 0 && slot.quantum.hard_size() == 1 {
                slot.quantum.reset_soft();
                if old_size == 1 {
                    self.card_states[&previsous_first_quantum_card.expect("size was tested")]
                        .clued = None;
                }
                slot.fixed = true;
            }
            if touched.contains(pos) {
                slot.clued = true;
            }
            if old_size != 1 && slot.quantum.size() == 1 {
                let card = slot.quantum.iter().next().expect("we checked the size");
                if slot.clued {
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
                error += self.resolve_play_clue(who, whom, focus, touched);
            }
            return error;
        }

        let mut potential_safe = false;
        let focus = if old_chop >= 0 && touched.contains(old_chop as u8) {
            let chop_slot = self.hands.slot_mut(whom as u8, old_chop as u8);
            // check whether it can be a safe clue.
            for potential_card in chop_slot.quantum.clone().iter() {
                match self.card_states[&potential_card].play {
                    game::CardPlayState::Critical() => {
                        if potential_card.rank == 5 && clue != game::Clue::Rank(5) {
                            // 5s will only be safed via rank
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
                        if potential_card.rank == 2 && clue == game::Clue::Rank(2) {
                            potential_safe = true;
                        }
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
        for pos in newly_clued.iter_first(focus) {
            let slot = self.hands.slot_mut(whom as u8, pos);
            if !slot.locked {
                for (card, state) in self.card_states.iter_clued() {
                    if state.clued != Some(whom as u8) {
                        slot.quantum.remove_card(&card, true);
                    }
                }
            }
            if pos == focus {
                if potential_safe
                    && (whom == 0 || !(slot.card.rank == 5 && clue != game::Clue::Rank(5)))
                {
                    for potential_card in slot.quantum.clone().iter() {
                        match self.card_states[&potential_card].play {
                            game::CardPlayState::Normal() => {
                                if potential_card.rank == 2 && clue == game::Clue::Rank(2) {
                                    let mut second_copy_visible_by_both = false;
                                    for i in 0..self.card_states[&potential_card].tracked_count {
                                        let place = self.card_states[&potential_card]
                                            .tracked_places
                                            [i as usize];
                                        if place != who as i8 && place != whom as i8 {
                                            second_copy_visible_by_both = true;
                                        }
                                    }
                                    if second_copy_visible_by_both {
                                        slot.quantum.remove_card(&potential_card, true)
                                    }
                                } else {
                                    slot.quantum.remove_card(&potential_card, true)
                                }
                            }
                            game::CardPlayState::Dead() => {
                                slot.quantum.remove_card(&potential_card, true);
                            }
                            game::CardPlayState::Trash() => {
                                slot.quantum.remove_card(&potential_card, true);
                            }
                            game::CardPlayState::Critical() => {
                                if potential_card.rank == 5 && clue != game::Clue::Rank(5) {
                                    slot.quantum.remove_card(&potential_card, true);
                                    // 5 will only be safed via rank
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    slot.play = true;
                    error += self.resolve_play_clue(who, whom, pos, touched);
                }
                let slot = self.hands.slot_mut(whom as u8, pos);
                if slot.quantum.size() == 1 {
                    let card = slot
                        .quantum
                        .clone()
                        .iter()
                        .next()
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
                let card = slot.card;
                if self.card_states[&card].clued.is_none() {
                    self.card_states[&card].clued = Some(whom as u8);
                } else {
                    // potentially bad clued:
                    slog::debug!(self.logger, "potential card reclue {card:?} ?");
                    let mut sure_trash = true;
                    for alternative_card in slot.quantum.iter() {
                        if self.card_states[&alternative_card].clued.is_none() {
                            sure_trash = false;
                        }
                    }
                    if !sure_trash {
                        error += 1;
                    }
                }
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
            }
        }
        error
    }

    fn resolve_play_clue(
        &mut self,
        who: usize,
        whom: usize,
        pos: u8,
        premarked: PositionSet,
    ) -> u8 {
        let mut error = 0;
        if who == 0 {
            let clued_card = self.hands.slot(whom as u8, pos).card;
            for (_pos, slot) in self.hands.iter_hand(0) {
                if slot.clued && slot.quantum.contains(&clued_card) {
                    slog::debug!(
                        self.logger,
                        "Potential double clue of {clued_card:?} (already on our slot {slot:?}"
                    );
                    error += 1;
                }
            }
        }
        let mut evaluations: [PlayEvaluation; 5] = [
            PlayEvaluation::empty(),
            PlayEvaluation::empty(),
            PlayEvaluation::empty(),
            PlayEvaluation::empty(),
            PlayEvaluation::empty(),
        ];
        let mut num_evaluations = 0;
        // 0. update prompt based on actually clued card:
        if whom > 0 {
            error += PlayEvaluation::prep(
                self,
                who,
                whom,
                pos,
                self.logger.new(slog::o!("eval" => "prep")),
            );
        }
        let mut play_quantum = self.hands.slot(whom as u8, pos).quantum;

        // Evaluating play clue based on Occam's Razor https://hanabi.github.io/docs/level-10#clue-interpretation--occams-razor
        // 1. direct and delayed plays (or prompts of other hands)
        for potential_card in self.hands.slot(whom as u8, pos).quantum.clone().iter() {
            match PlayEvaluation::test(
                self,
                potential_card,
                who,
                whom,
                pos,
                premarked,
                FirstAction::NonSelf(),
                self.logger.new(
                    slog::o!("eval" => "non-self", "card?" => format!("{:?}", potential_card)),
                ),
            ) {
                Ok(evaluation) => {
                    evaluations[num_evaluations] = evaluation;
                    num_evaluations += 1;
                }
                Err(soft) => {
                    self.hands
                        .slot_mut(whom as u8, pos)
                        .quantum
                        .remove_card(&potential_card, true);
                    play_quantum.remove_card(&potential_card, soft);
                }
            }
        }

        // 2. include play clues via self-prompts
        if play_quantum.size() == 0 {
            play_quantum.reset_soft();
            for potential_card in play_quantum.clone().iter() {
                match PlayEvaluation::test(
                    self,
                    potential_card,
                    who,
                    whom,
                    pos,
                    PositionSet::new(self.hands.hand_sizes[whom]),
                    FirstAction::SelfPrompt(),
                    self.logger.new(
                        slog::o!("eval" => "self-prompt", "card?" => format!("{:?}", potential_card)),
                    ),
                ) {
                    Ok(evaluation) => {
                        evaluations[num_evaluations] = evaluation;
                        num_evaluations += 1;
                    }
                    Err(soft) => {
                        self.hands
                            .slot_mut(whom as u8, pos)
                            .quantum
                            .remove_card(&potential_card, true);
                        play_quantum.remove_card(&potential_card, soft);
                    }
                }
            }
        }

        // 3. include play clues via self-finesses
        if play_quantum.size() == 0 {
            play_quantum.reset_soft();
            for potential_card in play_quantum.clone().iter() {
                match PlayEvaluation::test(
                    self,
                    potential_card,
                    who,
                    whom,
                    pos,
                    PositionSet::new(self.hands.hand_sizes[whom]),
                    FirstAction::SelfFiness(),
                    self.logger.new(
                        slog::o!("eval" => "self-finess", "card?" => format!("{:?}", potential_card)),
                    ),
                ) {
                    Ok(evaluation) => {
                        evaluations[num_evaluations] = evaluation;
                        num_evaluations += 1;
                    }
                    Err(soft) => {
                        self.hands
                            .slot_mut(whom as u8, pos)
                            .quantum
                            .remove_card(&potential_card, true);
                        play_quantum.remove_card(&potential_card, soft);
                    }
                }
            }
        }

        match play_quantum.size() {
            0 => error += 2,
            1 => {
                slog::debug!(
                    self.logger,
                    "Play evaluation left only: {:?}",
                    evaluations[0].card
                );
                assert_eq!(num_evaluations, 1);
                self.hands
                    .slot_mut(whom as u8, pos)
                    .quantum
                    .add_card(&evaluations[0].card, true);
                evaluations[0].mark(self, MarkCertainty::Unambigious(), true);
            }
            _ => {
                slog::debug!(
                    self.logger,
                    "Play evaluation left multiple-options: {play_quantum}",
                );
                for evaluation in evaluations.iter_mut() {
                    evaluation.mark(
                        self,
                        MarkCertainty::Ambigious(),
                        whom > 0 && evaluation.card == self.hands.slot(whom as u8, pos).card,
                    );
                }
            }
        }

        error
    }

    pub fn clue(&mut self, whom: usize, clue: game::Clue) -> Option<LineScore> {
        let mut touched = PositionSet::new(self.hands.hand_sizes[whom]);
        for (pos, slot) in self.hands.iter_hand_mut(whom as u8) {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        if touched.is_empty() {
            return None;
        }
        let error = self.clued(0, whom, clue, touched);
        Some(self.score(error))
    }

    pub fn discard(&mut self) -> game::Move {
        // look for trash
        let mut chop = -1;
        for (pos, slot) in self.hands.iter_hand_mut(0) {
            if slot.trash {
                return game::Move::Discard(pos);
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
                    return game::Move::Discard(pos);
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
            if slot.play
                && slot.delayed == 0
                && self.card_states.play_quantum.interset(slot.quantum)
            {
                return Some(game::Move::Play(pos));
            }
        }
        None
    }

    pub fn print_callbacks(&self, prefix: &str) {
        let mut output = prefix.to_string();
        for (pos, callback) in self.callbacks.iter().enumerate() {
            if pos > 0 {
                output += "\n   ";
            }
            match callback {
                Callback::WaitingPlay {
                    delayed_slot,
                    pending_slot,
                } => {
                    output += &format!(
                        "WaitingPlay ({:?} for {:?})",
                        self.hands.slots[*delayed_slot as usize],
                        self.hands.slots[*pending_slot as usize]
                    );
                }
                Callback::PotentialPrompt {
                    delayed_slot,
                    potential_player,
                } => {
                    output += &format!(
                        "PotentialPrompt {:?} for player {potential_player}",
                        self.hands.slots[*delayed_slot as usize]
                    );
                }
                Callback::PotentialFiness {
                    delayed_slot,
                    pending_slot,
                    expected_card,
                } => {
                    output += &format!(
                        "PotentialFiness of {expected_card:?} on {:?} for {:?}",
                        self.hands.slots[*pending_slot as usize],
                        self.hands.slots[*delayed_slot as usize],
                    );
                }
                Callback::Finess {
                    delayed_slot,
                    pending_slot,
                } => {
                    output += &format!(
                        "Finess ({:?} for {:?})",
                        self.hands.slots[*pending_slot as usize],
                        self.hands.slots[*delayed_slot as usize],
                    );
                }
            }
        }
        println!("{}", output);
    }
}
