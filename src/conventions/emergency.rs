//! Emergency discards (level 7): scream, shout and generation discards, read from discards
//! and chosen by the bot, and blind plays of chop-moved cards.
//!
//! A scream discard (discarding the chop while holding a known play, at 0 clues or at 1
//! clue with a locked next player, or while holding known trash) and a shout discard
//! (discarding known trash while holding a known play) chop move the next player's chop
//! and ask that player to clue instead of discarding. A generation discard (discarding a
//! known-playable clued card) only asks the next player to clue.

use super::profile::Rule;
use super::view::{PlayState, View};
use crate::domain::{CardId, Identity, Seat};

/// The meaning of a special discard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DiscardSignal {
    Scream,
    Shout,
    Generation,
}

impl View {
    fn known_play_besides(&self, seat: Seat, card: CardId) -> bool {
        self.state.hand(seat).iter().any(|&c| {
            let note = self.note(c);
            c != card && note.play && note.delayed == 0 && !note.trash
        })
    }

    fn known_trash_besides(&self, seat: Seat, card: CardId) -> bool {
        self.state
            .hand(seat)
            .iter()
            .any(|&c| c != card && self.note(c).trash)
    }

    /// What `seat` discarding `card` signals, judged before the discard (not for sarcastic
    /// discards, which the caller handles first).
    /// `identity` is the discarded card's identity once it is public (`None` for a
    /// discard we only consider).
    pub fn discard_signal(
        &self,
        seat: Seat,
        card: CardId,
        identity: Option<Identity>,
    ) -> Option<DiscardSignal> {
        if !self.rule(Rule::ScreamDiscard) && !self.rule(Rule::GenerationDiscard) {
            return None;
        }
        let note = self.note(card);
        let next = seat.after(1, self.num_seats());
        if note.trash {
            return (self.rule(Rule::ShoutDiscard) && self.known_play_besides(seat, card))
                .then_some(DiscardSignal::Shout);
        }
        if note.clued && note.play && note.delayed == 0 {
            // Returning a duplicate of a card clued or promised elsewhere is a sarcastic or
            // dupe discard, not a generation discard.
            let identity = identity
                .or_else(|| self.identity(card))
                .or_else(|| note.belief.identity());
            let duplicate = identity.is_some_and(|identity| {
                self.state.seats().any(|holder| {
                    self.state.hand(holder).iter().any(|&other| {
                        let n = self.note(other);
                        other != card
                            && (n.clued || n.promised.is_some())
                            && self.identity(other) == Some(identity)
                    })
                })
            });
            return (self.rule(Rule::GenerationDiscard) && !duplicate)
                .then_some(DiscardSignal::Generation);
        }
        if !self.rule(Rule::ScreamDiscard) || self.chop(seat) != Some(card) {
            return None;
        }
        let tokens = self.state.clue_tokens();
        let play = self.known_play_besides(seat, card)
            && (tokens == 0 || (tokens == 1 && self.chop(next).is_none()));
        (play || self.known_trash_besides(seat, card)).then_some(DiscardSignal::Scream)
    }

    /// Apply a special discard by `seat` (after the discard is public).
    pub(super) fn apply_discard_signal(&mut self, seat: Seat, signal: DiscardSignal) {
        let next = seat.after(1, self.num_seats());
        if signal != DiscardSignal::Generation
            && self.chop_moves_allowed()
            && let Some(chop) = self.chop(next)
        {
            self.note_mut(chop).chop_moved = true;
        }
        self.context.must_clue = Some(next);
        self.last_rule = match signal {
            DiscardSignal::Scream => "L7.scream-discard",
            DiscardSignal::Shout => "L7.shout-discard",
            DiscardSignal::Generation => "L7.generation-discard",
        };
    }

    /// `seat` is about to discard a visible chop card worth saving: no known play, no known
    /// trash, and the chop is critical (or, unless `critical_only`, playable or an unseen 2).
    fn chop_endangered(&self, seat: Seat, critical_only: bool) -> bool {
        if seat == self.me {
            return false;
        }
        let Some(chop) = self.chop(seat) else {
            return false;
        };
        let Some(identity) = self.identity(chop) else {
            return false;
        };
        let valuable = if critical_only {
            self.is_critical(identity) && self.worth_protecting(chop)
        } else {
            self.worth_protecting(chop)
                || (self.play_state(identity) == PlayState::Playable
                    && self.info[identity.index()].clued.is_none())
        };
        if !valuable {
            return false;
        }
        let view = self.decision_view(seat);
        view.next_play().is_none() && view.discard_choice() == chop
    }

    /// The scream or shout discard we should make now, if the next player is about to lose
    /// a valuable chop card for lack of clues.
    pub fn signal_discard(&self) -> Option<(CardId, DiscardSignal)> {
        if !self.rule(Rule::ScreamDiscard) || !self.chop_moves_allowed() {
            return None;
        }
        let tokens = self.state.clue_tokens();
        if tokens > 1 || self.state.tokens >= self.variant.tokens.max {
            return None;
        }
        let next = self.me.after(1, self.num_seats());
        let play = self.next_play();
        let trash = self
            .newest_first(self.me)
            .map(|(_, c)| c)
            .find(|&c| self.note(c).trash);
        // A shout loses nothing; a scream risks our own unknown chop, so only for a
        // critical card.
        let shout = play.is_some() && trash.is_some() && self.rule(Rule::ShoutDiscard);
        if !self.chop_endangered(next, !shout) {
            return None;
        }
        match (play, trash) {
            (Some(_), Some(trash)) if shout => Some((trash, DiscardSignal::Shout)),
            (Some(_), None) if tokens == 0 => {
                self.chop(self.me).map(|chop| (chop, DiscardSignal::Scream))
            }
            (None, Some(_)) if tokens == 0 => {
                self.chop(self.me).map(|chop| (chop, DiscardSignal::Scream))
            }
            _ => None,
        }
    }

    /// A generation discard: at 0 clues, playing would leave the next player no choice but
    /// to discard while the player after that is about to lose a valuable chop card; we
    /// discard our known-playable clued card instead so that the next player can clue.
    pub fn generation_discard(&self) -> Option<CardId> {
        if !self.rule(Rule::GenerationDiscard) || self.state.clue_tokens() > 0 {
            return None;
        }
        let card = self.next_play()?;
        // The discarded card must be known exactly and have another copy left.
        let note = self.note(card);
        if !note.clued || note.belief.identity().is_none_or(|i| self.is_critical(i)) {
            return None;
        }
        let n = self.num_seats();
        let next = self.me.after(1, n);
        let after = self.me.after(2, n);
        if after == self.me || self.chop_endangered(next, false) {
            return None;
        }
        let next_view = self.decision_view(next);
        (next_view.next_play().is_none() && self.chop_endangered(after, true)).then_some(card)
    }

    /// A chop-moved card of ours that is probably playable (at least 80% by the copies we
    /// cannot see), blind-played only while the team has a strike to spare.
    pub fn chop_moved_blind_play(&self) -> Option<CardId> {
        if !self.rule(Rule::BlindPlayChopMoved) || self.state.strikes >= 2 {
            return None;
        }
        let known = self.known_counts();
        let playable = self.playable_set();
        self.newest_first(self.me).find_map(|(_, card)| {
            let note = self.note(card);
            if note.clued || !note.chop_moved {
                return None;
            }
            let (mut good, mut total) = (0u32, 0u32);
            for identity in note.belief.soft.iter() {
                let remaining = self
                    .variant
                    .copies(identity)
                    .saturating_sub(known[identity.index()]) as u32;
                total += remaining;
                if playable.contains(identity) {
                    good += remaining;
                }
            }
            (total > 0 && good * 5 >= total * 4).then_some(card)
        })
    }
}
