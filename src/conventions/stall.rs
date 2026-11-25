//! Stalling (level 2 5-stall, level 9 stall table): severity classifier and stall kinds.

use super::profile::Rule;
use super::view::{PlayState, View};
use crate::domain::{CardId, CardSet, ClueId, Seat};

/// How much pressure a clue giver is under to stall. Ordered by severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StallSeverity {
    Normal,
    Early,
    /// The end-game (level 8) where the giver may burn: stalls are read by context.
    EndGame,
    DoubleDiscard,
    Locked,
    EightClues,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StallKind {
    Five,
    Tempo,
    FillIn,
    LockedSave,
    EightClueSave,
    Burn,
}

impl StallKind {
    /// The `stall.table` slot: lower values are preferred.
    pub fn precedence(self) -> u8 {
        match self {
            Self::Five => 0,
            Self::Tempo => 1,
            Self::FillIn | Self::LockedSave | Self::EightClueSave => 2,
            Self::Burn => 3,
        }
    }

    pub fn is_general_save(self) -> bool {
        matches!(self, Self::LockedSave | Self::EightClueSave)
    }
}

impl View {
    fn max_clues(&self) -> u8 {
        self.variant.tokens.max / self.variant.tokens.clue_cost
    }

    /// Stall severity for `giver` on its turn, judged from what `giver` can see.
    pub fn stall_severity(&self, giver: Seat) -> StallSeverity {
        use StallSeverity::*;
        let clues = self.state.clue_tokens();
        if clues == 0 {
            return Normal;
        }
        let rotated;
        let view = if giver == self.me {
            self
        } else {
            rotated = self.decision_view(giver);
            &rotated
        };
        if view.next_play().is_some() {
            return Normal;
        }
        if clues == self.max_clues() && self.state.turn > 0 {
            return EightClues;
        }
        if clues < self.max_clues() && view.state.hand(view.me).iter().any(|&c| view.note(c).trash)
        {
            return Normal;
        }
        if view.chop(view.me).is_none() {
            return Locked;
        }
        // A scream or shout discard told this seat to clue instead of discarding.
        if self.context.must_clue == Some(giver) {
            return DoubleDiscard;
        }
        if let Some((previous, identity)) = self.context.last_loss
            && previous.after(1, self.num_seats()) == giver
            && matches!(
                self.play_state(identity),
                PlayState::Critical | PlayState::CriticalPlayable
            )
        {
            let discard = view.discard_choice();
            let visible_elsewhere = view.others().any(|seat| {
                view.state
                    .hand(seat)
                    .iter()
                    .any(|&c| view.identity(c) == Some(identity))
            });
            if !visible_elsewhere && view.note(discard).belief.contains(identity) {
                return DoubleDiscard;
            }
        }
        if self.in_end_game() {
            // The end-game alone is no stalling situation: a stall is read only where the
            // giver may burn a clue instead of discarding.
            if view.burn_allowed() { EndGame } else { Normal }
        } else if self.context.early_game {
            Early
        } else {
            Normal
        }
    }

    fn stall_allowed(&self, kind: StallKind, severity: StallSeverity) -> bool {
        self.rule(match kind {
            StallKind::Five if severity == StallSeverity::Early => Rule::FiveStallEarly,
            StallKind::Five => Rule::StallFive,
            StallKind::Tempo => Rule::StallTempo,
            StallKind::FillIn => Rule::StallFillIn,
            StallKind::LockedSave => Rule::StallLockedSave,
            StallKind::EightClueSave => Rule::StallEightClueSave,
            StallKind::Burn if severity == StallSeverity::EndGame => Rule::Burning,
            StallKind::Burn => Rule::StallBurn,
        })
    }

    /// Whether a clue is a stall, and which kind.
    pub fn stall_kind(
        &self,
        giver: Seat,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
    ) -> Option<StallKind> {
        if self.fix_kind(receiver, clue, touched).is_some() {
            return None;
        }
        let severity = self.stall_severity(giver);
        if severity == StallSeverity::Normal || touched.is_empty() {
            return None;
        }
        let kind = self.classify_stall(severity, receiver, clue, touched)?;
        self.stall_allowed(kind, severity).then_some(kind)
    }

    fn classify_stall(
        &self,
        severity: StallSeverity,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
    ) -> Option<StallKind> {
        let touch = self.variant.touch(clue);
        let top = self.variant.num_ranks;
        if !self.newly_touched(receiver, touched).is_empty() {
            let chop = self.chop(receiver);
            if self.is_rank_clue(clue, top) && chop.is_none_or(|c| !touched.contains(c)) {
                return Some(StallKind::Five);
            }
            let on_chop = chop.is_some_and(|c| touched.contains(c));
            if on_chop && (self.is_rank_clue(clue, 2) || self.is_rank_clue(clue, top)) {
                return None;
            }
            let focus = self.clue_focus(receiver, touched)?;
            let mut possibilities = self.note(focus).belief;
            possibilities.apply_clue(touch, true);
            if possibilities.size() > 0 && possibilities.is_subset_of(self.playable_set()) {
                return None;
            }
            if severity == StallSeverity::Locked
                && on_chop
                && self.state.hand(receiver).iter().any(|&c| {
                    let note = self.note(c);
                    !note.touched() && !touched.contains(c)
                })
            {
                return Some(StallKind::LockedSave);
            }
            let newest = self.newest_first(receiver).next().map(|(_, c)| c);
            if severity == StallSeverity::EightClues && Some(focus) != newest {
                return Some(StallKind::EightClueSave);
            }
            return None;
        }
        if severity < StallSeverity::EndGame {
            return None;
        }
        let mut informative = false;
        for card in touched.iter() {
            let note = self.note(card);
            let mut narrowed = note.belief;
            narrowed.apply_clue(touch, true);
            informative |= narrowed.hard_size() < note.belief.hard_size();
        }
        let focus = self.stall_focus(receiver, touched).expect("nonempty clue");
        let note = self.note(focus);
        let mut narrowed = note.belief;
        narrowed.apply_clue(touch, true);
        let playable = !note.play && note.delayed == 0 && narrowed.intersects(self.playable_set());
        Some(if playable {
            StallKind::Tempo
        } else if informative {
            StallKind::FillIn
        } else {
            StallKind::Burn
        })
    }

    /// The card a tempo clue asks to play: the newest touched card not already queued.
    pub fn stall_focus(&self, receiver: Seat, touched: CardSet) -> Option<CardId> {
        self.newest_first(receiver)
            .filter(|&(_, c)| touched.contains(c))
            .find(|&(_, c)| {
                let note = self.note(c);
                !note.play || note.delayed > 0
            })
            .or_else(|| {
                self.newest_first(receiver)
                    .find(|&(_, c)| touched.contains(c))
            })
            .map(|(_, c)| c)
    }

    /// The stall kind of our own candidate clue.
    pub fn clue_stall(&self, receiver: Seat, clue: ClueId) -> Option<StallKind> {
        self.stall_kind(
            self.me,
            receiver,
            clue,
            self.visible_touched(receiver, clue),
        )
    }

    /// 5 Stall Finesse Position Exception: our play clue to `target` would focus a playable
    /// card on its finesse position that another teammate could still finesse (its
    /// successor is visible to that teammate in a third hand, and a clue would be left).
    pub fn finesse_position_exception(&self, target: Seat, clue: ClueId) -> bool {
        if !self.rule(Rule::FiveStallFinesseException) || self.state.clue_tokens() < 2 {
            return false;
        }
        let touched = self.visible_touched(target, clue);
        let Some(focus) = self.clue_focus(target, touched) else {
            return false;
        };
        let finesse_position = self
            .newest_first(target)
            .map(|(_, c)| c)
            .find(|&c| !self.note(c).touched() && !self.note(c).play);
        if finesse_position != Some(focus) || self.chop(target) == Some(focus) {
            return false;
        }
        let Some(identity) = self.identity(focus) else {
            return false;
        };
        let rank = self.variant.rank_of(identity);
        if !self.playable_set().contains(identity) || rank >= self.variant.num_ranks {
            return false;
        }
        let successor = self
            .variant
            .identity(self.variant.suit_of(identity), rank + 1);
        self.others().filter(|&z| z != target).any(|finesser| {
            self.others()
                .filter(|&w| w != target && w != finesser)
                .any(|holder| {
                    self.state
                        .hand(holder)
                        .iter()
                        .any(|&c| !self.note(c).clued && self.identity(c) == Some(successor))
                })
        })
    }

    /// The Anxiety Play: locked at 0 clues with no known play or trash, play the card most
    /// likely to be playable (the newest on ties), while a strike can be spared.
    pub fn anxiety_play(&self) -> Option<CardId> {
        if !self.rule(Rule::AnxietyPlay)
            || self.chop(self.me).is_some()
            || self.state.clue_tokens() > 0
            || self.state.strikes >= 2
            || self.next_play().is_some()
            || self.state.hand(self.me).iter().any(|&c| self.note(c).trash)
        {
            return None;
        }
        let known = self.known_counts();
        let playable = self.playable_set();
        let mut best: Option<(u32, u32, CardId)> = None;
        for (_, card) in self.newest_first(self.me) {
            let (mut good, mut total) = (0u32, 0u32);
            for identity in self.note(card).belief.soft.iter() {
                let remaining = self
                    .variant
                    .copies(identity)
                    .saturating_sub(known[identity.index()]) as u32;
                total += remaining;
                if playable.contains(identity) {
                    good += remaining;
                }
            }
            if total == 0 || good == 0 {
                continue;
            }
            // compare good/total without division; keep the newest on ties
            if best.is_none_or(|(g, t, _)| good * t > g * total) {
                best = Some((good, total, card));
            }
        }
        best.map(|(_, _, card)| card)
    }

    /// How far the nearest unclued 5 is from chop (`u8::MAX` if none).
    pub fn five_stall_distance(&self, receiver: Seat) -> u8 {
        let mut distance = 0;
        for &card in self.state.hand(receiver) {
            let note = self.note(card);
            if note.touched() {
                continue;
            }
            if self
                .identity(card)
                .is_some_and(|i| self.variant.rank_of(i) == self.variant.num_ranks)
            {
                return distance;
            }
            distance += 1;
        }
        u8::MAX
    }

    pub fn saves_critical_chop(&self, receiver: Seat, clue: ClueId) -> bool {
        let Some(card) = self.chop(receiver) else {
            return false;
        };
        let Some(identity) = self.identity(card) else {
            return false;
        };
        self.variant.touch(clue).contains(identity)
            && self.save_clue_allowed(receiver, clue, self.known_touches(receiver, clue), identity)
            && matches!(
                self.play_state(identity),
                PlayState::Critical | PlayState::CriticalPlayable
            )
    }
}
