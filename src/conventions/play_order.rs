//! Which of several known plays to make first.
//!
//! The conventions fix only a few orderings (finessed cards before clued plays, the 1s
//! order). Among the remaining plays a card is preferred by what its play does for the
//! team, in this order:
//!
//! 1. it fulfils a finesse or prompt another player is waiting on (blind plays first);
//! 2. it unblocks other players: a touched card elsewhere becomes a known play, above all
//!    for a player who has no play now;
//! 3. it completes a stack and so returns a clue token while clues are needed;
//! 4. it makes more of the holder's own cards playable.
//!
//! Ties keep the hand order (newest first). Every criterion is read from the deciding
//! view, with the holder's beliefs for cards it cannot see, so the same function predicts
//! a teammate's next play.

use super::view::View;
use crate::domain::{CardId, Identity, IdentitySet, Seat};

/// What playing a card does for the team; compared field by field, larger is better.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PlayPriority {
    /// An unclued card promised by a finesse: playing it proves the finesse.
    pub blind: bool,
    /// Another player's connection waits on this card.
    pub connection: bool,
    /// Players with no play now who gain a known play.
    pub unblocked_players: u8,
    /// Touched cards in other hands that become known plays.
    pub unblocked_cards: u8,
    /// Completes a stack while the team needs the clue token.
    pub clue: bool,
    /// Own touched cards that become known plays.
    pub own_followups: u8,
}

impl PlayPriority {
    /// The weakest outcome per criterion over the identities the card may be.
    fn meet(self, other: Self) -> Self {
        Self {
            blind: self.blind && other.blind,
            connection: self.connection && other.connection,
            unblocked_players: self.unblocked_players.min(other.unblocked_players),
            unblocked_cards: self.unblocked_cards.min(other.unblocked_cards),
            clue: self.clue && other.clue,
            own_followups: self.own_followups.min(other.own_followups),
        }
    }
}

impl View {
    /// The first of `candidates` (in hand order) whose play helps the team most.
    pub(super) fn best_play(&self, seat: Seat, candidates: &[CardId]) -> Option<CardId> {
        if candidates.len() <= 1 {
            return candidates.first().copied();
        }
        let mut best: Option<(PlayPriority, CardId)> = None;
        for &card in candidates {
            let priority = self.play_priority(seat, card);
            if best.is_none_or(|(b, _)| priority > b) {
                best = Some((priority, card));
            }
        }
        best.map(|(_, card)| card)
    }

    pub(super) fn play_priority(&self, seat: Seat, card: CardId) -> PlayPriority {
        let note = self.note(card);
        let blind = !note.clued && note.promised.is_some();
        let connection = self.connection_waits_on(seat, card);
        let mut identities = note.belief.soft & self.playable_set();
        if identities.is_empty() {
            identities = note.belief.hard & self.playable_set();
        }
        let clues_needed = self.clues_needed(seat);
        let mut outcome: Option<PlayPriority> = None;
        for identity in identities.iter() {
            let effect = self.play_effect(seat, card, identity, clues_needed);
            outcome = Some(outcome.map_or(effect, |o| o.meet(effect)));
        }
        PlayPriority {
            blind,
            connection,
            ..outcome.unwrap_or_default()
        }
    }

    /// Whether a live reading has `card` as a pending step that comes before a step held
    /// by another player (a finesse or prompt someone else's clue relies on).
    fn connection_waits_on(&self, seat: Seat, card: CardId) -> bool {
        self.interpretations.records.iter().any(|record| {
            let known = self.known_focus(record);
            record.for_seat(seat, known).any(|hypothesis| {
                let mut steps = hypothesis.pending_steps();
                steps.by_ref().any(|step| step.card == card)
                    && steps.any(|step| step.holder != seat)
            })
        })
    }

    /// The team can use another clue token: it is not at the maximum, and clues are scarce
    /// or a teammate has nothing to do but discard.
    fn clues_needed(&self, seat: Seat) -> bool {
        let max = self.variant.tokens.max;
        if self.state.tokens + self.variant.tokens.stack_complete_gain > max {
            return false;
        }
        self.state.clue_tokens() < 2
            || self
                .state
                .seats()
                .filter(|&s| s != seat)
                .any(|s| !self.is_loaded(s))
    }

    /// Unblocking, clue and follow-up effects of playing `card` as `identity`.
    fn play_effect(
        &self,
        seat: Seat,
        card: CardId,
        identity: Identity,
        clues_needed: bool,
    ) -> PlayPriority {
        let mut stacks = self.state.stacks;
        let completes = self.variant.play(&mut stacks, identity);
        let before = self.playable_set();
        let after = self.variant.playable(&stacks);
        let newly = after - before;
        let mut priority = PlayPriority {
            clue: completes && clues_needed,
            ..PlayPriority::default()
        };
        if newly.is_empty() {
            return priority;
        }
        for holder in self.state.seats() {
            let gained = self
                .state
                .hand(holder)
                .iter()
                .filter(|&&other| other != card && self.becomes_known_play(other, newly, after))
                .count() as u8;
            if gained == 0 {
                continue;
            }
            if holder == seat {
                priority.own_followups += gained;
            } else {
                priority.unblocked_cards += gained;
                if !self.has_known_play(holder, card) {
                    priority.unblocked_players += 1;
                }
            }
        }
        priority
    }

    /// A touched card that is not a known play now and will be one once `newly` becomes
    /// playable: its holder's belief fits the new stacks (or it waits on a connection),
    /// and as far as this view can see it really is one of the new identities.
    fn becomes_known_play(&self, card: CardId, newly: IdentitySet, after: IdentitySet) -> bool {
        let note = self.note(card);
        if !note.touched() || note.trash || note.stopped || (note.play && note.delayed == 0) {
            return false;
        }
        let fits =
            note.belief.intersects(newly) && (note.delayed > 0 || note.belief.is_subset_of(after));
        fits && self.identity(card).is_none_or(|i| newly.contains(i))
    }

    /// `seat` already has a known play other than `except`.
    fn has_known_play(&self, seat: Seat, except: CardId) -> bool {
        let playable = self.playable_set();
        self.state.hand(seat).iter().any(|&c| {
            let note = self.note(c);
            c != except
                && !note.trash
                && note.play
                && note.delayed == 0
                && note.belief.intersects(playable)
        })
    }
}
