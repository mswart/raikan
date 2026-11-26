//! The end-game (level 8): pace, positional discards and misplays, distribution clues and
//! burning.
//!
//! Pace is the score plus the cards left in the deck plus the number of players minus the
//! maximum score. The end-game starts when pace drops below the number of players; chop
//! moves are off from then on. Once the deck is empty a player can see every card, so a
//! discard or misplay from an unexpected slot points at the same slot of the last player
//! (in turn order) who holds a playable card there.

use smallvec::SmallVec;

use super::profile::Rule;
use super::view::View;
use crate::domain::{CardId, IdentitySet, Seat};

impl View {
    /// Score + cards left in the deck + players − maximum score.
    pub fn pace(&self) -> i16 {
        self.score() as i16 + self.state.cards_left() as i16 + self.num_seats() as i16
            - self.variant.max_score() as i16
    }

    /// The end-game (level 8): pace is below the number of players.
    pub fn in_end_game(&self) -> bool {
        self.rule(Rule::EndGame) && self.pace() < self.num_seats() as i16
    }

    /// Hand position (1 = newest) of `card` in `seat`'s hand.
    pub fn slot_of(&self, seat: Seat, card: CardId) -> Option<u8> {
        self.newest_first(seat)
            .find(|&(_, c)| c == card)
            .map(|(pos, _)| pos + 1)
    }

    /// The card of `seat` at hand position `slot` (1 = newest).
    pub fn card_at(&self, seat: Seat, slot: u8) -> Option<CardId> {
        self.newest_first(seat)
            .find(|&(pos, _)| pos + 1 == slot)
            .map(|(_, c)| c)
    }

    /// The card `seat` would discard as far as everybody can tell: known trash first,
    /// otherwise its chop.
    pub fn expected_discard(&self, seat: Seat) -> Option<CardId> {
        self.newest_first(seat)
            .map(|(_, c)| c)
            .find(|&c| self.note(c).trash)
            .or_else(|| self.chop(seat))
    }

    /// Whether `card` of `seat` is a playable candidate for a positional signal, as this
    /// view can tell: a visible unclued playable card, or an unknown card of ours that may
    /// be playable.
    fn positional_candidate(&self, seat: Seat, card: CardId) -> bool {
        let note = self.note(card);
        if note.clued || note.play || note.chop_moved {
            return false;
        }
        if seat == self.me {
            note.belief.intersects(self.playable_set())
        } else {
            self.identity(card)
                .is_some_and(|i| self.playable_set().contains(i))
        }
    }

    /// The seats a positional signal of `signaller` on `slot` asks to blind-play (the last
    /// `count` seats in turn order holding a playable card on that slot), as this view can
    /// tell.
    pub fn positional_targets(
        &self,
        signaller: Seat,
        slot: u8,
        count: usize,
    ) -> SmallVec<[Seat; 2]> {
        let n = self.num_seats();
        let mut targets: SmallVec<[Seat; 2]> = SmallVec::new();
        for offset in (1..n).rev() {
            let seat = signaller.after(offset, n);
            if targets.len() == count {
                break;
            }
            if let Some(card) = self.card_at(seat, slot)
                && self.positional_candidate(seat, card)
            {
                targets.push(seat);
            }
        }
        targets
    }

    /// Whether a discard (or misplay) by `seat` of `card` is positional: the deck is empty
    /// and the card is not the one `seat` would be expected to discard. Judged before the
    /// action.
    pub fn positional_slot(&self, seat: Seat, card: CardId, misplay: bool) -> Option<(u8, usize)> {
        if self.state.cards_left() > 0 {
            return None;
        }
        let note = self.note(card);
        if note.clued || note.chop_moved || note.promised.is_some() || (misplay && note.play) {
            return None;
        }
        let slot = self.slot_of(seat, card)?;
        let expected = self.expected_discard(seat) == Some(card);
        if misplay {
            if !self.rule(Rule::PositionalMisplay) {
                return None;
            }
            // A misplay where a discard would have worked promises two plays.
            Some((slot, if expected { 1 } else { 2 }))
        } else {
            (self.rule(Rule::PositionalDiscard) && !expected).then_some((slot, 1))
        }
    }

    /// Mark the targets of a positional signal as playable (after the action is public).
    pub(super) fn apply_positional(&mut self, signaller: Seat, slot: u8, count: usize) {
        let playable = self.playable_set();
        for seat in self.positional_targets(signaller, slot, count) {
            // The signaller's hand shifted after the action; others' hands did not.
            if let Some(card) = self.card_at(seat, slot) {
                let note = self.note_mut(card);
                note.base.soft_limit(playable);
            }
        }
        self.last_rule = if count == 1 {
            "L8.positional-discard"
        } else {
            "L8.positional-misplay"
        };
    }

    /// A positional discard or misplay of ours that every teammate reads as a play of the
    /// same visible playable card: `(our card, misplay)`.
    pub fn positional_signal(&self) -> Option<(CardId, bool)> {
        if !self.rule(Rule::PositionalDiscard) || self.state.cards_left() > 0 {
            return None;
        }
        let trash = self.trash_set();
        // We can see every card: our unknown cards must all be trash.
        if !self.state.hand(self.me).iter().all(|&c| {
            let note = self.note(c);
            note.clued || note.belief.soft.is_subset(trash)
        }) {
            return None;
        }
        let expected = self.expected_discard(self.me);
        for (pos, card) in self.newest_first(self.me) {
            let slot = pos + 1;
            if self.note(card).clued {
                continue;
            }
            let misplay = expected == Some(card);
            if misplay && (!self.rule(Rule::PositionalMisplay) || self.state.strikes >= 2) {
                continue;
            }
            if !misplay && self.state.tokens >= self.variant.tokens.max {
                continue;
            }
            let targets = self.positional_targets(self.me, slot, 1);
            let Some(&target) = targets.first() else {
                continue;
            };
            let target_card = self.card_at(target, slot).expect("target has the slot");
            // The target must not already know to play it, and every teammate must agree.
            if self.note(target_card).play {
                continue;
            }
            let agreed = self.others().all(|seat| {
                self.decision_view(seat)
                    .positional_targets(self.me, slot, 1)
                    .first()
                    == Some(&target)
            });
            if agreed {
                return Some((card, misplay));
            }
        }
        None
    }

    /// Identities a play clue may focus despite good touch in the end-game: playable
    /// identities clued in a hand that already holds two or more queued plays (a
    /// distribution clue).
    pub(super) fn distribution_identities(&self, receiver: Seat) -> IdentitySet {
        if !self.rule(Rule::DistributionClue) || !self.in_stall_end_game() {
            return IdentitySet::EMPTY;
        }
        let mut set = IdentitySet::EMPTY;
        for seat in self.state.seats().filter(|&s| s != receiver) {
            let hand = self.state.hand(seat);
            let queued = hand
                .iter()
                .filter(|&&c| {
                    let note = self.note(c);
                    note.clued && note.play && !note.trash
                })
                .count();
            if queued < 2 {
                continue;
            }
            for &card in hand {
                let note = self.note(card);
                if note.clued
                    && note.play
                    && let Some(identity) = note.belief.identity()
                    && self.playable_set().contains(identity)
                {
                    set.insert(identity);
                }
            }
        }
        set
    }

    /// The real end-game for stalling: fewer than 1.5 × players cards remain in the deck.
    /// In hard variants pace drops below the number of players long before that, and an
    /// early burn would waste clues on fill-ins while discards still draw useful cards.
    pub fn in_stall_end_game(&self) -> bool {
        self.in_end_game() && 2 * u16::from(self.state.cards_left()) < 3 * self.num_seats() as u16
    }

    /// Burning (level 8): in the real end-game ([`View::in_stall_end_game`]), a stall clue
    /// is worth more than a discard when we hold two or more useful clued cards (team
    /// distribution), no discard is left without losing a point (pace 0), or the last
    /// discard belongs to a teammate (the Pace +1 rule: we have a play, a teammate has
    /// none; without a play of our own we may discard).
    pub fn burn_allowed(&self) -> bool {
        if !self.rule(Rule::Burning) || !self.in_stall_end_game() {
            return false;
        }
        let useful = self
            .state
            .hand(self.me)
            .iter()
            .filter(|&&c| {
                let note = self.note(c);
                note.clued && !note.trash
            })
            .count();
        if useful >= 2 || self.pace() <= 0 {
            return true;
        }
        self.pace() == 1
            && self.next_play().is_some()
            && self
                .others()
                .any(|seat| self.decision_view(seat).next_play().is_none())
    }
}
