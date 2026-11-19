//! Clue worth: a public, deterministic grading of the position after a clue, as a sum of
//! named terms (score, touched and saved cards, finesses, queued plays, chop risk, errors).

use smallvec::SmallVec;

use super::records::StepKind;
use super::view::{PlayState, View};
use crate::domain::{CardId, ClueId, Identity, Seat};
use crate::engine::Event;
use crate::variants::{Stack, Stacks};

const CRITICAL_CARD_VALUE: i32 = 5;

/// Named terms of a position's worth.
#[derive(PartialEq, Eq, Clone, Debug, Default, serde::Serialize)]
pub struct Worth {
    pub discard_risks: i8,
    pub score: u8,
    pub clued: u8,
    pub saved: u8,
    pub finesses: u8,
    pub plays: u8,
    pub errors: u8,
    pub bonus: u8,
}

impl Worth {
    fn weighted(&self) -> i32 {
        self.discard_risks as i32
            + self.plays as i32
            + self.clued as i32 * 2
            + self.saved as i32 * CRITICAL_CARD_VALUE
            + self.finesses as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10
    }

    pub fn no_more_errors_than(&self, other: &Self) -> bool {
        self.errors <= other.errors
    }

    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    /// Leaf value: an immediate point must not outweigh an avoidable strike or the loss of a
    /// critical card.
    pub fn utility(&self) -> i32 {
        self.score as i32 * 10 + self.weighted()
    }
}

impl PartialOrd for Worth {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(
            self.score
                .cmp(&other.score)
                .then(self.weighted().cmp(&other.weighted()))
                .then(self.plays.cmp(&other.plays)),
        )
    }
}

/// Simulated play/discard/non-play of a seat, for the views of a forecast.
pub fn card_event(view: &View, seat: Seat, card: CardId, play: bool) -> Event {
    let identity = view.identity(card);
    if play {
        let success = identity.is_some_and(|i| view.playable_set().contains(i));
        Event::Play {
            seat,
            card,
            identity,
            success,
        }
    } else {
        Event::Discard {
            seat,
            card,
            identity,
        }
    }
}

fn play_on(stacks: &mut Stacks, view: &View, identity: Identity) -> bool {
    if !view.variant.playable(stacks).contains(identity) {
        return false;
    }
    let suit = view.variant.suit_of(identity);
    let Stack::Up(top) = stacks[suit];
    stacks[suit] = Stack::Up(top + 1);
    true
}

impl View {
    /// Grade the current position from this view (normally ours, after our candidate clue).
    pub fn worth(&self, extra_error: u8) -> Worth {
        let mut discard_risks: i8 = 0;
        let mut clued = 0;
        let mut saved = 0;
        let mut plays = 0;
        let mut errors = extra_error as u16;
        let mut finesse_cards: SmallVec<[CardId; 20]> = SmallVec::new();
        let mut layer_cards: SmallVec<[CardId; 20]> = SmallVec::new();
        for record in &self.interpretations.records {
            let Some(holder) = self.state.holder(record.focus) else {
                continue;
            };
            if holder == self.me {
                continue;
            }
            let Some(truth) = self.true_hypothesis(record) else {
                errors += 2;
                continue;
            };
            if !self.resolves_before_receiver(record, truth.focus_identity) {
                errors += 2;
            }
            let mut projected = self.state.stacks;
            for step in truth.pending_steps() {
                if step.kind == StepKind::Finesse
                    && step.holder != self.me
                    && !finesse_cards.contains(&step.card)
                {
                    finesse_cards.push(step.card);
                    // Finesses implicitly touch their connecting cards. A clue must not also
                    // protect another copy.
                    if self.others().any(|seat| {
                        self.state.hand(seat).iter().any(|&c| {
                            c != step.card
                                && self.note(c).clued
                                && !self.note(c).trash
                                && self.identity(c) == Some(step.identity)
                        })
                    }) {
                        errors += 2;
                    }
                }
                let layers = self.layer_cards(record, truth, step);
                if step.holder != self.me && layers.len() != step.layers as usize {
                    errors += 2;
                }
                for card in layers {
                    match self.identity(card) {
                        Some(identity) if play_on(&mut projected, self, identity) => {}
                        _ => errors += 2,
                    }
                    if !layer_cards.contains(&card) {
                        layer_cards.push(card);
                    }
                }
                if !play_on(&mut projected, self, step.identity) {
                    errors += 2;
                }
                if step.kind == StepKind::Finesse
                    && step.holder == record.giver
                    && record.turn == self.turn
                {
                    errors += 2;
                }
            }
        }
        let finesses = finesse_cards.len() as u8;
        plays += layer_cards.len() as u8;
        let mut bonus = 0;
        for seat in self.others() {
            let mut queued_actions = 0;
            let chop = self.chop(seat);
            let mut discard_risk: i8 = 0;
            for &card in self.state.hand(seat) {
                let Some(identity) = self.identity(card) else {
                    continue;
                };
                let note = self.note(card);
                let state = self.play_state(identity);
                let info = self.info[identity.index()];
                if note.clued {
                    clued += 1;
                    // Credit lasting protection of a critical card even while its holder has
                    // a play.
                    if !note.trash
                        && matches!(state, PlayState::Critical | PlayState::CriticalPlayable)
                    {
                        saved += 1;
                    }
                    if note.play || note.delayed > 0 {
                        plays += 1;
                        queued_actions += 1;
                    }
                    if note.trash {
                        let error = match state {
                            PlayState::Trash | PlayState::Dead => {
                                queued_actions += 1;
                                None
                            }
                            PlayState::Critical | PlayState::CriticalPlayable => Some(3),
                            PlayState::Playable => Some(2),
                            PlayState::Normal => Some(1),
                        };
                        if let Some(error) = error {
                            let duplicated_self =
                                info.locked.is_some_and(|(locked_seat, locked_card)| {
                                    locked_seat == seat
                                        && locked_card != card
                                        && note.belief.size() == 0
                                });
                            if !duplicated_self && !self.protected_duplicate(seat, card, identity) {
                                errors += error;
                            }
                        }
                    } else if matches!(state, PlayState::Trash | PlayState::Dead) {
                        errors += 2;
                    }
                } else {
                    if chop == Some(card) && info.clued.is_none() {
                        match state {
                            PlayState::Critical | PlayState::CriticalPlayable => {
                                discard_risk -= CRITICAL_CARD_VALUE as i8
                            }
                            PlayState::Playable => discard_risk -= 2,
                            _ => {}
                        }
                    }
                    if note.promised.is_some() {
                        queued_actions += 1;
                        if !note.belief.contains(identity) {
                            errors += 2;
                        }
                    }
                }
                if note.play && note.delayed == 0 {
                    errors += match state {
                        PlayState::Playable | PlayState::CriticalPlayable => 0,
                        PlayState::Critical => 3,
                        PlayState::Normal => 2,
                        PlayState::Dead | PlayState::Trash => 1,
                    };
                }
                if !note.trash
                    && (info.locked.unwrap_or((seat, card)) == (seat, card)
                        || note.belief.size() > 0)
                    && !note.belief.contains(identity)
                    && !self.interpretations.records.iter().any(|record| {
                        record.focus == card && self.resolves_before_receiver(record, identity)
                    })
                {
                    errors += match state {
                        PlayState::Playable | PlayState::Normal => 2,
                        PlayState::Critical | PlayState::CriticalPlayable => 3,
                        PlayState::Dead | PlayState::Trash => 1,
                    };
                }
                if note.belief.size() == 1 {
                    bonus += 1;
                }
            }
            if discard_risk != 0 && queued_actions < 1 {
                discard_risks += discard_risk;
            }
        }
        Worth {
            score: self.score(),
            clued,
            saved,
            finesses,
            plays,
            discard_risks,
            errors: errors.min(u8::MAX as u16) as u8,
            bonus,
        }
    }

    /// Apply our own candidate clue to this view. Returns the convention errors it makes,
    /// or `None` if it touches nothing.
    pub fn give_clue(&mut self, receiver: Seat, clue: ClueId) -> Option<u8> {
        let touched = self.visible_touched(receiver, clue);
        if touched.is_empty() {
            return None;
        }
        let event = Event::Clue {
            giver: self.me,
            target: receiver,
            clue,
            touched,
        };
        Some(self.clued(self.me, receiver, clue, touched, &event))
    }

    /// Our candidate clue as every seat would read it. `views[k]` belongs to the seat `k`
    /// places after us. `accept` can reject the optimistic grade before the (expensive)
    /// views of the other seats are built.
    pub fn clue_views(
        &self,
        receiver: Seat,
        clue: ClueId,
        accept: impl FnOnce(&View, &Worth) -> bool,
    ) -> Option<(Worth, Vec<View>)> {
        let mut after = self.detached();
        let error = after.give_clue(receiver, clue)?;
        let mut worth = after.worth(error);
        if !accept(&after, &worth) {
            return None;
        }
        let touched = self.visible_touched(receiver, clue);
        let event = Event::Clue {
            giver: self.me,
            target: receiver,
            clue,
            touched,
        };
        let mut views = Vec::with_capacity(self.num_seats() as usize);
        views.push(after.clone());
        for seat in self.others() {
            let mut view = self.perspective(seat);
            view.clued(self.me, receiver, clue, touched, &event);
            views.push(view);
        }
        if !after.first_round_is_safe(&views) {
            worth.errors = worth.errors.saturating_add(2);
        }
        Some((worth, views))
    }

    /// Check the next turn of each other seat using what that seat can see. A valid-looking
    /// chain can still cause an immediate misplay when another holder reads the clue
    /// differently.
    fn first_round_is_safe(&self, initial: &[View]) -> bool {
        let mut views = initial.to_vec();
        let max = self.variant.tokens.max;
        for (offset, seat) in self.others().enumerate() {
            let k = offset + 1;
            if views[k].state.tokens < max
                && let Some(card) = views[k].sarcastic_discard()
            {
                let event = card_event(self, seat, card, false);
                for view in &mut views {
                    view.observe(&event);
                }
                continue;
            }
            if let Some(card) = views[k].next_play() {
                let Some(identity) = self.identity(card) else {
                    continue;
                };
                if !views[0].playable_set().contains(identity) {
                    return false;
                }
                let event = Event::Play {
                    seat,
                    card,
                    identity: Some(identity),
                    success: true,
                };
                for view in &mut views {
                    view.observe(&event);
                }
            } else {
                for view in &mut views {
                    if view.interpretations.on_non_play(seat) {
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
                if self.state.holder(record.focus).is_none() {
                    return true;
                }
                let Some(identity) = self.identity(record.focus) else {
                    return true;
                };
                views[0].state.holder(record.focus).is_none()
                    || views[0].play_state(identity) == PlayState::Trash
                    || views[0].interpretations.records.iter().any(|remaining| {
                        remaining.focus == record.focus
                            && remaining.turn == record.turn
                            && remaining.hypotheses.iter().any(|h| {
                                h.focus_identity == identity && views[0].matches_visible_hand(h)
                            })
                    })
            })
    }

    /// Copies of `identity` this view can see (card counting).
    pub fn known_copies(&self, identity: Identity) -> u8 {
        self.known_counts()[identity.index()]
    }

    /// Copies of every identity this view can see.
    pub fn known_counts(&self) -> [u8; crate::domain::MAX_IDENTITIES] {
        let mut counts = [0u8; crate::domain::MAX_IDENTITIES];
        let mine = self.state.hand_set(self.me);
        for card in self.state.dealt().iter() {
            if !mine.contains(card)
                && let Some(identity) = self.known.get(card)
            {
                counts[identity.index()] += 1;
            }
        }
        counts
    }
}
