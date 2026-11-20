//! Special discards of level 10: the Gentleman's Discard (and its layered form), the Baton
//! Discard, and the Certain Discard of a finessed card.
//!
//! A Gentleman's Discard throws away a clued card whose identity is playable (and that
//! duplicates nothing clued or promised elsewhere): its other copy is exactly on another
//! player's finesse position, possibly behind playable cards (layered). It is read as a
//! finesse record on that card, so blind plays, layers and declines work as for finesses.
//! A Baton Discard does the same with a known unplayable card; the receiving card then
//! counts as clued with that identity. Only the true identity is transferred, so a Double
//! Gentleman's Discard has no meaning.

use smallvec::SmallVec;

use super::profile::Rule;
use super::records::{ClueRecord, Hypothesis, Step, StepKind, Tier};
use super::view::View;
use crate::domain::{CardId, Identity, IdentitySet, Seat};

/// Which special discard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Gentleman,
    Baton,
}

impl View {
    /// The finesse position of `seat`: the newest card neither touched nor queued to play.
    fn finesse_card(&self, seat: Seat) -> Option<CardId> {
        self.newest_first(seat)
            .map(|(_, c)| c)
            .find(|&c| !self.note(c).touched() && !self.note(c).play)
    }

    /// Whether the discard of `card` (identity `identity`, now public) is a Gentleman's or
    /// Baton discard. Judged before the discard.
    pub fn transfer_kind(&self, seat: Seat, card: CardId, identity: Identity) -> Option<Transfer> {
        if !self.rule(Rule::GentlemansDiscard) {
            return None;
        }
        let note = self.note(card);
        // Only a card whose identity the discarder demonstrably knew (from public clue
        // information) is transferred, and only by choice: a locked player's discard is
        // forced.
        let known = (self.public.literal(card) - self.trash_set()).single_item();
        if self.chop(seat).is_none()
            || !note.clued
            || note.trash
            || known != Some(identity)
            || self.trash_set().contains(identity)
        {
            return None;
        }
        // A possible duplicate of a clued or promised card (our own cards by belief, since
        // we cannot see them) makes it a sarcastic or dupe discard instead.
        let duplicate = self.state.seats().any(|holder| {
            self.state.hand(holder).iter().any(|&other| {
                let n = self.note(other);
                other != card
                    && (n.clued || n.promised.is_some())
                    && if holder == self.me {
                        n.belief.contains(identity)
                    } else {
                        self.identity(other) == Some(identity)
                    }
            })
        });
        if duplicate {
            return None;
        }
        if self.playable_set().contains(identity) {
            Some(Transfer::Gentleman)
        } else if self.rule(Rule::BatonDiscard) {
            Some(Transfer::Baton)
        } else {
            None
        }
    }

    /// Where a transfer of `identity` by `discarder` lands, as this view can tell: the first
    /// other seat (in turn order) whose finesse position visibly holds it, exactly or (for
    /// a Gentleman's Discard) behind playable cards; otherwise our own finesse position.
    /// Returns the seat, the card and the number of layers in front of it.
    pub fn transfer_target(
        &self,
        discarder: Seat,
        identity: Identity,
        kind: Transfer,
    ) -> Option<(Seat, CardId, u8)> {
        let n = self.num_seats();
        let others = || {
            (1..n)
                .map(|offset| discarder.after(offset, n))
                .filter(|&s| s != self.me)
        };
        for seat in others() {
            if let Some(card) = self.finesse_card(seat)
                && self.identity(card) == Some(identity)
            {
                return Some((seat, card, 0));
            }
        }
        if kind == Transfer::Gentleman {
            let playable = self.playable_set();
            for seat in others() {
                let mut layers: SmallVec<[Identity; 4]> = SmallVec::new();
                for (_, card) in self.newest_first(seat) {
                    let note = self.note(card);
                    if note.touched() || note.play {
                        continue;
                    }
                    let Some(seen) = self.identity(card) else {
                        break;
                    };
                    if seen == identity {
                        if !layers.is_empty() {
                            return Some((seat, card, layers.len() as u8));
                        }
                        break;
                    }
                    if !playable.contains(seen) || layers.contains(&seen) {
                        break;
                    }
                    layers.push(seen);
                }
            }
        }
        if discarder == self.me {
            return None;
        }
        self.finesse_card(self.me).map(|card| (self.me, card, 0))
    }

    /// Apply a transfer by `seat` (after the discard is public).
    pub(super) fn apply_transfer(
        &mut self,
        seat: Seat,
        discarded: CardId,
        identity: Identity,
        kind: Transfer,
    ) {
        let Some((target, card, layers)) = self.transfer_target(seat, identity, kind) else {
            return;
        };
        match kind {
            Transfer::Gentleman => {
                let step = Step {
                    identity,
                    holder: target,
                    kind: StepKind::Finesse,
                    card,
                    fallback: None,
                    layers,
                    done: false,
                    known_play: false,
                };
                self.interpretations.add(ClueRecord {
                    turn: self.turn,
                    giver: seat,
                    receiver: target,
                    focus: discarded,
                    hypotheses: vec![Hypothesis {
                        focus_identity: identity,
                        steps: smallvec::smallvec![step],
                        tier: Tier::Direct,
                    }],
                });
                self.last_rule = "L10.gentlemans-discard";
            }
            Transfer::Baton => {
                // The card counts as clued with the transferred identity.
                let note = self.note_mut(card);
                note.clued = true;
                note.base.soft_limit(IdentitySet::single(identity));
                self.last_rule = "L10.baton-discard";
            }
        }
    }

    /// A clued card of ours to give away: a Gentleman's Discard at 0 clues when the next
    /// player would have to discard, or a Baton Discard instead of discarding an unknown
    /// chop. Every teammate must agree on the receiving card.
    pub fn transfer_discard(&self) -> Option<CardId> {
        if !self.rule(Rule::GentlemansDiscard) || self.state.tokens >= self.variant.tokens.max {
            return None;
        }
        // A Gentleman's Discard costs tempo; give it only where a generation discard would
        // otherwise be needed (the next player would discard while the player after loses
        // a critical chop card); the card is not lost this way. A Baton Discard protects
        // our own chop when it may be critical.
        let next_discards = self.state.clue_tokens() == 0 && self.generation_needed();
        let critical: IdentitySet = self
            .variant
            .all_identities()
            .iter()
            .filter(|&i| self.is_critical(i))
            .collect();
        let would_discard_chop = self.next_play().is_none()
            && !self.state.hand(self.me).iter().any(|&c| self.note(c).trash)
            && self
                .chop(self.me)
                .is_some_and(|chop| self.note(chop).belief.intersects(critical));
        self.newest_first(self.me).map(|(_, c)| c).find(|&card| {
            let note = self.note(card);
            let Some(identity) = note.belief.identity() else {
                return false;
            };
            if !note.clued || self.trash_set().contains(identity) {
                return false;
            }
            let kind = if self.playable_set().contains(identity) {
                if !next_discards {
                    return false;
                }
                Transfer::Gentleman
            } else {
                if !would_discard_chop || !self.rule(Rule::BatonDiscard) {
                    return false;
                }
                Transfer::Baton
            };
            // Our own reading (not counting ourselves) and every teammate's must agree.
            let Some((target, target_card, layers)) = self.transfer_target(self.me, identity, kind)
            else {
                return false;
            };
            if layers > 0 || self.transfer_kind(self.me, card, identity) != Some(kind) {
                return false;
            }
            self.others().all(|seat| {
                self.decision_view(seat)
                    .transfer_target(self.me, identity, kind)
                    .is_some_and(|(s, c, _)| s == target && c == target_card)
            })
        })
    }

    /// Certain Discard: a finessed card of ours whose promised identity is clued in
    /// another hand is a duplicate; discard it instead of blind-playing it.
    pub fn certain_discard(&self) -> Option<CardId> {
        if !self.rule(Rule::CertainDiscard) {
            return None;
        }
        self.newest_first(self.me).map(|(_, c)| c).find(|&card| {
            let note = self.note(card);
            if note.clued || note.promised.is_none() {
                return false;
            }
            let Some(identity) = note.belief.identity() else {
                return false;
            };
            self.others().any(|seat| {
                self.state.hand(seat).iter().any(|&other| {
                    let n = self.note(other);
                    n.clued && !n.stopped && self.identity(other) == Some(identity)
                })
            })
        })
    }

    /// Observers of a Certain Discard: the finesse step on the discarded card moves to the
    /// clued copy (a prompt), and that copy learns its identity.
    pub(super) fn receive_certain_discard(&mut self, seat: Seat, card: CardId, identity: Identity) {
        if !self.rule(Rule::CertainDiscard) {
            return;
        }
        let note = self.note(card);
        if note.clued || note.promised.is_none() {
            return;
        }
        let candidate = self
            .state
            .seats()
            .filter(|&h| h != seat)
            .find_map(|holder| {
                self.state
                    .hand(holder)
                    .iter()
                    .copied()
                    .find(|&other| {
                        let n = self.note(other);
                        n.clued && !n.stopped && n.belief.contains_hard(identity)
                    })
                    .map(|c| (holder, c))
            });
        let Some((holder, copy)) = candidate else {
            return;
        };
        for record in &mut self.interpretations.records {
            for hypothesis in &mut record.hypotheses {
                for step in &mut hypothesis.steps {
                    if !step.done && step.card == card && step.identity == identity {
                        step.card = copy;
                        step.holder = holder;
                        step.kind = StepKind::Prompt;
                        step.layers = 0;
                    }
                }
            }
        }
        self.note_mut(copy)
            .base
            .soft_limit(IdentitySet::single(identity));
    }
}
