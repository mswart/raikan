//! Level-3 ordering, corrections, and discard transfers.

use super::{Line, SlotId};
use crate::{
    PositionSet, Variant,
    card_quantum::CardQuantum,
    game::{Card, Clue, Move},
};
use smallvec::SmallVec;

#[cfg(test)]
#[path = "basic_strategy_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SarcasticTransfer {
    pub card: Card,
    pub candidates: SmallVec<[SlotId; 5]>,
}

#[derive(Clone, Copy)]
pub(super) enum Fix {
    Literal(PositionSet),
    Stop(u8),
}

impl Line {
    /// Rank-1 order is shared by clue focus, actual plays, and forecasts.
    pub(super) fn first_one(&self, player: u8, positions: PositionSet) -> Option<u8> {
        positions
            .iter()
            .min_by_key(|pos| self.one_order(player, *pos))
    }

    pub(super) fn one_order(&self, player: u8, pos: u8) -> (u8, u8) {
        let slot = self.hands.slot(player, pos);
        if slot.one_chop_focus {
            (0, pos)
        } else if !slot.starting {
            (1, pos)
        } else {
            (2, u8::MAX - pos)
        }
    }

    pub(super) fn next_play_for(&self, player: u8) -> Option<u8> {
        let mut playable = PositionSet::new(self.hands.hand_sizes[player as usize]);
        for (pos, original) in self.hands.iter_hand(player) {
            let mut slot = *original;
            if slot.clued {
                slot.update_slot_attributes(&self.card_states);
            }
            if !slot.trash
                && slot.play
                && slot.delayed == 0
                && self.card_states.play_quantum.interset(slot.quantum)
            {
                playable.add(pos);
            }
        }
        let first = playable.first()?;
        // Blind finesse instructions keep their ordinary position. Clued 1s
        // retain their agreed order even when another clue queues one of them.
        let slot = self.hands.slot(player, first);
        if !slot.clued || !slot.quantum.is_rank(1) {
            return Some(first);
        }
        let mut ones = playable;
        for pos in playable.iter() {
            let slot = self.hands.slot(player, pos);
            if !slot.clued || !slot.quantum.is_rank(1) {
                ones.remove(pos);
            }
        }
        self.first_one(player, ones)
    }

    /// Classify using the receiver's pre-clue knowledge, never the actual card.
    pub(super) fn fix_kind(&self, receiver: u8, clue: Clue, touched: PositionSet) -> Option<Fix> {
        let mut informative = false;
        let mut conflicts = PositionSet::new(self.hands.hand_sizes[receiver as usize]);
        for (pos, slot) in self.hands.iter_hand(receiver) {
            let mut narrowed = *slot;
            narrowed.apply_clue(clue, touched.contains(pos));
            informative |=
                touched.contains(pos) && narrowed.quantum.hard_size() < slot.quantum.hard_size();
            if (slot.clued || slot.promised.is_some())
                && narrowed.quantum.hard_size() > 0
                && narrowed.quantum.size() == 0
                && (slot.quantum.size() > 0
                    || (touched.contains(pos)
                        && narrowed.quantum.hard_size() < slot.quantum.hard_size()))
            {
                conflicts.add(pos);
            }
        }
        if !conflicts.is_empty() {
            return Some(Fix::Literal(conflicts));
        }
        if informative || !self.newly_touched(receiver, touched).is_empty() {
            return None;
        }
        let next = self.next_play_for(receiver)?;
        if touched.contains(next)
            && self.stop_options(receiver, next).size() > 0
            && touched.iter().all(|pos| {
                let slot = self.hands.slot(receiver, pos);
                slot.clued && (slot.trash || (slot.play && slot.delayed == 0))
            })
        {
            return Some(Fix::Stop(next));
        }
        None
    }

    pub(super) fn protected_duplicate(&self, player: u8, pos: u8, card: Card) -> bool {
        (0..self.hands.num_players).any(|holder| {
            self.hands.iter_hand(holder).any(|(other_pos, other)| {
                (holder != player || other_pos != pos)
                    && other.clued
                    && !other.stopped
                    && (if holder == 0 || holder == player {
                        other.quantum.size() == 1 && other.quantum.contains(&card)
                    } else {
                        other.card == card
                    })
            })
        })
    }

    pub(super) fn stop_options(&self, player: u8, pos: u8) -> CardQuantum {
        let mut options = self.hands.slot(player, pos).quantum;
        options.reset_soft();
        let candidates = options;
        for card in candidates.iter() {
            if !self.card_states.trash_quantum.contains(&card)
                && !self.protected_duplicate(player, pos, card)
            {
                options.remove_card(&card, true);
            }
        }
        options
    }

    /// A mistakenly duplicated direct play still identifies the card. Keeping
    /// that identity lets its receiver return the copy with a sarcastic discard.
    pub(super) fn duplicated_play(&self, player: u8, pos: u8) -> Option<Card> {
        let mut playable = self.hands.slot(player, pos).quantum;
        playable.soft_limit(self.card_states.play_quantum);
        if playable.size() != 1 {
            return None;
        }
        let card = playable.iter().next()?;
        (0..self.hands.num_players)
            .filter(|holder| *holder != player)
            .any(|holder| {
                self.hands.iter_hand(holder).any(|(_, other)| {
                    other.clued
                        && !other.stopped
                        && (if holder == 0 {
                            other.quantum.size() == 1 && other.quantum.contains(&card)
                        } else {
                            other.card == card
                        })
                })
            })
            .then_some(card)
    }

    pub(super) fn clue_fixes(&self, receiver: u8, clue: Clue) -> bool {
        let mut touched = PositionSet::new(self.hands.hand_sizes[receiver as usize]);
        for (pos, slot) in self.hands.iter_hand(receiver) {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        self.fix_kind(receiver, clue, touched).is_some()
    }

    pub(super) fn receive_sarcastic(&mut self, player: u8, pos: u8, card: Card) {
        if !self.hands.slot(player, pos).clued || self.card_states.trash_quantum.contains(&card) {
            return;
        }
        let mut candidates = SmallVec::new();
        for holder in 0..self.hands.num_players {
            if holder == player {
                continue;
            }
            for (_, slot) in self.hands.iter_hand(holder) {
                if slot.clued
                    && !slot.stopped
                    && slot.quantum.contains_hard(&card)
                    && (slot.quantum.contains(&card) || slot.quantum.size() != 1)
                {
                    candidates.push(slot.id);
                }
            }
        }
        if !candidates.is_empty() {
            self.sarcastic.push(SarcasticTransfer { card, candidates });
        }
    }

    pub(super) fn refresh_sarcastic(&mut self) {
        let hands = &self.hands;
        self.sarcastic.retain_mut(|transfer| {
            if self.card_states.trash_quantum.contains(&transfer.card) {
                return false;
            }
            transfer.candidates.retain(|id| {
                hands.find(*id).is_some_and(|(holder, pos)| {
                    let slot = hands.slot(holder, pos);
                    !slot.stopped && slot.quantum.contains_hard(&transfer.card)
                })
            });
            !transfer.candidates.is_empty()
        });
        for transfer in &self.sarcastic {
            if let [id] = transfer.candidates.as_slice() {
                let (holder, pos) = self.hands.find(*id).expect("retained target");
                let slot = self.hands.slot_mut(holder, pos);
                let mut identity = CardQuantum::new(Variant {});
                identity.clear();
                identity.add_card(&transfer.card, false);
                slot.base_quantum.reset_soft();
                slot.base_quantum.soft_limit(identity);
                // The public discard corrects a duplicate's previous deduction.
                self.interpretations
                    .retain_consistent(|step| step.slot != *id || step.card == transfer.card);
            }
        }
    }

    /// A disjunction cannot identify either individual card. In particular,
    /// previous good-touch exclusions must not make every candidate trash (or
    /// make them all look like the transferred identity). Keep literal choices
    /// until another public observation singles out a target.
    pub(super) fn apply_sarcastic_ambiguity(&mut self) {
        for transfer in &self.sarcastic {
            if transfer.candidates.len() < 2 {
                continue;
            }
            for id in &transfer.candidates {
                if let Some((holder, pos)) = self.hands.find(*id) {
                    let slot = self.hands.slot_mut(holder, pos);
                    if slot.quantum.hard_size() > 1 {
                        slot.quantum.reset_soft();
                        slot.play = false;
                        slot.trash = false;
                    }
                }
            }
        }
    }

    pub(super) fn sarcastic_discard(&self) -> Option<Move> {
        self.hands.iter_hand(0).find_map(|(pos, slot)| {
            if !slot.clued || slot.promised.is_some() || slot.stopped {
                return None;
            }
            let mut identity = slot.quantum;
            if identity.size() == 0 {
                identity.reset_soft();
            }
            if identity.size() != 1 {
                return None;
            }
            let card = identity.iter().next()?;
            if self.card_states.trash_quantum.contains(&card) {
                return None;
            }
            (1..self.hands.num_players)
                .any(|player| {
                    self.hands.iter_hand(player).any(|(_, other)| {
                        other.clued
                            && !other.stopped
                            && other.card == card
                            && other.quantum.contains_hard(&card)
                    })
                })
                .then_some(Move::Discard(pos))
        })
    }
}
