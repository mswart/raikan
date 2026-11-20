//! Situation classifiers and the level-1/3 rule helpers: chop, focus, saves, 1s order,
//! fixes, stop clues, duplicated plays and sarcastic discards.

use smallvec::SmallVec;

use super::belief::Belief;
use super::profile::Rule;
use super::view::{PlayState, SarcasticTransfer, View};
use crate::domain::{CardId, CardSet, ClueId, Identity, Seat};
use crate::variants::ClueValue;

/// A fix clue, classified with the receiver's knowledge before the clue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// These cards' inferences contradict the literal clue.
    Literal(CardSet),
    /// Stop the receiver's next queued play.
    Stop(CardId),
}

impl View {
    pub fn is_rank_clue(&self, clue: ClueId, rank: u8) -> bool {
        self.variant.clue_value(clue) == ClueValue::Rank(rank)
    }

    /// Chop: the oldest card that is neither clued, promised nor chop moved. Finessed and
    /// chop-moved cards carry an implicit clue and are not on chop.
    pub fn chop(&self, seat: Seat) -> Option<CardId> {
        self.state
            .hand(seat)
            .iter()
            .copied()
            .find(|&c| !self.note(c).touched())
    }

    pub fn newly_touched(&self, receiver: Seat, touched: CardSet) -> CardSet {
        let _ = receiver;
        touched
            .iter()
            .filter(|&c| !self.note(c).clued && self.note(c).promised.is_none())
            .collect()
    }

    /// Focus: chop if newly touched, otherwise the newest new card; a clue without new
    /// cards focuses the newest re-touched card. Chop-moved cards count as already touched,
    /// unless the clue touches no other new card: then the newest chop-moved card is the
    /// focus (level 4, chop moves and new clues).
    pub fn clue_focus(&self, receiver: Seat, touched: CardSet) -> Option<CardId> {
        let new = self.newly_touched(receiver, touched);
        let fresh: CardSet = new.iter().filter(|&c| !self.note(c).chop_moved).collect();
        if !fresh.is_empty() {
            self.chop(receiver)
                .filter(|&c| fresh.contains(c))
                .or_else(|| fresh.max())
        } else if !new.is_empty() {
            new.max()
        } else {
            touched.max()
        }
    }

    /// Whether a chop card of `identity` may be saved with `clue`. Only copies visible to
    /// giver and receiver can rule out a 2 save; a duplicate on another chop is the
    /// double-chop exception.
    pub fn can_save(&self, giver: Seat, receiver: Seat, clue: ClueId, identity: Identity) -> bool {
        let rank = self.variant.rank_of(identity);
        match self.play_state(identity) {
            PlayState::Critical if rank == self.variant.num_ranks => {
                self.rule(Rule::Save5) && self.is_rank_clue(clue, rank)
            }
            PlayState::Critical => self.rule(Rule::SaveCritical),
            PlayState::Normal
                if rank == 2 && self.is_rank_clue(clue, 2) && self.rule(Rule::Save2) =>
            {
                self.state
                    .seats()
                    .filter(|&seat| seat != giver && seat != receiver)
                    .all(|seat| {
                        let chop = self.chop(seat);
                        self.state
                            .hand(seat)
                            .iter()
                            .all(|&c| self.identity(c) != Some(identity) || chop == Some(c))
                    })
            }
            _ => false,
        }
    }

    // ----- 1s order -----

    fn position(&self, seat: Seat, card: CardId) -> u8 {
        self.newest_first(seat)
            .find(|&(_, c)| c == card)
            .map(|(p, _)| p)
            .unwrap_or(u8::MAX)
    }

    /// Ordering key for playing 1s: a chop-focused 1 first, then fresh 1s newest first, then
    /// starting-hand 1s oldest first.
    pub fn one_order(&self, seat: Seat, card: CardId) -> (u8, u8) {
        let pos = self.position(seat, card);
        if !self.rule(Rule::OnesOrder) {
            return (0, pos);
        }
        let note = self.note(card);
        if note.one_chop_focus {
            (0, pos)
        } else if !note.starting {
            (1, pos)
        } else {
            (2, u8::MAX - pos)
        }
    }

    pub fn first_one(&self, seat: Seat, cards: CardSet) -> Option<CardId> {
        cards.iter().min_by_key(|&c| self.one_order(seat, c))
    }

    pub(super) fn is_rank_belief(&self, belief: &Belief, rank: u8) -> bool {
        belief.soft.is_subset(self.variant.rank_identities(rank))
    }

    /// The card `seat` will play next according to this view.
    pub fn next_play_for(&self, seat: Seat) -> Option<CardId> {
        let playable_set = self.playable_set();
        let trash_set = self.trash_set();
        let mut playable: SmallVec<[CardId; 6]> = SmallVec::new();
        for (_, card) in self.newest_first(seat) {
            let mut note = *self.note(card);
            if note.clued {
                note.update_attributes(playable_set, trash_set);
            }
            if !note.trash && note.play && note.delayed == 0 && note.belief.intersects(playable_set)
            {
                playable.push(card);
            }
        }
        // Finessed (blind) cards come first: they prove the finesse, so teammates waiting on
        // it learn where the connection is, before any clued known play.
        if let Some(&blind) = playable.iter().find(|&&c| {
            let note = self.note(c);
            !note.clued && note.promised.is_some()
        }) {
            return Some(blind);
        }
        let first = *playable.first()?;
        // Clued 1s retain their agreed order even when another clue queues one of them.
        let note = self.note(first);
        if !note.clued || !self.is_rank_belief(&note.belief, 1) {
            return Some(first);
        }
        let ones: CardSet = playable
            .iter()
            .copied()
            .filter(|&c| self.note(c).clued && self.is_rank_belief(&self.note(c).belief, 1))
            .collect();
        self.first_one(seat, ones)
    }

    pub fn next_play(&self) -> Option<CardId> {
        self.next_play_for(self.me)
    }

    // ----- fixes -----

    /// Classify a clue as a fix using the receiver's knowledge before it.
    pub fn fix_kind(&self, receiver: Seat, clue: ClueId, touched: CardSet) -> Option<Fix> {
        if !self.rule(Rule::Fix) {
            return None;
        }
        let touch = self.variant.touch(clue);
        let mut informative = false;
        let mut conflicts = CardSet::EMPTY;
        for &card in self.state.hand(receiver) {
            let note = self.note(card);
            let mut narrowed = note.belief;
            narrowed.apply_clue(touch, touched.contains(card));
            informative |= touched.contains(card) && narrowed.hard_size() < note.belief.hard_size();
            if (note.clued || note.promised.is_some())
                && narrowed.hard_size() > 0
                && narrowed.size() == 0
                && (note.belief.size() > 0
                    || (touched.contains(card) && narrowed.hard_size() < note.belief.hard_size()))
            {
                conflicts.insert(card);
            }
        }
        if !conflicts.is_empty() {
            return Some(Fix::Literal(conflicts));
        }
        if informative
            || !self.newly_touched(receiver, touched).is_empty()
            || !self.rule(Rule::Stop)
        {
            return None;
        }
        let next = self.next_play_for(receiver)?;
        if touched.contains(next)
            && self.stop_options(receiver, next).size() > 0
            && touched.iter().all(|c| {
                let note = self.note(c);
                note.clued && (note.trash || (note.play && note.delayed == 0))
            })
        {
            return Some(Fix::Stop(next));
        }
        None
    }

    /// Another protected card of `identity` exists, so `card` would be a duplicate.
    pub fn protected_duplicate(&self, seat: Seat, card: CardId, identity: Identity) -> bool {
        self.state.seats().any(|holder| {
            self.state.hand(holder).iter().any(|&other| {
                let note = self.note(other);
                other != card
                    && note.clued
                    && !note.stopped
                    && (if holder == self.me || holder == seat {
                        note.belief.size() == 1 && note.belief.contains(identity)
                    } else {
                        self.identity(other) == Some(identity)
                    })
            })
        })
    }

    pub fn stop_options(&self, seat: Seat, card: CardId) -> Belief {
        let mut options = self.note(card).belief;
        options.reset_soft();
        let trash = self.trash_set();
        for identity in options.soft.iter() {
            if !trash.contains(identity) && !self.protected_duplicate(seat, card, identity) {
                options.remove(identity, true);
            }
        }
        options
    }

    /// A mistakenly duplicated direct play still identifies the card, so its receiver can
    /// return the copy with a sarcastic discard.
    pub fn duplicated_play(&self, seat: Seat, card: CardId) -> Option<Identity> {
        if !self.rule(Rule::DuplicatedPlay) {
            return None;
        }
        let mut playable = self.note(card).belief;
        playable.soft_limit(self.playable_set());
        let identity = playable.identity()?;
        self.state
            .seats()
            .filter(|&holder| holder != seat)
            .any(|holder| {
                self.state.hand(holder).iter().any(|&other| {
                    let note = self.note(other);
                    note.clued
                        && !note.stopped
                        && (if holder == self.me {
                            note.belief.size() == 1 && note.belief.contains(identity)
                        } else {
                            self.identity(other) == Some(identity)
                        })
                })
            })
            .then_some(identity)
    }

    /// Cards of `receiver` that `clue` touches, as far as this view can see them.
    pub fn visible_touched(&self, receiver: Seat, clue: ClueId) -> CardSet {
        let touch = self.variant.touch(clue);
        self.state
            .hand(receiver)
            .iter()
            .copied()
            .filter(|&c| self.identity(c).is_some_and(|i| touch.contains(i)))
            .collect()
    }

    pub fn clue_fixes(&self, receiver: Seat, clue: ClueId) -> bool {
        self.fix_kind(receiver, clue, self.visible_touched(receiver, clue))
            .is_some()
    }

    // ----- sarcastic discards -----

    pub(super) fn receive_sarcastic(&mut self, seat: Seat, card: CardId, identity: Identity) {
        if !self.note(card).clued || self.trash_set().contains(identity) {
            return;
        }
        let mut candidates = SmallVec::new();
        for holder in self.state.seats().filter(|&h| h != seat) {
            for &other in self.state.hand(holder) {
                let note = self.note(other);
                if note.clued
                    && !note.stopped
                    && note.belief.contains_hard(identity)
                    && (note.belief.contains(identity) || note.belief.size() != 1)
                {
                    candidates.push(other);
                }
            }
        }
        if !candidates.is_empty() {
            self.sarcastic.push(SarcasticTransfer {
                identity,
                candidates,
            });
        }
    }

    pub(super) fn refresh_sarcastic(&mut self) {
        let trash = self.trash_set();
        let mut transfers = std::mem::take(&mut self.sarcastic);
        transfers.retain_mut(|transfer| {
            if trash.contains(transfer.identity) {
                return false;
            }
            transfer.candidates.retain(|card| {
                self.state.holder(*card).is_some() && {
                    let note = self.note(*card);
                    !note.stopped && note.belief.contains_hard(transfer.identity)
                }
            });
            !transfer.candidates.is_empty()
        });
        for transfer in &transfers {
            if let [card] = transfer.candidates.as_slice() {
                let card = *card;
                let note = self.note_mut(card);
                note.base.reset_soft();
                note.base
                    .soft_limit(crate::domain::IdentitySet::single(transfer.identity));
                // The public discard corrects a duplicate's previous deduction.
                let identity = transfer.identity;
                self.interpretations
                    .retain_consistent(|step| step.card != card || step.identity == identity);
            }
        }
        self.sarcastic = transfers;
    }

    /// A disjunction cannot identify either card: keep literal choices until another public
    /// observation singles out a target.
    pub(super) fn apply_sarcastic_ambiguity(&mut self) {
        let transfers = self.sarcastic.clone();
        for transfer in &transfers {
            if transfer.candidates.len() < 2 {
                continue;
            }
            for &card in &transfer.candidates {
                if self.state.holder(card).is_some() {
                    let note = self.note_mut(card);
                    if note.belief.hard_size() > 1 {
                        note.belief.reset_soft();
                        note.play = false;
                        note.trash = false;
                    }
                }
            }
        }
    }

    /// A clued card of ours that duplicates a clued card elsewhere, to discard sarcastically.
    pub fn sarcastic_discard(&self) -> Option<CardId> {
        if !self.rule(Rule::SarcasticDiscard) {
            return None;
        }
        let trash = self.trash_set();
        self.newest_first(self.me).find_map(|(_, card)| {
            let note = self.note(card);
            if !note.clued || note.promised.is_some() || note.stopped {
                return None;
            }
            let mut belief = note.belief;
            if belief.size() == 0 {
                belief.reset_soft();
            }
            let identity = belief.identity()?;
            if trash.contains(identity) {
                return None;
            }
            // Layered Finesse Dupe Clue: a copy already promised to a blind play elsewhere
            // (a layer, or a finesse) makes ours a duplicate as well.
            let dupe = self.rule(Rule::LayeredFinesseDupe);
            self.others()
                .any(|seat| {
                    self.state.hand(seat).iter().any(|&other| {
                        let n = self.note(other);
                        self.identity(other) == Some(identity)
                            && ((n.clued && !n.stopped && n.belief.contains_hard(identity))
                                || (dupe && !n.clued && n.promised.is_some()))
                    })
                })
                .then_some(card)
        })
    }

    /// The convention discard: sarcastic, known trash, chop, then the card least likely to
    /// matter.
    pub fn discard_choice(&self) -> CardId {
        if let Some(card) = self.sarcastic_discard() {
            return card;
        }
        let mut chop = None;
        for (_, card) in self.newest_first(self.me) {
            let note = self.note(card);
            if note.trash {
                return card;
            }
            if !note.touched() {
                chop = Some(card);
            }
        }
        if let Some(card) = chop {
            return card;
        }
        // Locked hand: drop the card most likely to be a high rank.
        for rank in (1..=self.variant.num_ranks).rev() {
            for (_, card) in self.newest_first(self.me) {
                if self.is_rank_belief(&self.note(card).belief, rank) {
                    return card;
                }
            }
        }
        self.newest_first(self.me)
            .next()
            .map(|(_, c)| c)
            .expect("hand is not empty")
    }
}
