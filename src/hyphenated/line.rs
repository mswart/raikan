use crate::game::{self, CardPlayState};
use crate::{
    PositionSet,
    card_quantum::{CardQuantum, Variant},
};

use super::basic_strategy::{Fix, SarcasticTransfer};
use super::card_states::CardStates;
use super::interpretation::{
    ClueRecord, HandEntry, Hypothesis, Interpretations, Step, StepKind, Tier,
};
use super::slot::{Slot, SlotId};
use super::stall::{StallKind, TurnContext};

use slog;
use smallvec::SmallVec;
use std::sync::Arc;

const CRITICAL_CARD_VALUE: i32 = 5;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct LineScore {
    discard_risks: i8,
    score: u8,
    clued: u8,
    saved: u8,
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
            + self.saved as i32 * CRITICAL_CARD_VALUE
            + self.finess as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10)
            .cmp(
                &(other.discard_risks as i32
                    + other.play as i32
                    + other.clued as i32 * 2
                    + other.saved as i32 * CRITICAL_CARD_VALUE
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
    pub(super) fn no_more_errors_than(&self, other: &Self) -> bool {
        self.errors <= other.errors
    }

    /// Finite leaf value for action search: an immediate point must not
    /// outweigh an avoidable strike or the loss of a critical card.
    pub(super) fn utility(&self) -> i32 {
        self.score as i32 * 10
            + self.discard_risks as i32
            + self.play as i32
            + self.clued as i32 * 2
            + self.saved as i32 * CRITICAL_CARD_VALUE
            + self.finess as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10
    }

    pub fn zero() -> Self {
        Self {
            score: 0,
            clued: 0,
            saved: 0,
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
            saved: 0,
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
    pub interpretations: Interpretations,
    pub(super) sarcastic: SmallVec<[SarcasticTransfer; 2]>,
    pub(super) context: TurnContext,
    clue_history: Option<Arc<ClueSnapshot>>,
    logger: slog::Logger,
}

/// Soft information before a clue, keyed by stable card IDs. Literal evidence
/// continues to accumulate independently and is never undone after a strike.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClueSnapshot {
    turn: i8,
    slots: [Slot; 20],
    previous: Option<Arc<ClueSnapshot>>,
}

impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.hands == other.hands
            && self.turn == other.turn
            && self.variant == other.variant
            && self.card_states == other.card_states
            && self.score == other.score
            && self.own_player == other.own_player
            && self.interpretations == other.interpretations
            && self.sarcastic == other.sarcastic
            && self.context == other.context
            && self.clue_history == other.clue_history
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
    next_id: SlotId,
}

impl Hands {
    fn insert_slot(&mut self, player: usize, mut slot: Slot) {
        slot.id = self.next_id;
        self.next_id += 1;
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
        for i in offset + pos as usize..offset + self.hand_sizes[player] as usize - 1 {
            self.hand_slots[i] = self.hand_slots[i + 1];
        }
        self.hand_sizes[player] -= 1;
        old_pos as usize
    }

    pub fn iter_hand(&self, player: u8) -> HandIterator<'_> {
        HandIterator {
            hands: self,
            player,
            next_pos: 0,
            back_pos: self.hand_sizes[player as usize],
        }
    }

    fn iter_hand_mut(&mut self, player: u8) -> HandMutIterator<'_> {
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

    /// Locate a card (by its stable id) in the hands: `(player, pos)`.
    pub fn find(&self, id: SlotId) -> Option<(u8, u8)> {
        for player in 0..self.num_players {
            for (pos, slot) in self.iter_hand(player) {
                if slot.id == id {
                    return Some((player, pos));
                }
            }
        }
        None
    }
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
        f.write_str("\n interpretations: ")?;
        std::fmt::Debug::fmt(&self.interpretations, f)?;
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
    steps: SmallVec<[Step; 5]>,
    tier: Tier,
    marked_cards: [PositionSet; 6],
    excluded_finesses: PositionSet,
}

impl PlayEvaluation {
    fn add_step(&mut self, line: &Line, card: game::Card, holder: u8, pos: u8, kind: StepKind) {
        let fallback = (kind == StepKind::Prompt)
            .then(|| {
                line.hands
                    .iter_hand(holder)
                    .find(|(position, slot)| {
                        !slot.clued
                            && slot.promised.is_none()
                            && !self.marked_cards[holder as usize].contains(*position)
                    })
                    .map(|(_, slot)| slot.id)
            })
            .flatten();
        self.steps.push(Step {
            card,
            holder,
            kind,
            slot: line.hands.slot(holder, pos).id,
            fallback,
            layers: 0,
            done: false,
        });
    }

    fn into_hypothesis(mut self) -> Hypothesis {
        self.steps.sort_by_key(|step| step.card.rank);
        Hypothesis {
            focus_card: self.card,
            steps: self.steps,
            tier: self.tier,
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
        excluded_finesses: PositionSet,
    ) -> Result<Self, bool> {
        let mut evaluation = Self {
            who,
            whom,
            pos,
            card,
            steps: SmallVec::new(),
            tier: Tier::Direct,
            marked_cards: [PositionSet::new(6); 6],
            excluded_finesses,
        };
        match evaluation.resolve(line, premarked, first_action) {
            Ok(_) => Ok(evaluation),
            Err(hard) => Err(!hard),
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
                .iter_hand(clued_player)
                .find(|(other_pos, slot)| {
                    !self.marked_cards[clued_player as usize].contains(*other_pos)
                                    && (slot.clued || slot.promised.is_some())
                                    && slot.card == previous_card // quantum.contains(&previous_card)
                                    && (slot.play || slot.delayed > 0)
                })
        {
            slog::debug!(
                line.logger,
                "Looking for {:?}: queued on {clued_player}'s hand: {slot:?}",
                previous_card
            );
            self.marked_cards[clued_player as usize].add(found_pos);
            self.add_step(
                line,
                previous_card,
                clued_player,
                found_pos,
                StepKind::Queued,
            );
            return Some(true);
        }
        if clued_player == self.who as u8 {
            if let Some((found_pos, slot)) =
                line.hands
                    .iter_hand(clued_player)
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
                    line.logger,
                    "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                    previous_card
                );
                self.marked_cards[clued_player as usize].add(found_pos);
                self.add_step(
                    line,
                    previous_card,
                    clued_player,
                    found_pos,
                    StepKind::Prompt,
                );
                return Some(true);
            }
            slog::debug!(
                line.logger,
                "Looking for {:?}: not found: card is invisible (clued on clue givers hand); assume duplicate clue",
                previous_card
            );
            return None;
        }
        if clued_player == self.whom as u8 && allowed_self_search == FirstAction::NonSelf() {
            if let Some((found_pos, slot)) =
                line.hands
                    .iter_hand(clued_player)
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
                    line.logger,
                    "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                    previous_card
                );
                self.marked_cards[clued_player as usize].add(found_pos);
                self.add_step(
                    line,
                    previous_card,
                    clued_player,
                    found_pos,
                    StepKind::Prompt,
                );
                return Some(true);
            }
            return Some(false);
        }
        if let Some((found_pos, slot)) =
            line.hands
                .iter_hand(clued_player)
                .find(|(other_pos, slot)| {
                    !self.marked_cards[clued_player as usize].contains(*other_pos)
                        && slot.clued
                        && slot.quantum.contains(&previous_card)
                        && !slot.play
                        && slot.delayed == 0
                })
        {
            slog::debug!(
                line.logger,
                "Looking for {:?}: clued on {clued_player}'s hand ({found_pos}): {slot:?}",
                previous_card
            );
            self.marked_cards[clued_player as usize].add(found_pos);
            self.add_step(
                line,
                previous_card,
                clued_player,
                found_pos,
                StepKind::Prompt,
            );
            return Some(true);
        }
        slog::debug!(
            line.logger,
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
        self.tier = match first_action {
            FirstAction::NonSelf() => Tier::Direct,
            FirstAction::SelfPrompt() => Tier::SelfPrompt,
            FirstAction::SelfFiness() => Tier::SelfFinesse,
        };
        self.marked_cards[self.whom] = premarked;
        self.marked_cards[self.whom].add(self.pos);
        self.add_step(line, self.card, self.whom as u8, self.pos, StepKind::Focus);
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
                        continue;
                    }
                    // An earlier clue may already reserve this connecting card,
                    // even while its other meanings remain unresolved.
                    let queued = (0..line.hands.num_players).find_map(|holder| {
                        line.hands
                            .iter_hand(holder)
                            .find(|(pos, slot)| {
                                slot.promised.is_some()
                                    && !self.marked_cards[holder as usize].contains(*pos)
                                    && slot.quantum.contains(&previous_card)
                                    && if holder == 0 {
                                        slot.quantum.size() == 1
                                    } else {
                                        slot.card == previous_card
                                    }
                            })
                            .map(|(pos, _)| (holder, pos))
                    });
                    if let Some((holder, pos)) = queued {
                        self.marked_cards[holder as usize].add(pos);
                        self.add_step(line, previous_card, holder, pos, StepKind::Queued);
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
                                let kind = if slot.play || slot.delayed > 0 {
                                    StepKind::Queued
                                } else {
                                    StepKind::Prompt
                                };
                                self.add_step(line, previous_card, player, pos, kind);
                                slog::debug!(
                                    line.logger,
                                    "Looking for {:?}: clued on {player}'s hand: {slot:?}",
                                    previous_card
                                );
                            }
                        }
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
                        if finess_player == self.who as u8
                            || self.excluded_finesses.contains(finess_player)
                        {
                            // a clue giver does not know their own hand => they can't target their finess cards
                            continue;
                        }
                        if finess_player == self.whom as u8
                            && allowed_self_search != FirstAction::SelfFiness()
                        {
                            continue;
                        }
                        let blocking_prompt = line
                            .hands
                            .iter_hand(finess_player)
                            .find(|(other_pos, slot)| {
                                !self.marked_cards[finess_player as usize].contains(*other_pos)
                                    && slot.clued
                                    && slot.quantum.contains(&previous_card)
                                    && !matches!(
                                        line.card_states[&slot.card].play,
                                        CardPlayState::Playable()
                                            | CardPlayState::CriticalPlayable()
                                    )
                            })
                            .map(|(pos, _)| pos);
                        // ensure how clued card would be played earlier:
                        let mut layers = 0;
                        let mut layer_cards: SmallVec<[game::Card; 5]> = SmallVec::new();
                        let mut layer_positions: SmallVec<[u8; 5]> = SmallVec::new();
                        for (other_pos, slot) in line.hands.iter_hand(finess_player) {
                            if slot.clued
                                || slot.promised.is_some()
                                || self.marked_cards[finess_player as usize].contains(other_pos)
                            {
                                continue;
                            }
                            if slot.card != previous_card {
                                if line.card_states.play_quantum.contains(&slot.card)
                                    && line.card_states[&slot.card].clued.is_none()
                                    && !(0..line.hands.num_players)
                                        .flat_map(|player| line.hands.iter_hand(player))
                                        .any(|(_, other)| {
                                            other.id != slot.id
                                                && (other.clued || other.promised.is_some())
                                                && other.card == slot.card
                                        })
                                    && !self.steps.iter().any(|step| {
                                        step.kind != StepKind::Focus && step.card == slot.card
                                    })
                                    && !layer_cards.contains(&slot.card)
                                {
                                    layers += 1;
                                    layer_cards.push(slot.card);
                                    layer_positions.push(other_pos);
                                    continue;
                                }
                                break;
                            }
                            if let Some(prompt_pos) = blocking_prompt {
                                // Even a mistaken clue prompts the leftmost
                                // matching clued card first. Retain that meaning
                                // so all observers can recognize a wrong prompt.
                                self.marked_cards[finess_player as usize].add(prompt_pos);
                                self.add_step(
                                    line,
                                    previous_card,
                                    finess_player,
                                    prompt_pos,
                                    StepKind::Prompt,
                                );
                                self.marked_cards[finess_player as usize].add(other_pos);
                                allowed_self_search = FirstAction::SelfFiness();
                                continue 'rank_loop;
                            }
                            slog::debug!(
                                line.logger,
                                "Looking for {:?}: found on {finess_player}'s finess position ({other_pos}): {slot:?}",
                                previous_card
                            );
                            self.marked_cards[finess_player as usize].add(other_pos);
                            for layer_pos in &layer_positions {
                                self.marked_cards[finess_player as usize].add(*layer_pos);
                            }
                            self.add_step(
                                line,
                                previous_card,
                                finess_player,
                                other_pos,
                                StepKind::Finesse,
                            );
                            self.steps.last_mut().expect("just added finesse").layers = layers;
                            allowed_self_search = FirstAction::SelfFiness();
                            continue 'rank_loop;
                        }
                    }
                    if let Some((found_pos, slot)) = line
                        .hands
                        .iter_hand(0)
                        .filter(|(other_pos, slot)| {
                            !self.marked_cards[0_usize].contains(*other_pos)
                                && (slot.clued || slot.promised.is_some())
                                && slot.quantum.contains(&previous_card)
                                && (self.who != 0 || slot.quantum.size() == 1)
                                && (slot.play || slot.delayed > 0)
                        })
                        .min_by_key(|(pos, slot)| {
                            if previous_card.rank == 1 && slot.quantum.is_rank(1) {
                                line.one_order(0, *pos)
                            } else {
                                (0, *pos)
                            }
                        })
                    {
                        slog::debug!(
                            line.logger,
                            "Looking for {:?}: queued in our hand ({found_pos}): {slot:?}",
                            previous_card
                        );
                        self.marked_cards[0_usize].add(found_pos);
                        self.add_step(line, previous_card, 0, found_pos, StepKind::Queued);
                        continue;
                    }

                    if allowed_self_search != FirstAction::NonSelf() {
                        for (other_pos, slot) in line.hands.iter_hand(0) {
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
                                line.logger,
                                "Looking for {:?}: self-prompted in our hand ({other_pos}): {slot:?} - {allowed_self_search:?}",
                                previous_card
                            );
                            self.marked_cards[0_usize].add(other_pos);
                            self.add_step(line, previous_card, 0, other_pos, StepKind::Prompt);
                            continue 'rank_loop;
                        }
                    } else {
                        for (other_pos, slot) in line.hands.iter_hand(0) {
                            if self.marked_cards[0].contains(other_pos) {
                                continue;
                            }
                            if slot.quantum.size() != 1 || !slot.quantum.contains(&previous_card) {
                                continue;
                            }
                            self.marked_cards[0_usize].add(other_pos);
                            if slot.clued {
                                slog::debug!(
                                    line.logger,
                                    "Looking for {:?}: known self-prompted in our hand ({other_pos}): {slot:?}",
                                    previous_card
                                );
                                self.add_step(line, previous_card, 0, other_pos, StepKind::Prompt);
                            } else {
                                slog::debug!(
                                    line.logger,
                                    "Looking for {:?}: finess in our hand ({other_pos}): {slot:?}",
                                    previous_card
                                );
                                self.add_step(line, previous_card, 0, other_pos, StepKind::Finesse);
                            }
                            continue 'rank_loop;
                        }
                    }

                    if allowed_self_search == FirstAction::SelfFiness()
                        && self.who > 0
                        && !self.excluded_finesses.contains(0)
                    {
                        for (other_pos, slot) in line.hands.iter_hand(0) {
                            if slot.clued || self.marked_cards[0].contains(other_pos) {
                                continue;
                            }
                            if !slot.quantum.contains(&previous_card) {
                                break;
                            }
                            slog::debug!(
                                line.logger,
                                "Looking for {:?}: found on own finess position ({other_pos}): {slot:?}",
                                previous_card
                            );
                            self.marked_cards[0_usize].add(other_pos);
                            self.add_step(line, previous_card, 0, other_pos, StepKind::Finesse);
                            continue 'rank_loop;
                        }
                    }
                    slog::debug!(line.logger, "Looking for {:?}: not found", previous_card);
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
}

impl Line {
    pub fn with_logger(num_players: u8, own_player: u8, logger: slog::Logger) -> Self {
        let variant = Variant {};
        let empty_slot = Slot {
            id: 0,
            starting: true,
            one_chop_focus: false,
            stopped: false,
            quantum: CardQuantum::new(variant),
            base_quantum: CardQuantum::new(variant),
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
                next_id: 0,
            },
            turn: -16,
            variant: Variant {},
            card_states: CardStates::new(),
            score: 0,
            own_player,
            interpretations: Interpretations::new(),
            sarcastic: SmallVec::new(),
            context: TurnContext::default(),
            clue_history: None,
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

    /// Rebuild the inferred view from clue evidence and the remaining meanings.
    fn refresh_derived(&mut self) {
        self.refresh_sarcastic();
        for player in 0..self.hands.num_players {
            for (_, slot) in self.hands.iter_hand_mut(player) {
                if slot.clued {
                    // Good touch applies to every protected card, including
                    // non-focus cards and suits blocked by a later discard.
                    slot.base_quantum
                        .soft_exclude(self.card_states.trash_quantum);
                }
                slot.quantum = slot.base_quantum;
                slot.play = false;
                slot.trash = false;
                slot.delayed = 0;
                slot.promised = None;
                slot.update_slot_attributes(&self.card_states);
            }
        }
        if self.interpretations.is_empty() {
            self.apply_sarcastic_ambiguity();
            return;
        }
        for record in &self.interpretations.records {
            let known_card = self.hands.find(record.focus).and_then(|(player, pos)| {
                (player != 0 && self.hands.slot(player, pos).card.rank != 0)
                    .then(|| self.hands.slot(player, pos).card)
            });
            if let Some(hypothesis) = record.resolved() {
                let mut earlier = 0;
                for (index, step) in hypothesis.steps.iter().enumerate().filter(|(_, s)| !s.done) {
                    let delay = earlier + step.layers;
                    earlier += 1 + step.layers;
                    if (step.holder != record.receiver
                        && known_card.is_some_and(|card| card != hypothesis.focus_card))
                        || hypothesis.steps[..index]
                            .iter()
                            .any(|previous| !previous.done && previous.slot == step.slot)
                    {
                        continue;
                    }
                    if let Some((holder, pos)) = self.hands.find(step.slot) {
                        let slot = self.hands.slot_mut(holder, pos);
                        let mut identity = CardQuantum::new(self.variant);
                        identity.clear();
                        identity.add_card(&step.card, false);
                        slot.quantum.soft_limit(identity);
                        slot.delayed = slot.delayed.max(delay);
                        if step.kind == StepKind::Finesse
                            || (holder == 0 && holder != record.receiver)
                        {
                            slot.promised = Some(record.turn);
                        }
                        slot.play = delay == 0;
                    }
                }
                continue;
            }
            let receiver_meanings = record.for_player(record.receiver, known_card);
            let observer_meanings =
                record.for_player((record.receiver + 1) % self.hands.num_players, known_card);
            for player in 0..self.hands.num_players {
                let relevant = if player == record.receiver {
                    receiver_meanings.clone()
                } else {
                    observer_meanings.clone()
                };
                if relevant.clone().next().is_none() {
                    continue;
                }
                let mut cards = [CardQuantum::new(self.variant); 5];
                for quantum in &mut cards {
                    quantum.clear();
                }
                let mut references = [0usize; 5];
                let mut finesses = [true; 5];
                let mut delays = [0u8; 5];
                let mut meanings = 0;
                for hypothesis in relevant {
                    meanings += 1;
                    let mut seen = 0u8;
                    let mut earlier = 0;
                    for step in hypothesis.pending_steps() {
                        if step.holder == player {
                            if let Some((pos, _)) = self
                                .hands
                                .iter_hand(player)
                                .find(|(_, s)| s.id == step.slot)
                            {
                                let index = pos as usize;
                                if seen & (1 << pos) == 0 {
                                    seen |= 1 << pos;
                                    references[index] += 1;
                                    cards[index].add_card(&step.card, false);
                                    finesses[index] &= step.kind == StepKind::Finesse;
                                    delays[index] = delays[index].max(earlier + step.layers);
                                }
                            }
                        }
                        earlier += 1 + step.layers;
                    }
                }
                for (pos, slot) in self.hands.iter_hand_mut(player) {
                    let index = pos as usize;
                    if references[index] == meanings {
                        slot.quantum.soft_limit(cards[index]);
                        slot.delayed = slot.delayed.max(delays[index]);
                        if finesses[index] || (player == 0 && player != record.receiver) {
                            slot.promised = Some(record.turn);
                        }
                        slot.play = delays[index] == 0;
                    } else if references[index] > 0 {
                        slot.delayed = slot.delayed.max(delays[index]);
                    }
                }
            }
        }
        for record in &self.interpretations.records {
            let known_card = self.hands.find(record.focus).and_then(|(player, pos)| {
                (player != 0 && self.hands.slot(player, pos).card.rank != 0)
                    .then(|| self.hands.slot(player, pos).card)
            });
            if !record
                .hypotheses
                .iter()
                .any(|h| h.pending_steps().any(|s| s.layers > 0))
            {
                continue;
            }
            for player in 0..self.hands.num_players {
                let relevant = record.for_player(player, known_card);
                // The same card can be a layer in one meaning and a target in
                // another. Intersect their combined obligations, preserving the
                // latest required timing across all meanings.
                let mut common: SmallVec<[(SlotId, u8, bool); 5]> = SmallVec::new();
                for (index, hypothesis) in relevant.enumerate() {
                    let mut obligations: SmallVec<[(SlotId, u8, bool); 5]> = SmallVec::new();
                    for step in hypothesis.pending_steps().filter(|s| s.holder == player) {
                        let earlier = hypothesis.pending_before(step.slot).unwrap_or(0)
                            + hypothesis
                                .pending_steps()
                                .take_while(|s| s.slot != step.slot)
                                .map(|s| s.layers)
                                .sum::<u8>();
                        for (offset, id) in self
                            .layer_slots(record, hypothesis, step)
                            .into_iter()
                            .enumerate()
                        {
                            obligations.push((id, earlier + offset as u8, true));
                        }
                        obligations.push((step.slot, earlier + step.layers, false));
                    }
                    if index == 0 {
                        common = obligations;
                    } else {
                        common.retain_mut(|(id, delay, includes_layer)| {
                            if let Some((_, other, layer)) =
                                obligations.iter().find(|(slot, _, _)| slot == id)
                            {
                                *delay = (*delay).max(*other);
                                *includes_layer |= layer;
                                true
                            } else {
                                false
                            }
                        });
                    }
                }
                for (id, delay, includes_layer) in common {
                    // Pure target constraints were already projected above.
                    if !includes_layer {
                        continue;
                    }
                    let (holder, pos) = self.hands.find(id).expect("layer slot exists");
                    let slot = self.hands.slot_mut(holder, pos);
                    slot.promised = Some(record.turn);
                    slot.delayed = slot.delayed.max(delay);
                    slot.play = slot.delayed == 0;
                    if holder != 0 {
                        slot.quantum.soft_clear();
                        slot.quantum.add_card(&slot.card, true);
                    }
                }
            }
        }
        for player in 0..self.hands.num_players {
            for (_, slot) in self.hands.iter_hand_mut(player) {
                slot.update_slot_attributes(&self.card_states);
                if slot.delayed > 0 {
                    slot.play = false;
                }
            }
        }
        self.apply_sarcastic_ambiguity();
    }

    fn refresh_clued_cards(&mut self) {
        for state in self.card_states.states_mut() {
            state.clued = if state.play == CardPlayState::Trash() {
                Some(255)
            } else {
                None
            };
            state.locked = None;
        }
        for player in 0..self.hands.num_players {
            for (_, slot) in self.hands.iter_hand(player) {
                if slot.clued && player != 0 && slot.card.rank != 0 {
                    self.card_states[&slot.card].clued.get_or_insert(player);
                }
                if (slot.clued
                    || self.interpretations.records.iter().any(|record| {
                        record.resolved().is_some_and(|hypothesis| {
                            hypothesis
                                .pending_steps()
                                .any(|step| step.slot == slot.id && step.kind == StepKind::Finesse)
                        })
                    }))
                    && slot.quantum.size() == 1
                {
                    let card = slot.quantum.iter().next().expect("singleton quantum");
                    if player == 0 || slot.card == card {
                        self.card_states[&card].clued = Some(255);
                        self.card_states[&card].locked = Some((player, slot.turn));
                    }
                }
            }
        }
    }

    fn layer_slots(
        &self,
        record: &ClueRecord,
        hypothesis: &Hypothesis,
        step: &Step,
    ) -> SmallVec<[SlotId; 5]> {
        if step.layers == 0 {
            return SmallVec::new();
        }
        let Some((holder, target)) = self.hands.find(step.slot) else {
            return SmallVec::new();
        };
        let eligible: SmallVec<[SlotId; 5]> = self
            .hands
            .iter_hand(holder)
            .filter(|(pos, slot)| {
                *pos < target
                    && !slot.clued
                    && slot.turn <= record.turn
                    && !hypothesis.steps.iter().any(|s| s.slot == slot.id)
                    && slot.promised.is_none_or(|turn| turn == record.turn)
            })
            .map(|(_, slot)| slot.id)
            .collect();
        SmallVec::from_slice(&eligible[eligible.len().saturating_sub(step.layers as usize)..])
    }

    fn matches_visible_hand(&self, hypothesis: &Hypothesis) -> bool {
        hypothesis.pending_steps().all(|step| {
            self.hands.find(step.slot).is_some_and(|(holder, pos)| {
                let slot = self.hands.slot(holder, pos);
                if holder == 0 {
                    slot.quantum.contains_hard(&step.card)
                } else {
                    slot.card == step.card
                }
            })
        })
    }

    fn true_hypothesis<'a>(&self, record: &'a ClueRecord) -> Option<&'a Hypothesis> {
        let (holder, pos) = self.hands.find(record.focus)?;
        if holder == 0 {
            return None;
        }
        let card = self.hands.slot(holder, pos).card;
        record
            .hypotheses
            .iter()
            .filter(|h| h.focus_card == card && self.matches_visible_hand(h))
            .min_by_key(|h| h.tier)
    }

    /// Whether earlier seats can disambiguate this clue before its receiver acts.
    fn resolves_before_receiver(&self, record: &ClueRecord, card: game::Card) -> bool {
        let Some(truth) = record
            .hypotheses
            .iter()
            .filter(|h| h.focus_card == card && self.matches_visible_hand(h))
            .min_by_key(|h| h.tier)
        else {
            return false;
        };
        let mut future = Interpretations {
            records: vec![record.clone()],
        };
        let mut player = (record.giver + 1) % self.hands.num_players;
        while player != record.receiver {
            if truth
                .expected_now()
                .is_none_or(|step| step.holder != player)
            {
                future.on_non_play(player);
            }
            player = (player + 1) % self.hands.num_players;
        }
        future.records.first().is_some_and(|r| {
            r.active()
                .any(|h| h.focus_card == card && self.matches_visible_hand(h))
        })
    }

    pub fn score(&self, extra_error: u8) -> LineScore {
        let mut discard_risks = 0;
        let mut clued = 0;
        let mut saved = 0;
        let mut finess = 0;
        let mut play = 0;
        let mut errors = extra_error as u16;
        if cfg!(debug_assertions) && extra_error > 0 {
            println!("error {extra_error}: initial error passed in",);
        }
        let mut finesse_slots: SmallVec<[SlotId; 20]> = SmallVec::new();
        let mut layer_slots: SmallVec<[SlotId; 20]> = SmallVec::new();
        for record in &self.interpretations.records {
            let Some((holder, _)) = self.hands.find(record.focus) else {
                continue;
            };
            if holder == 0 {
                continue;
            }
            let Some(truth) = self.true_hypothesis(record) else {
                errors += 2;
                continue;
            };
            if !self.resolves_before_receiver(record, truth.focus_card) {
                errors += 2;
            }
            let mut projected = self.card_states.clone();
            for step in truth.pending_steps() {
                if step.kind == StepKind::Finesse
                    && step.holder != 0
                    && !finesse_slots.contains(&step.slot)
                {
                    finesse_slots.push(step.slot);
                }
                let layers = self.layer_slots(record, truth, step);
                if step.holder != 0 && layers.len() != step.layers as usize {
                    errors += 2;
                }
                for id in layers {
                    let (player, pos) = self.hands.find(id).expect("layer slot exists");
                    let card = self.hands.slot(player, pos).card;
                    if !projected.play_quantum.contains(&card) {
                        errors += 2;
                    } else {
                        projected.played(&card);
                    }
                    if !layer_slots.contains(&id) {
                        layer_slots.push(id);
                    }
                }
                if !projected.play_quantum.contains(&step.card) {
                    errors += 2;
                } else {
                    projected.played(&step.card);
                }
                if step.kind == StepKind::Finesse
                    && step.holder == record.giver
                    && record.turn == self.turn
                {
                    errors += 2;
                }
            }
        }
        finess += finesse_slots.len() as u8;
        play += layer_slots.len() as u8;
        let mut bonus = 0;
        for player in 1..self.hands.num_players {
            let mut queued_actions = 0;
            let chop = self.chop(player);
            let mut discard_risk = 0;
            for (pos, slot) in self.hands.iter_hand(player).rev() {
                let card_state = self.card_states[&slot.card];
                if slot.clued {
                    clued += 1;
                    // A queued play only postpones a discard. Credit lasting
                    // protection of a critical card even while its holder has
                    // a play, using the same value as an exposed critical chop.
                    if !slot.trash
                        && matches!(
                            card_state.play,
                            CardPlayState::Critical() | CardPlayState::CriticalPlayable()
                        )
                    {
                        saved += 1;
                    }
                    if slot.play || slot.delayed > 0 {
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
                            if let Some((locked_player, turn)) = card_state.locked
                                && player == locked_player
                                && slot.turn != turn
                                && slot.quantum.size() == 0
                            {
                                duplicated_self = true;
                            }
                            if !duplicated_self && !self.protected_duplicate(player, pos, slot.card)
                            {
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
                    if chop == Some(pos) && card_state.clued.is_none() {
                        match card_state.play {
                            CardPlayState::Critical() | CardPlayState::CriticalPlayable() => {
                                discard_risk -= CRITICAL_CARD_VALUE as i8
                            }
                            CardPlayState::Playable() => discard_risk -= 2,
                            _ => {}
                        }
                    }
                    if slot.promised.is_some() {
                        queued_actions += 1;
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
                if slot.play
                    && slot.delayed == 0
                    && let Some(error) = match card_state.play {
                        CardPlayState::Playable() => None,
                        CardPlayState::CriticalPlayable() => None,
                        CardPlayState::Critical() => Some(3),
                        CardPlayState::Normal() => Some(2),
                        CardPlayState::Dead() => Some(1),
                        CardPlayState::Trash() => Some(1),
                    }
                {
                    if cfg!(debug_assertions) {
                        println!(
                            "Error {error}: {:?} card ({:?}; {}) marked as to play",
                            card_state.play, slot.card, slot.quantum
                        );
                    }
                    errors += error;
                }
                if !slot.trash
                    && (card_state.locked.unwrap_or((player, slot.turn)) == (player, slot.turn)
                        || slot.quantum.size() > 0)
                    && !slot.quantum.contains(&slot.card)
                    && !self.interpretations.records.iter().any(|record| {
                        record.focus == slot.id && self.resolves_before_receiver(record, slot.card)
                    })
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
            saved,
            finess,
            play,
            discard_risks,
            errors: errors.min(u8::MAX as u16) as u8,
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
                id: 0,
                starting: self.context.actions_taken == 0,
                one_chop_focus: false,
                stopped: false,
                quantum,
                base_quantum: quantum,
                card,
                clued: false,
                play: false,
                trash: false,
                locked: false,
                fixed: false,
                turn: self.turn,
                delayed: 0,
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
            id: 0,
            starting: self.context.actions_taken == 0,
            one_chop_focus: false,
            stopped: false,
            quantum: CardQuantum::new(self.variant),
            base_quantum: CardQuantum::new(self.variant),
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
            promised: None,
        };
        if self.turn < 0 {
            self.turn += 1;
        }
        for (card, state) in self.card_states.iter() {
            if state.tracked_count == card.suit.card_count(card.rank) {
                hand.remove_card(&card, false);
            }
        }
        self.hands.insert_slot(0, hand);
    }

    fn recover_strike(&mut self, player: u8, played_slot: Slot, hand_after: &[HandEntry]) {
        let played = played_slot.id;
        let affected: Vec<_> = self
            .interpretations
            .records
            .iter()
            .filter(|record| {
                record.hypotheses.iter().any(|h| {
                    h.pending_steps().any(|step| {
                        step.slot == played
                            || (step.holder == player
                                && self.layer_slots(record, h, step).contains(&played))
                    })
                })
            })
            .cloned()
            .collect();
        let suspected = (!played_slot.clued)
            .then(|| {
                self.interpretations
                    .records
                    .iter()
                    .filter(|record| {
                        record.giver != player
                            && record.receiver != player
                            && record.active().any(|h| {
                                h.expected_now()
                                    .is_some_and(|step| step.kind == StepKind::Focus)
                            })
                    })
                    .map(|r| r.turn)
                    .min()
            })
            .flatten();
        let Some(turn) = affected
            .iter()
            .map(|r| r.turn)
            .min()
            .or(suspected)
            .or_else(|| self.clue_history.as_ref().map(|s| s.turn))
        else {
            return;
        };

        // A wrong prompt is the only exception to withdrawing the mistaken
        // clue. Require its original finesse card to still exist and respect
        // all literal clues; never substitute the card drawn after the bomb.
        let mut recovered = Vec::new();
        for mut record in affected {
            record.hypotheses.retain_mut(|h| {
                let Some(step) = h.expected_now() else {
                    return false;
                };
                if step.slot != played || step.kind != StepKind::Prompt {
                    return false;
                }
                let Some(fallback) = step.fallback else {
                    return false;
                };
                let Some(entry) = hand_after.iter().find(|entry| entry.id == fallback) else {
                    return false;
                };
                if entry.clued
                    || !entry.quantum.contains_hard(&step.card)
                    || !self.card_states.play_quantum.contains(&step.card)
                {
                    return false;
                }
                if player != 0
                    && self
                        .hands
                        .find(fallback)
                        .is_some_and(|(p, pos)| self.hands.slot(p, pos).card != step.card)
                {
                    return false;
                }
                let step = h
                    .steps
                    .iter_mut()
                    .find(|s| !s.done && s.slot == played)
                    .unwrap();
                step.slot = fallback;
                step.kind = StepKind::Finesse;
                step.fallback = None;
                step.layers = 0;
                // In the holder's view a later finesse may have occupied the
                // same unknown slot. Shift that suffix across the original
                // hand, leaving fixed prompts and other clues' cards reserved.
                let mut used: Vec<_> = h
                    .pending_steps()
                    .filter(|s| s.kind != StepKind::Finesse || s.holder != player)
                    .map(|s| s.slot)
                    .collect();
                used.extend(
                    self.interpretations
                        .records
                        .iter()
                        .filter(|r| r.turn < turn)
                        .flat_map(|r| r.active())
                        .flat_map(|other| other.pending_steps())
                        .map(|s| s.slot),
                );
                if used.contains(&fallback) {
                    return false;
                }
                for step in h
                    .steps
                    .iter_mut()
                    .filter(|s| !s.done && s.holder == player && s.kind == StepKind::Finesse)
                {
                    if used.contains(&step.slot) {
                        let Some(next) = hand_after.iter().find(|entry| {
                            !entry.clued && entry.turn <= record.turn && !used.contains(&entry.id)
                        }) else {
                            return false;
                        };
                        if !next.quantum.contains_hard(&step.card) {
                            return false;
                        }
                        step.slot = next.id;
                    }
                    used.push(step.slot);
                }
                true
            });
            if !record.hypotheses.is_empty() {
                recovered.push(record);
            }
        }
        let snapshot =
            std::iter::successors(self.clue_history.as_deref(), |s| s.previous.as_deref())
                .take_while(|s| s.turn >= turn)
                .last();
        for player in 0..self.hands.num_players {
            for (_, slot) in self.hands.iter_hand_mut(player) {
                slot.base_quantum.reset_soft();
                if let Some(previous) =
                    snapshot.and_then(|s| s.slots.iter().find(|old| old.id == slot.id))
                {
                    slot.base_quantum.soft_limit(previous.base_quantum);
                    slot.locked = previous.locked;
                    slot.fixed = previous.fixed;
                    slot.stopped = previous.stopped;
                    slot.one_chop_focus = previous.one_chop_focus;
                } else {
                    slot.locked = false;
                    slot.fixed = false;
                    slot.stopped = false;
                    slot.one_chop_focus = false;
                }
            }
        }
        self.interpretations.records.retain(|r| r.turn < turn);
        self.interpretations.records.extend(recovered);
        while self.clue_history.as_ref().is_some_and(|s| s.turn > turn) {
            self.clue_history = self.clue_history.as_ref().unwrap().previous.clone();
        }
    }

    pub fn played(&mut self, player: usize, pos: usize, card: game::Card, successful: bool) {
        if successful {
            self.sarcastic.retain(|transfer| transfer.card != card);
        }
        self.context.actions_taken += 1;
        self.context.last_loss = (!successful).then_some((player as u8, card));
        if successful && card.rank == 5 {
            self.context.clues = (self.context.clues + 1).min(8);
        }
        self.turn += 1;
        let slot = *self.hands.slot(player as u8, pos as u8);
        let hand_after: SmallVec<[HandEntry; 5]> = self
            .hands
            .iter_hand(player as u8)
            .filter(|(position, _)| *position as usize != pos)
            .map(|(_, slot)| HandEntry {
                id: slot.id,
                clued: slot.clued,
                quantum: slot.base_quantum,
                turn: slot.turn,
                starting: slot.starting,
                one_chop_focus: slot.one_chop_focus,
            })
            .collect();
        if !successful {
            self.recover_strike(player as u8, slot, &hand_after);
        }
        self.hands.remove_slot(player, pos as u8);
        self.interpretations
            .on_play(player as u8, slot.id, card, successful, &hand_after);
        if player == 0 {
            self.track_card(card, -1, -2);
            if slot.clued && successful {
                self.card_states[&card].clued = Some(255);
            }
        } else {
            self.track_card(card, -1, player as i8);
        }
        if successful {
            self.score += 1;
            self.card_states[&card].clued = Some(255);
            self.card_states.played(&card);
            for player in 0..self.hands.num_players {
                for (_pos, slot) in self.hands.iter_hand_mut(player) {
                    if !slot.clued || slot.locked {
                        continue;
                    }
                    slot.remove_card(&card, true);
                }
            }
        } else {
            if slot.quantum.size() == 1 {
                self.card_states[&slot.quantum.iter().next().expect("asd")].clued = None;
            }
            self.card_states[&card].clued = None;
            self.card_states.discarded(&card);
        }
        self.refresh_derived();
        self.refresh_clued_cards();
    }

    pub fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.receive_sarcastic(player as u8, pos as u8, card);
        self.context.actions_taken += 1;
        let slot = self.hands.slot(player as u8, pos as u8);
        if !slot.clued && slot.promised.is_none() && !slot.trash {
            self.context.early_game = false;
        }
        self.context.clues = (self.context.clues + 1).min(8);
        self.context.last_loss = Some((player as u8, card));
        self.turn += 1;
        self.interpretations.on_non_play(player as u8);
        self.interpretations
            .on_discard(self.hands.slot(player as u8, pos as u8).id);
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
        self.refresh_derived();
        self.refresh_clued_cards();
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
                            slot.remove_card(&card, false);
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
        self.clue_history = Some(Arc::new(ClueSnapshot {
            turn: self.turn + 1,
            slots: self.hands.slots,
            previous: self.clue_history.clone(),
        }));
        let old_chop = self.chop(whom as u8);
        let newly_clued = self.newly_touched(whom as u8, touched);
        let fix = self.fix_kind(whom as u8, clue, touched);
        let mut focus = self
            .clue_focus(whom as u8, touched)
            .expect("empty clues are not implemented");
        if clue == game::Clue::Rank(1) {
            focus = old_chop
                .filter(|pos| newly_clued.contains(*pos))
                .or_else(|| self.first_one(whom as u8, touched))
                .expect("nonempty rank-1 clue");
            if fix.is_none() && old_chop == Some(focus) && newly_clued.contains(focus) {
                self.hands.slot_mut(whom as u8, focus).one_chop_focus = true;
            }
        }
        let mut save_options = CardQuantum::new(self.variant);
        save_options.clear();
        if old_chop == Some(focus) && newly_clued.contains(focus) {
            for card in self.hands.slot(whom as u8, focus).quantum.iter() {
                if card.affected(clue) && self.can_save(who as u8, whom as u8, clue, card) {
                    save_options.add_card(&card, false);
                }
            }
        }
        let stall = if fix.is_some() {
            None
        } else {
            self.stall_kind(who as u8, whom as u8, clue, touched)
        };
        let tempo_focus = (stall == Some(StallKind::Tempo))
            .then(|| self.stall_focus(whom as u8, touched))
            .flatten();
        self.context.actions_taken += 1;
        if stall == Some(StallKind::Five) && self.context.early_game {
            self.context.early_five_stalled = true;
        }
        self.context.clues = self.context.clues.saturating_sub(1);
        self.context.last_loss = None;
        let skipped_finesse = who == 0
            && self
                .hands
                .iter_hand(0)
                .any(|(_, slot)| slot.promised.is_some() && slot.play && !slot.clued);
        self.interpretations.on_non_play(who as u8);
        let touched_ids: SmallVec<[SlotId; 5]> = touched
            .iter()
            .map(|pos| self.hands.slot(whom as u8, pos).id)
            .collect();
        self.interpretations.on_clue(&touched_ids);
        self.turn += 1;
        self.refresh_derived();

        let mut error = if skipped_finesse { 2 } else { 0 };
        for pos in 0..self.hands.hand_sizes[whom] {
            let slot = self.hands.slot_mut(whom as u8, pos);
            let old_size = slot.quantum.size();
            let previsous_first_quantum_card = slot.quantum.iter().next();
            slot.apply_clue(clue, touched.contains(pos));
            if matches!(fix, Some(Fix::Literal(conflicts)) if conflicts.contains(pos)) {
                slot.quantum.reset_soft();
                slot.base_quantum.reset_soft();
                if old_size == 1 {
                    self.card_states[&previsous_first_quantum_card.expect("size was tested")]
                        .clued = None;
                }
                slot.fixed = true;
                slot.locked = false;
                slot.stopped = false;
            }
            if touched.contains(pos) {
                slot.clued = true;
            }
            if slot.quantum.size() == 1 {
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
                    // Knowing an unclued card's identity does not promise it
                    // will be kept. Good touch only reserves protected cards.
                    if other_pos != pos
                        && self.hands.slot(whom as u8, pos).clued
                        && self.hands.slot(whom as u8, other_pos).clued
                    {
                        self.hands
                            .slot_mut(whom as u8, other_pos)
                            .remove_card(&card, true);
                    }
                }
            }
        }
        for record in &mut self.interpretations.records {
            for hypothesis in &mut record.hypotheses {
                let targets: SmallVec<[SlotId; 5]> =
                    hypothesis.pending_steps().map(|s| s.slot).collect();
                for step in hypothesis
                    .steps
                    .iter_mut()
                    .filter(|s| !s.done && s.kind == StepKind::Finesse && s.holder == whom as u8)
                {
                    if let Some((holder, position)) = self.hands.find(step.slot) {
                        if !self
                            .hands
                            .slot(holder, position)
                            .quantum
                            .contains_hard(&step.card)
                        {
                            if let Some((_, slot)) =
                                self.hands.iter_hand(holder).find(|(pos, slot)| {
                                    touched.contains(*pos)
                                        && !targets.contains(&slot.id)
                                        && slot.quantum.contains_hard(&step.card)
                                })
                            {
                                step.slot = slot.id;
                                step.kind = StepKind::Prompt;
                                step.layers = 0;
                            }
                        }
                    }
                }
            }
        }
        let hands = &self.hands;
        self.interpretations.retain_consistent(|step| {
            hands.find(step.slot).is_some_and(|(player, pos)| {
                hands.slot(player, pos).quantum.contains_hard(&step.card)
            })
        });
        if let Some(fix) = fix {
            if let Fix::Literal(conflicts) = fix {
                let corrected: SmallVec<[SlotId; 5]> = conflicts
                    .iter()
                    .map(|pos| self.hands.slot(whom as u8, pos).id)
                    .collect();
                self.interpretations
                    .retain_consistent(|step| !corrected.contains(&step.slot));
            }
            if let Fix::Stop(pos) = fix {
                let options = self.stop_options(whom as u8, pos);
                let slot = self.hands.slot_mut(whom as u8, pos);
                slot.stopped = true;
                slot.fixed = true;
                slot.base_quantum = options;
                let id = slot.id;
                self.interpretations
                    .retain_consistent(|step| step.slot != id);
            }
            // A correction never promises ancillary cards as new plays. Keep
            // their literal information and apply only ordinary good touch.
            for pos in newly_clued.iter() {
                let slot = self.hands.slot_mut(whom as u8, pos);
                for (card, state) in self.card_states.iter_clued() {
                    if state.clued != Some(whom as u8)
                        && state.locked != Some((whom as u8, slot.turn))
                    {
                        slot.remove_card(&card, true);
                    }
                }
            }
            self.refresh_derived();
            self.refresh_clued_cards();
            return error;
        }
        self.refresh_derived();
        if stall.is_some() {
            // Tempo promises an immediate play of the focus only. Other stalls
            // convey literal information and good touch without a new chain.
            if let Some(focus) = tempo_focus {
                let slot = self.hands.slot_mut(whom as u8, focus);
                slot.base_quantum.soft_limit(self.card_states.play_quantum);
            }
            for pos in newly_clued.iter() {
                let slot = self.hands.slot_mut(whom as u8, pos);
                if !slot.locked {
                    for (card, state) in self.card_states.iter_clued() {
                        if state.clued != Some(whom as u8)
                            && state.locked != Some((whom as u8, slot.turn))
                        {
                            slot.remove_card(&card, true);
                        }
                    }
                }
                if whom != 0 {
                    let card = slot.card;
                    for player in 0..self.hands.num_players {
                        if player == who as u8 || player == whom as u8 {
                            continue;
                        }
                        for (_, slot) in self.hands.iter_hand_mut(player) {
                            if slot.clued {
                                slot.remove_card(&card, true);
                            }
                        }
                    }
                }
            }
            self.refresh_derived();
            self.refresh_clued_cards();
            return error;
        }
        if newly_clued.is_empty() {
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
            slot.play = true;
            error += self.resolve_play_clue(who, whom, focus, touched);
            self.refresh_derived();
            self.refresh_clued_cards();
            return error;
        }

        // somebody else was clued -> remember which cards are clued
        for pos in newly_clued.iter_first(focus) {
            let duplicate =
                (pos == focus && save_options.size() == 0 && matches!(clue, game::Clue::Rank(_)))
                    .then(|| self.duplicated_play(whom as u8, pos))
                    .flatten();
            let slot = self.hands.slot_mut(whom as u8, pos);
            if !slot.locked {
                for (card, state) in self.card_states.iter_clued() {
                    // Identifying a resolved finesse must preserve its own
                    // assignment; good touch excludes it from other slots.
                    if state.clued != Some(whom as u8)
                        && state.locked != Some((whom as u8, slot.turn))
                    {
                        slot.remove_card(&card, true);
                    }
                }
            }
            if pos == focus {
                if let Some(card) = duplicate {
                    slot.base_quantum.soft_clear();
                    slot.base_quantum.add_card(&card, true);
                    slot.quantum = slot.base_quantum;
                    // Recover from duplicates, but do not reward giving them.
                    error += 1;
                } else if save_options.interset(slot.quantum)
                    && (whom == 0 || !(slot.card.rank == 5 && clue != game::Clue::Rank(5)))
                {
                    for potential_card in slot.quantum.clone().iter() {
                        if !save_options.contains(&potential_card)
                            && !self.card_states.play_quantum.contains(&potential_card)
                        {
                            slot.remove_card(&potential_card, true);
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
                                        slot.remove_card(&card, true);
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
                            slot.remove_card(&card, true);
                        }
                    }
                }
            }
        }
        self.refresh_derived();
        self.refresh_clued_cards();
        error
    }

    /// Reorient the information available to the giver for a conservative
    /// receiver check. Our unknown cards stay unknown in the temporary view.
    pub(super) fn receiver_view(&self, receiver: u8) -> Self {
        let mut view = self.clone();
        let players = self.hands.num_players;
        let rotate = |player: u8| (players + player - receiver) % players;
        view.context.last_loss = self
            .context
            .last_loss
            .map(|(player, card)| (rotate(player), card));
        for player in 0..players {
            let old = (receiver + player) % players;
            view.hands.hand_sizes[player as usize] = self.hands.hand_sizes[old as usize];
            for pos in 0..self.hands.max_hand_size {
                view.hands.hand_slots[(player * self.hands.max_hand_size + pos) as usize] =
                    self.hands.hand_slots[(old * self.hands.max_hand_size + pos) as usize];
            }
        }
        for (_, slot) in view.hands.iter_hand_mut(0) {
            slot.card.rank = 0;
        }
        for (_, slot) in view.hands.iter_hand_mut(rotate(0)) {
            if slot.quantum.size() == 1 {
                slot.card = slot.quantum.iter().next().expect("singleton");
            }
        }
        for state in view.card_states.states_mut() {
            if let Some(holder) = state.clued {
                if holder != 255 {
                    state.clued = if holder == receiver {
                        None
                    } else {
                        Some(rotate(holder))
                    };
                }
            }
            for place in &mut state.tracked_places {
                if *place == receiver as i8 {
                    *place = -2;
                    state.tracked_count = state.tracked_count.saturating_sub(1);
                } else if *place >= 0 {
                    *place = rotate(*place as u8) as i8;
                }
            }
            state
                .tracked_places
                .sort_unstable_by_key(|place| *place == -2);
            state.locked = state.locked.map(|(holder, turn)| (rotate(holder), turn));
        }
        for record in &mut view.interpretations.records {
            record.giver = rotate(record.giver);
            record.receiver = rotate(record.receiver);
            for hypothesis in &mut record.hypotheses {
                for step in &mut hypothesis.steps {
                    step.holder = rotate(step.holder);
                }
            }
        }
        view.own_player = (self.own_player + receiver) % players;
        view
    }

    /// Recompute a seat's decisions after a simulated public card action.
    /// Foreign holders may have had truth-specific deductions in our view.
    pub(super) fn decision_view(&self, player: u8) -> Self {
        let mut view = self.receiver_view(player);
        view.refresh_derived();
        view.refresh_clued_cards();
        view
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
        let receiver_check = (who == 0).then(|| self.receiver_view(whom as u8));
        // Observers can condition connecting cards on the visible focus. Use a
        // temporary view so these assumptions stay retractable with the record.
        let mut view = self.clone();
        let observed = if whom > 0 {
            let card = self.hands.slot(whom as u8, pos).card;
            PlayEvaluation::test(
                self,
                card,
                who,
                whom,
                pos,
                premarked,
                FirstAction::SelfFiness(),
                PositionSet::new(6),
            )
            .ok()
            .map(PlayEvaluation::into_hypothesis)
        } else {
            None
        };
        if let Some(hypothesis) = &observed {
            for step in hypothesis
                .pending_steps()
                .filter(|s| s.holder != whom as u8)
            {
                let (holder, position) = view
                    .hands
                    .find(step.slot)
                    .expect("new hypothesis slot exists");
                let slot = view.hands.slot_mut(holder, position);
                slot.quantum.soft_clear();
                slot.quantum.add_card(&step.card, true);
                slot.update_slot_attributes(&view.card_states);
                if step.kind != StepKind::Finesse {
                    view.hands
                        .slot_mut(whom as u8, pos)
                        .quantum
                        .remove_card(&step.card, true);
                }
            }
        }
        let candidates = view.hands.slot(whom as u8, pos).quantum;
        let mut hypotheses = Vec::new();
        for first_action in [
            FirstAction::NonSelf(),
            FirstAction::SelfPrompt(),
            FirstAction::SelfFiness(),
        ] {
            for potential_card in candidates.iter() {
                let mut exclusions: SmallVec<[PositionSet; 8]> =
                    smallvec::smallvec![PositionSet::new(6)];
                let mut index = 0;
                while index < exclusions.len() {
                    let excluded = exclusions[index];
                    index += 1;
                    if let Ok(evaluation) = PlayEvaluation::test(
                        &mut view,
                        potential_card,
                        who,
                        whom,
                        pos,
                        premarked,
                        first_action,
                        excluded,
                    ) {
                        let hypothesis = evaluation.into_hypothesis();
                        for step in hypothesis
                            .pending_steps()
                            .filter(|s| s.kind == StepKind::Finesse && s.holder != whom as u8)
                        {
                            let mut next = excluded;
                            next.add(step.holder);
                            if !exclusions.contains(&next) {
                                exclusions.push(next);
                            }
                        }
                        if !hypotheses
                            .iter()
                            .any(|h: &Hypothesis| h.steps == hypothesis.steps)
                        {
                            hypotheses.push(hypothesis);
                        }
                    }
                }
            }
        }
        if let Some(observed) = observed {
            if !hypotheses.iter().any(|h| h.steps == observed.steps) {
                hypotheses.push(observed);
            }
        } else if whom > 0 {
            error += 2;
        }
        if hypotheses.is_empty() {
            error += 2;
        }
        self.interpretations.add(ClueRecord {
            turn: self.turn,
            giver: who as u8,
            receiver: whom as u8,
            focus: self.hands.slot(whom as u8, pos).id,
            hypotheses,
        });
        self.refresh_derived();
        if let Some(mut receiver_view) = receiver_check
            && !self.hands.slot(whom as u8, pos).trash
        {
            let giver = (self.hands.num_players - whom as u8) % self.hands.num_players;
            receiver_view.resolve_play_clue(giver as usize, 0, pos, premarked);
            let card = self.hands.slot(whom as u8, pos).card;
            if !receiver_view.interpretations.records.iter().any(|record| {
                record.turn == self.turn
                    && record.receiver == 0
                    && receiver_view.resolves_before_receiver(record, card)
            }) {
                error += 2;
            }
        }

        error
    }

    /// Check the next turn of each other player using the information that
    /// player can see. A valid-looking chain can still cause an immediate bomb
    /// when another holder interprets the same clue differently.
    fn first_round_is_safe(&self, initial: &[Line]) -> bool {
        let players = self.hands.num_players;
        let mut views = initial.to_vec();
        for player in 1..players {
            if let Some(game::Move::Discard(pos)) = views[player as usize]
                .sarcastic_discard()
                .filter(|_| views[player as usize].context.clues < 8)
            {
                let card = self.hands.slot(player, pos).card;
                for (observer, view) in views.iter_mut().enumerate() {
                    view.discarded(
                        ((players + player - observer as u8) % players) as usize,
                        pos as usize,
                        card,
                    );
                }
                continue;
            }
            if let Some(game::Move::Play(pos)) = views[player as usize].play() {
                let card = self.hands.slot(player, pos).card;
                if !views[0].card_states.play_quantum.contains(&card) {
                    return false;
                }
                for (observer, view) in views.iter_mut().enumerate() {
                    view.played(
                        ((players + player - observer as u8) % players) as usize,
                        pos as usize,
                        card,
                        true,
                    );
                }
            } else {
                for (observer, view) in views.iter_mut().enumerate() {
                    if view
                        .interpretations
                        .non_play_changed((players + player - observer as u8) % players)
                    {
                        view.refresh_derived();
                        view.refresh_clued_cards();
                    }
                }
            }
        }
        self.interpretations
            .records
            .iter()
            .filter(|record| record.turn == self.turn)
            .all(|record| {
                let Some((holder, pos)) = self.hands.find(record.focus) else {
                    return true;
                };
                let card = self.hands.slot(holder, pos).card;
                views[0].hands.find(record.focus).is_none()
                    || matches!(views[0].card_states[&card].play, CardPlayState::Trash())
                    || views[0].interpretations.records.iter().any(|remaining| {
                        remaining.focus == record.focus
                            && remaining.turn == record.turn
                            && remaining
                                .hypotheses
                                .iter()
                                .any(|h| h.focus_card == card && views[0].matches_visible_hand(h))
                    })
            })
    }

    pub fn clue(&mut self, whom: usize, clue: game::Clue) -> Option<LineScore> {
        self.clue_views(whom, clue, |_, _| true)
            .map(|(score, _)| score)
    }

    pub(super) fn clue_views(
        &mut self,
        whom: usize,
        clue: game::Clue,
        accept: impl FnOnce(&Line, &LineScore) -> bool,
    ) -> Option<(LineScore, Vec<Line>)> {
        let mut touched = PositionSet::new(self.hands.hand_sizes[whom]);
        for (pos, slot) in self.hands.iter_hand_mut(whom as u8) {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        if touched.is_empty() {
            return None;
        }
        let before = self.clone();
        let error = self.clued(0, whom, clue, touched);
        let mut score = self.score(error);
        // Safety checks and rollout can only add errors or danger. Callers can
        // reject this optimistic bound without constructing every player view.
        if !accept(self, &score) {
            return None;
        }
        let players = self.hands.num_players;
        let mut views = Vec::with_capacity(players as usize);
        views.push(self.clone());
        for player in 1..players {
            let mut view = before.receiver_view(player);
            view.clued(
                (players - player) as usize,
                ((players + whom as u8 - player) % players) as usize,
                clue,
                touched,
            );
            views.push(view);
        }
        if !self.first_round_is_safe(&views) {
            score.errors = score.errors.saturating_add(2);
        }
        Some((score, views))
    }

    pub fn discard(&self) -> game::Move {
        if let Some(action) = self.sarcastic_discard() {
            return action;
        }
        // look for trash
        let mut chop = -1;
        for (pos, slot) in self.hands.iter_hand(0) {
            if slot.trash {
                return game::Move::Discard(pos);
            }
            if !slot.clued && slot.promised.is_none() {
                chop = pos as i8;
            }
        }

        if chop >= 0 {
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for (pos, slot) in self.hands.iter_hand(0) {
                if slot.quantum.is_rank(*rank) {
                    return game::Move::Discard(pos);
                }
            }
        }
        // nothing clear found; drop newest card
        game::Move::Discard(0)
    }

    pub fn play(&mut self) -> Option<game::Move> {
        self.next_play()
    }

    pub(super) fn next_play(&self) -> Option<game::Move> {
        self.next_play_for(0).map(game::Move::Play)
    }

    pub fn print_interpretations(&self, prefix: &str) {
        for record in &self.interpretations.records {
            println!("{prefix}{record:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: u8) -> game::Card {
        game::Card {
            suit: game::Suit::Red(),
            rank,
        }
    }

    fn step(line: &Line, holder: u8, pos: u8, rank: u8, kind: StepKind) -> Step {
        Step {
            card: card(rank),
            holder,
            kind,
            slot: line.hands.slot(holder, pos).id,
            fallback: None,
            layers: 0,
            done: false,
        }
    }

    #[test]
    fn derived_constraints_are_conditional_and_retractable() {
        let mut line = Line::new(4, 0);
        line.own_drawn();
        line.own_drawn();
        line.drawn(1, card(1));
        line.hands
            .slot_mut(0, 1)
            .apply_clue(game::Clue::Rank(2), true);
        line.hands.slot_mut(0, 1).clued = true;
        let focus = step(&line, 0, 1, 2, StepKind::Focus);
        let foreign = step(&line, 1, 0, 1, StepKind::Finesse);
        let own = step(&line, 0, 0, 1, StepKind::Finesse);
        line.interpretations.add(ClueRecord {
            turn: 1,
            giver: 2,
            receiver: 0,
            focus: focus.slot,
            hypotheses: vec![
                Hypothesis {
                    focus_card: card(2),
                    steps: smallvec::smallvec![foreign, focus],
                    tier: Tier::Direct,
                },
                Hypothesis {
                    focus_card: card(2),
                    steps: smallvec::smallvec![own, focus],
                    tier: Tier::Direct,
                },
            ],
        });
        line.refresh_derived();
        assert!(line.hands.slot(0, 0).promised.is_none());
        assert_eq!(
            line.hands.slot(0, 0).quantum.size(),
            25,
            "conditional slot stays unconstrained"
        );
        assert_eq!(line.hands.slot(0, 1).delayed, 1);
        assert!(!line.hands.slot(0, 1).play);

        line.interpretations.on_non_play(1);
        line.refresh_derived();
        assert_eq!(line.hands.slot(0, 0).quantum.to_vec(), vec![card(1)]);
        assert_eq!(line.hands.slot(0, 0).promised, Some(1));
        assert!(line.hands.slot(0, 0).play);
        assert_eq!(line.hands.slot(0, 1).quantum.to_vec(), vec![card(2)]);

        line.interpretations.on_discard(own.slot);
        line.refresh_derived();
        assert!(line.hands.slot(0, 0).promised.is_none());
        assert_eq!(line.hands.slot(0, 0).quantum.size(), 25);
        assert_eq!(
            line.hands.slot(0, 1).quantum.size(),
            5,
            "literal rank clue survives retraction"
        );
        assert_eq!(line.hands.slot(0, 1).delayed, 0);
    }
    fn layered_line(layer_rank: u8) -> Line {
        use game::Suit::*;
        let mut line = Line::new(4, 0);
        for _ in 0..4 {
            line.own_drawn();
        }
        let hands = [
            [(Red(), 3), (Purple(), 4), (Blue(), 4), (Yellow(), 4)],
            [
                (Green(), layer_rank),
                (Red(), 2),
                (Blue(), 4),
                (Yellow(), 4),
            ],
            [(Purple(), 1), (Green(), 4), (Blue(), 5), (Yellow(), 5)],
        ];
        for (player, cards) in hands.iter().enumerate() {
            for &(suit, rank) in cards.iter().rev() {
                line.drawn(player + 1, game::Card { suit, rank });
            }
        }
        line.card_states.played(&card(1));
        line.refresh_derived();
        line.refresh_clued_cards();
        line
    }

    #[test]
    fn conditional_layer_waits_for_disambiguation() {
        let mut line = layered_line(1);
        let focus = step(&line, 1, 0, 3, StepKind::Focus);
        let mut layered = step(&line, 2, 1, 2, StepKind::Finesse);
        layered.layers = 1;
        let alternative = step(&line, 3, 0, 2, StepKind::Finesse);
        line.interpretations.add(ClueRecord {
            turn: line.turn,
            giver: 0,
            receiver: 1,
            focus: focus.slot,
            hypotheses: vec![
                Hypothesis {
                    focus_card: card(3),
                    steps: smallvec::smallvec![layered, focus],
                    tier: Tier::Direct,
                },
                Hypothesis {
                    focus_card: card(3),
                    steps: smallvec::smallvec![alternative, focus],
                    tier: Tier::Direct,
                },
            ],
        });
        line.refresh_derived();
        assert!(line.hands.slot(2, 0).promised.is_none());
        assert!(!line.hands.slot(2, 0).play);

        line.interpretations.on_non_play(3);
        line.refresh_derived();
        assert!(line.hands.slot(2, 0).promised.is_some());
        assert!(line.hands.slot(2, 0).play);
    }

    #[test]
    fn mixed_layer_and_target_obligations_require_consensus_without_guessing_our_card() {
        for reverse in [false, true] {
            let mut line = layered_line(1);
            let focus = step(&line, 1, 0, 3, StepKind::Focus);
            let direct = step(&line, 0, 0, 2, StepKind::Finesse);
            let mut layered = step(&line, 0, 1, 2, StepKind::Finesse);
            layered.layers = 1;
            let mut hypotheses = vec![
                Hypothesis {
                    focus_card: card(3),
                    steps: smallvec::smallvec![direct, focus],
                    tier: Tier::Direct,
                },
                Hypothesis {
                    focus_card: card(3),
                    steps: smallvec::smallvec![layered, focus],
                    tier: Tier::Direct,
                },
            ];
            if reverse {
                hypotheses.reverse();
            }
            line.interpretations.add(ClueRecord {
                turn: line.turn,
                giver: 3,
                receiver: 1,
                focus: focus.slot,
                hypotheses,
            });
            line.refresh_derived();
            assert_eq!(line.play(), Some(game::Move::Play(0)));
            assert!(line.hands.slot(0, 0).promised.is_some());
            assert!(
                line.hands.slot(0, 0).quantum.size() > 1,
                "a hidden layer need not be r2"
            );
            assert!(line.hands.slot(0, 1).promised.is_none());

            // An alternative that does not require our card removes the common
            // obligation, without changing the literal information in our hand.
            let alternative = step(&line, 2, 1, 2, StepKind::Finesse);
            line.interpretations.records[0].hypotheses.push(Hypothesis {
                focus_card: card(3),
                steps: smallvec::smallvec![alternative, focus],
                tier: Tier::Direct,
            });
            line.refresh_derived();
            assert!(line.hands.slot(0, 0).promised.is_none());
            assert_eq!(line.play(), None);
        }
    }

    #[test]
    fn giving_layered_finesse_counts_target_and_layer_separately() {
        let line = layered_line(1);
        let layered = line.clone().clue(1, game::Clue::Rank(3)).unwrap();
        let direct = line
            .clone()
            .clue(2, game::Clue::Color(game::ClueColor::Green()))
            .unwrap();
        assert!(!layered.has_errors(), "{layered:?}");
        assert_eq!(
            layered.finess, 1,
            "g1 is a layer; only r2 is the finesse target"
        );
        assert!(
            layered.play >= 2,
            "both the layer and focused card yield plays"
        );
        assert!(layered > direct);
    }

    #[test]
    fn reject_unplayable_layers_and_occam_mismatch() {
        assert!(
            layered_line(4)
                .clue(1, game::Clue::Rank(3))
                .unwrap()
                .has_errors()
        );
        // Red could simply mean r2. A receiver would play this r3 too early.
        assert!(
            layered_line(1)
                .clue(1, game::Clue::Color(game::ClueColor::Red()))
                .unwrap()
                .has_errors()
        );
    }

    #[test]
    fn reject_layer_duplicating_an_already_queued_card() {
        let mut line = layered_line(1);
        let slot = line.hands.slot_mut(3, 0);
        slot.card = game::Card {
            suit: game::Suit::Green(),
            rank: 1,
        };
        slot.apply_clue(game::Clue::Rank(1), true);
        slot.clued = true;
        line.refresh_derived();
        line.refresh_clued_cards();
        assert!(line.clue(1, game::Clue::Rank(3)).unwrap().has_errors());
    }

    #[test]
    fn reject_clue_that_abandons_our_due_finesse() {
        let mut line = layered_line(1);
        let connecting = step(&line, 0, 0, 2, StepKind::Finesse);
        let focus = step(&line, 1, 0, 3, StepKind::Focus);
        line.interpretations.add(ClueRecord {
            turn: 0,
            giver: 3,
            receiver: 1,
            focus: focus.slot,
            hypotheses: vec![Hypothesis {
                focus_card: card(3),
                steps: smallvec::smallvec![connecting, focus],
                tier: Tier::Direct,
            }],
        });
        line.refresh_derived();
        assert!(line.hands.slot(0, 0).play);
        assert!(line.clue(3, game::Clue::Rank(5)).unwrap().has_errors());
    }
}
