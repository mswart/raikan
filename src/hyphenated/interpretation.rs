//! Tracking of play clue interpretations.
//!
//! Every play clue creates a [`ClueRecord`] holding all [`Hypothesis`]es
//! (possible meanings) the clue could have from the perspective of the line
//! owner. Each hypothesis is an ordered chain of [`Step`]s: the connecting
//! cards which have to be played (by prompts, finesses, already queued
//! cards, ...) before the focused card itself becomes playable.
//!
//! Following actions (and non-actions!) narrow the set of hypotheses down:
//!
//! * a play of the expected card fulfils a step
//! * a successful blind play of a *different* card on a finesse position is a
//!   layered finesse: the finesse target moves to the next finesse position
//! * a misplay or a contradicting play kills a hypothesis
//! * a player who discards or clues while one of their blind plays is due
//!   tells everybody that they are *not* finessed (non-play elimination)
//!
//! Hypotheses carry an Occam's razor [`Tier`]: only hypotheses of the lowest
//! tier with live hypotheses are *active*. When e.g. the finessed player does
//! not play, all hypotheses relying on their finesse die and the
//! self-finesse interpretation (higher tier) becomes active.

use crate::{CardQuantum, game};

use super::slot::SlotId;

/// Occam's razor ordering of interpretations
/// <https://hanabi.github.io/docs/level-10#clue-interpretation--occams-razor>
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// direct play, delayed play, prompts and finesses on other hands
    Direct,
    /// requires the clue receiver to play into own clued cards
    SelfPrompt,
    /// requires the clue receiver to blind play own cards
    SelfFinesse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// card was already promised to play by an earlier clue
    Queued,
    /// clued card which the holder plays into because of this clue
    Prompt,
    /// unclued card on the holder's finesse position (blind play)
    Finesse,
    /// the focused card of the clue itself (always the last step)
    Focus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// card this step has to deliver
    pub card: game::Card,
    /// player (relative to the line owner) holding the card
    pub holder: u8,
    pub kind: StepKind,
    /// slot expected to contain the card. For finesses it moves on layered plays.
    pub slot: SlotId,
    /// number of (playable) unclued cards in front of the target which the
    /// holder will blind play first (layered finesse). Only known to players
    /// seeing the holder's hand.
    pub layers: u8,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hypothesis {
    pub focus_card: game::Card,
    /// steps in play order; the last step is the focus card
    pub steps: Vec<Step>,
    pub tier: Tier,
}

impl Hypothesis {
    /// The next step that has to happen. Steps form a chain (rank n+1 can
    /// only be played after rank n), so only the first pending step can be
    /// acted upon right now.
    pub fn expected_now(&self) -> Option<&Step> {
        self.steps.iter().find(|step| !step.done)
    }

    /// Number of not yet completed steps before the step on `slot`
    pub fn pending_before(&self, slot: SlotId) -> Option<u8> {
        let mut pending = 0;
        for step in self.steps.iter() {
            if step.slot == slot && !step.done {
                return Some(pending);
            }
            if !step.done {
                pending += 1;
            }
        }
        None
    }

    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|step| step.done)
    }

    pub fn pending_steps(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|step| !step.done)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClueRecord {
    pub turn: i8,
    pub giver: u8,
    pub receiver: u8,
    pub focus: SlotId,
    pub hypotheses: Vec<Hypothesis>,
}

impl ClueRecord {
    /// Lowest tier with live hypotheses
    pub fn active_tier(&self) -> Option<Tier> {
        self.hypotheses.iter().map(|h| h.tier).min()
    }

    /// Hypotheses which are currently considered (Occam's razor)
    pub fn active(&self) -> impl Iterator<Item = &Hypothesis> {
        let tier = self.active_tier();
        let direct = self.hypotheses.iter().any(|h| {
            Some(h.tier) == tier && h.expected_now().is_some_and(|s| s.kind == StepKind::Focus)
        });
        self.hypotheses.iter().filter(move |h| {
            Some(h.tier) == tier && !(direct && h.pending_steps().any(|s| s.layers > 0))
        })
    }

    /// Single remaining hypothesis (if the clue is fully disambiguated)
    pub fn resolved(&self) -> Option<&Hypothesis> {
        if self.hypotheses.len() == 1 {
            self.hypotheses.first()
        } else {
            None
        }
    }
}

/// Minimal information about a hand card needed to move finesse targets.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HandEntry {
    pub id: SlotId,
    pub clued: bool,
    pub quantum: CardQuantum,
    /// turn the card was drawn
    pub turn: i8,
}

/// Records removed by an update
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// all hypotheses were contradicted
    pub dead: Vec<ClueRecord>,
    /// the single remaining hypothesis was played out completely
    pub completed: Vec<ClueRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interpretations {
    pub records: Vec<ClueRecord>,
}

impl Interpretations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, record: ClueRecord) {
        if !record.hypotheses.is_empty() {
            self.records.push(record);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Slots reserved as finesse targets by any pending step, excluding one record
    fn reserved_finesse_slots(&self, skip_record: usize) -> Vec<SlotId> {
        let mut reserved = Vec::new();
        for (index, record) in self.records.iter().enumerate() {
            if index == skip_record {
                continue;
            }
            for hypothesis in record.hypotheses.iter() {
                for step in hypothesis.pending_steps() {
                    if step.kind == StepKind::Finesse {
                        reserved.push(step.slot);
                    }
                }
            }
        }
        reserved
    }

    /// `player` played the card `card` from slot `slot`.
    /// `hand_after` is the player's hand (newest first) after the card was
    /// removed but before a replacement was drawn.
    pub fn on_play(
        &mut self,
        player: u8,
        slot: SlotId,
        card: game::Card,
        successful: bool,
        hand_after: &[HandEntry],
    ) -> Outcome {
        for record_index in 0..self.records.len() {
            let reserved = self.reserved_finesse_slots(record_index);
            let record = &mut self.records[record_index];
            let record_turn = record.turn;
            let matches_play = |step: &Step| {
                (step.slot == slot && step.card == card)
                    || (step.holder == player
                        && step.layers > 0
                        && hand_after.iter().any(|entry| entry.id == step.slot)
                        && remaining_layers(step, hand_after, record_turn, &reserved) < step.layers)
            };
            let confirmed = successful
                && record.hypotheses.iter().any(|h| {
                    (Some(h.tier) == record.active_tier()
                        || record
                            .active()
                            .any(|active| active.focus_card == h.focus_card))
                        && h.expected_now()
                            .is_some_and(|s| s.kind == StepKind::Finesse && matches_play(s))
                });
            record.hypotheses.retain_mut(|hypothesis| {
                if confirmed && !hypothesis.pending_steps().any(&matches_play) {
                    return false;
                }
                apply_play(
                    hypothesis,
                    player,
                    slot,
                    card,
                    successful,
                    hand_after,
                    record_turn,
                    &reserved,
                )
            });
        }
        self.prune()
    }

    /// The card on `slot` was discarded
    pub fn on_discard(&mut self, slot: SlotId) -> Outcome {
        for record in self.records.iter_mut() {
            record
                .hypotheses
                .retain(|hypothesis| !hypothesis.pending_steps().any(|step| step.slot == slot));
        }
        self.prune()
    }

    /// Cards were touched by a clue. Touched finesse targets become prompts.
    pub fn on_clue(&mut self, touched: &[SlotId]) {
        for record in self.records.iter_mut() {
            for hypothesis in record.hypotheses.iter_mut() {
                for step in hypothesis.steps.iter_mut() {
                    if !step.done && step.kind == StepKind::Finesse && touched.contains(&step.slot)
                    {
                        step.kind = StepKind::Prompt;
                        step.layers = 0;
                    }
                }
            }
        }
    }

    /// `player` used their turn for something else than a play (clue or
    /// discard). Eliminate active meanings whose next blind play belongs to
    /// them. Later links and inactive fallback meanings are not obligations.
    pub fn on_non_play(&mut self, player: u8) -> Outcome {
        for record in self.records.iter_mut() {
            // Only blind obligations compel a play: a clued card can be held
            // while its owner waits for another player's interpretation.
            // Urgent saves can exempt blind obligations once supported.
            let rejected: Vec<_> = record
                .active()
                .filter(|hypothesis| {
                    hypothesis
                        .expected_now()
                        .is_some_and(|step| step.holder == player && step.kind == StepKind::Finesse)
                })
                .cloned()
                .collect();
            record
                .hypotheses
                .retain(|hypothesis| !rejected.contains(hypothesis));
        }
        self.prune()
    }

    /// Drop hypotheses for which `consistent` returns false for any pending
    /// step (e.g. because a fix clue revealed the card is something else).
    pub fn retain_consistent<F: FnMut(&Step) -> bool>(&mut self, mut consistent: F) -> Outcome {
        for record in self.records.iter_mut() {
            record
                .hypotheses
                .retain(|hypothesis| hypothesis.pending_steps().all(&mut consistent));
        }
        self.prune()
    }

    /// Remove dead (no hypothesis left) and completed records
    pub fn prune(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        let mut index = 0;
        while index < self.records.len() {
            let record = &self.records[index];
            if record.hypotheses.is_empty() {
                outcome.dead.push(self.records.remove(index));
            } else if record.hypotheses.iter().all(|h| h.is_complete()) {
                outcome.completed.push(self.records.remove(index));
            } else {
                index += 1;
            }
        }
        outcome
    }

    pub fn iter_active(&self) -> impl Iterator<Item = (&ClueRecord, &Hypothesis)> {
        self.records
            .iter()
            .flat_map(|record| record.active().map(move |h| (record, h)))
    }
}

fn remaining_layers(step: &Step, hand: &[HandEntry], turn: i8, reserved: &[SlotId]) -> u8 {
    hand.iter()
        .take_while(|entry| entry.id != step.slot)
        .filter(|entry| !entry.clued && entry.turn <= turn && !reserved.contains(&entry.id))
        .count() as u8
}

/// Apply a play to a single hypothesis. Returns whether it is still alive.
#[allow(clippy::too_many_arguments)]
fn apply_play(
    hypothesis: &mut Hypothesis,
    player: u8,
    slot: SlotId,
    card: game::Card,
    successful: bool,
    hand_after: &[HandEntry],
    record_turn: i8,
    reserved: &[SlotId],
) -> bool {
    let mut layered = false;
    for step in hypothesis.steps.iter_mut() {
        if step.done {
            continue;
        }
        if step.slot == slot {
            if successful && step.card == card {
                step.done = true;
            } else if successful
                && matches!(
                    step.kind,
                    StepKind::Finesse | StepKind::Prompt | StepKind::Queued
                )
            {
                // blind play of another playable card: layered finesse
                layered = true;
            } else {
                // misplay or card is not what this hypothesis requires
                return false;
            }
        } else if successful && step.card == card {
            // Playing this identity elsewhere contradicts its assigned slot.
            return false;
        } else if step.holder == player
            && step.kind == StepKind::Finesse
            && step.layers > 0
            && hand_after.iter().any(|entry| entry.id == step.slot)
        {
            // observer view: a layer in front of the target got played
            let remaining = remaining_layers(step, hand_after, record_turn, reserved);
            if !successful && remaining < step.layers {
                return false;
            }
            step.layers = step.layers.min(remaining);
        }
    }
    if layered {
        return retarget_finesses(hypothesis, player, slot, hand_after, record_turn, reserved);
    }
    true
}

/// After a layered blind play of `played_slot` move all pending finesse
/// steps of `holder` to the next finesse positions (in order).
fn retarget_finesses(
    hypothesis: &mut Hypothesis,
    holder: u8,
    played_slot: SlotId,
    hand_after: &[HandEntry],
    record_turn: i8,
    reserved: &[SlotId],
) -> bool {
    let Some(index) = hypothesis
        .steps
        .iter()
        .position(|s| !s.done && s.slot == played_slot)
    else {
        return true;
    };
    let expected = hypothesis.steps[index].card;
    let prompt = hypothesis.steps[index].kind != StepKind::Finesse;
    let mut used = reserved.to_vec();
    for step in &hypothesis.steps[..index] {
        if !step.done {
            used.push(step.slot);
        }
    }
    for step in &mut hypothesis.steps[index..] {
        if step.done
            || step.holder != holder
            || (step.slot != played_slot && step.kind != StepKind::Finesse)
        {
            continue;
        }
        let candidate = hand_after
            .iter()
            .filter(|entry| {
                entry.turn <= record_turn
                    && !used.contains(&entry.id)
                    && entry.quantum.contains_hard(&step.card)
            })
            .find(|entry| prompt && step.card == expected && entry.clued)
            .or_else(|| {
                hand_after.iter().find(|entry| {
                    !entry.clued
                        && entry.turn <= record_turn
                        && !used.contains(&entry.id)
                        && entry.quantum.contains_hard(&step.card)
                })
            });
        let Some(entry) = candidate else {
            return false;
        };
        step.slot = entry.id;
        step.kind = if entry.clued {
            StepKind::Prompt
        } else {
            StepKind::Finesse
        };
        step.layers = step.layers.saturating_sub(1);
        used.push(entry.id);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(suit: game::Suit, rank: u8) -> game::Card {
        game::Card { suit, rank }
    }

    fn step(c: game::Card, holder: u8, kind: StepKind, slot: SlotId) -> Step {
        Step {
            card: c,
            holder,
            kind,
            slot,
            layers: 0,
            done: false,
        }
    }

    fn hand(ids: &[(SlotId, bool)]) -> Vec<HandEntry> {
        ids.iter()
            .map(|(id, clued)| HandEntry {
                id: *id,
                clued: *clued,
                quantum: CardQuantum::new(crate::card_quantum::Variant {}),
                turn: 0,
            })
            .collect()
    }

    const R: game::Suit = game::Suit::Red();
    const B: game::Suit = game::Suit::Blue();

    /// Bob (1) is finessed for r1 (slot 10), our (0) slot 1 is r2.
    /// Alternative: we self-finesse r1 on slot 3.
    fn finesse_record() -> ClueRecord {
        ClueRecord {
            turn: 5,
            giver: 2,
            receiver: 0,
            focus: 1,
            hypotheses: vec![
                Hypothesis {
                    focus_card: card(R, 2),
                    steps: vec![
                        step(card(R, 1), 1, StepKind::Finesse, 10),
                        step(card(R, 2), 0, StepKind::Focus, 1),
                    ],
                    tier: Tier::Direct,
                },
                Hypothesis {
                    focus_card: card(R, 2),
                    steps: vec![
                        step(card(R, 1), 0, StepKind::Finesse, 3),
                        step(card(R, 2), 0, StepKind::Focus, 1),
                    ],
                    tier: Tier::SelfFinesse,
                },
            ],
        }
    }

    #[test]
    fn expected_now_and_pending_before() {
        let record = finesse_record();
        let h = &record.hypotheses[0];
        assert_eq!(h.expected_now().unwrap().slot, 10);
        assert_eq!(h.pending_before(1), Some(1));
        assert_eq!(h.pending_before(10), Some(0));
        assert_eq!(h.pending_before(42), None);
        assert_eq!(record.active_tier(), Some(Tier::Direct));
        assert_eq!(record.active().count(), 1);
    }

    #[test]
    fn play_fulfills_step_and_completes() {
        let mut i = Interpretations::new();
        i.add(finesse_record());
        let outcome = i.on_play(1, 10, card(R, 1), true, &hand(&[(11, false)]));
        assert!(outcome.dead.is_empty());
        // The play rules out the alternative which assigned r1 to our hand.
        let record = &i.records[0];
        assert!(record.hypotheses[0].steps[0].done);
        assert_eq!(record.hypotheses[0].pending_before(1), Some(0));
        let outcome = i.on_play(0, 1, card(R, 2), true, &hand(&[(3, false)]));
        assert_eq!(outcome.completed.len(), 1);
        assert!(i.is_empty());
    }

    #[test]
    fn non_play_switches_to_self_finesse() {
        let mut i = Interpretations::new();
        i.add(finesse_record());
        // somebody else not playing does not change anything
        i.on_non_play(2);
        assert_eq!(i.records[0].hypotheses.len(), 2);
        // Bob discards instead of blind playing: he isn't finessed
        let outcome = i.on_non_play(1);
        assert!(outcome.dead.is_empty());
        let record = &i.records[0];
        assert_eq!(record.active_tier(), Some(Tier::SelfFinesse));
        assert_eq!(record.resolved().unwrap().steps[0].holder, 0);
    }

    #[test]
    fn non_play_ignores_inactive_hypotheses() {
        let mut i = Interpretations::new();
        i.add(finesse_record());
        // we are not expected to play yet (only in the inactive tier)
        i.on_non_play(0);
        assert_eq!(i.records[0].hypotheses.len(), 2);
    }

    #[test]
    fn layered_blind_play_moves_target() {
        // we (0) are finessed for r1 on slot 3; slot 4 is the next unclued card
        let mut i = Interpretations::new();
        i.add(ClueRecord {
            turn: 5,
            giver: 1,
            receiver: 2,
            focus: 20,
            hypotheses: vec![Hypothesis {
                focus_card: card(R, 2),
                steps: vec![
                    step(card(R, 1), 0, StepKind::Finesse, 3),
                    step(card(R, 2), 2, StepKind::Focus, 20),
                ],
                tier: Tier::Direct,
            }],
        });
        // blind play turns out to be b1
        let outcome = i.on_play(0, 3, card(B, 1), true, &hand(&[(5, true), (4, false)]));
        assert!(outcome.dead.is_empty());
        let h = &i.records[0].hypotheses[0];
        assert!(!h.steps[0].done);
        assert_eq!(h.steps[0].slot, 4, "finesse moved to next unclued card");
        // a misplay kills the finesse
        let outcome = i.on_play(0, 4, card(B, 3), false, &hand(&[(5, true)]));
        assert_eq!(outcome.dead.len(), 1);
    }

    #[test]
    fn layered_retarget_keeps_order_and_reservations() {
        // two blind plays (r1, r2) by player 0; slot 6 reserved by another clue
        let mut i = Interpretations::new();
        i.add(ClueRecord {
            turn: 1,
            giver: 3,
            receiver: 0,
            focus: 30,
            hypotheses: vec![Hypothesis {
                focus_card: card(B, 1),
                steps: vec![
                    step(card(B, 1), 0, StepKind::Finesse, 6),
                    step(card(B, 1), 1, StepKind::Focus, 30),
                ],
                tier: Tier::Direct,
            }],
        });
        i.add(ClueRecord {
            turn: 5,
            giver: 1,
            receiver: 2,
            focus: 20,
            hypotheses: vec![Hypothesis {
                focus_card: card(R, 3),
                steps: vec![
                    step(card(R, 1), 0, StepKind::Finesse, 3),
                    step(card(R, 2), 0, StepKind::Finesse, 4),
                    step(card(R, 3), 2, StepKind::Focus, 20),
                ],
                tier: Tier::Direct,
            }],
        });
        i.on_play(
            0,
            3,
            card(game::Suit::Green(), 1),
            true,
            &hand(&[(4, false), (6, false), (7, false)]),
        );
        let steps = &i.records[1].hypotheses[0].steps;
        assert_eq!(steps[0].slot, 4);
        assert_eq!(steps[1].slot, 7, "slot 6 is reserved by the other clue");
    }

    #[test]
    fn observers_track_only_layers_and_abort_a_bombed_layer() {
        let mut record = finesse_record();
        record.hypotheses.truncate(1);
        record.hypotheses[0].steps[0].layers = 1;
        let mut i = Interpretations {
            records: vec![record],
        };
        // An unrelated play behind the target leaves the layer intact.
        i.on_play(1, 12, card(B, 1), true, &hand(&[(11, false), (10, false)]));
        assert_eq!(i.records[0].hypotheses[0].steps[0].layers, 1);
        // The blind layer in front of the target bombs: observers abandon it too.
        let outcome = i.on_play(1, 11, card(B, 4), false, &hand(&[(10, false)]));
        assert_eq!(outcome.dead.len(), 1);
        assert!(i.is_empty());
    }

    #[test]
    fn layered_retarget_skips_new_draws_and_incompatible_prompts() {
        let mut record = finesse_record();
        record.hypotheses.truncate(1);
        record.hypotheses[0].steps[0].kind = StepKind::Prompt;
        let mut after = hand(&[(11, false), (12, true), (13, true)]);
        after[0].turn = 6;
        after[1].quantum.limit_by_rank(3, true);
        after[2].quantum.limit_by_rank(1, true);
        let mut i = Interpretations {
            records: vec![record],
        };
        i.on_play(1, 10, card(B, 1), true, &after);
        assert_eq!(i.records[0].hypotheses[0].steps[0].slot, 13);
    }

    #[test]
    fn focus_card_played_elsewhere_kills_hypothesis() {
        let mut i = Interpretations::new();
        let mut record = finesse_record();
        record.hypotheses.push(Hypothesis {
            focus_card: card(B, 1),
            steps: vec![step(card(B, 1), 0, StepKind::Focus, 1)],
            tier: Tier::Direct,
        });
        i.add(record);
        i.on_play(3, 50, card(B, 1), true, &[]);
        assert!(
            i.records[0]
                .hypotheses
                .iter()
                .all(|h| h.focus_card != card(B, 1))
        );
    }

    #[test]
    fn discard_and_clue_updates() {
        let mut i = Interpretations::new();
        i.add(finesse_record());
        i.on_clue(&[10]);
        assert_eq!(i.records[0].hypotheses[0].steps[0].kind, StepKind::Prompt);
        i.on_discard(3);
        assert_eq!(i.records[0].hypotheses.len(), 1);
        let outcome = i.on_discard(10);
        assert_eq!(outcome.dead.len(), 1);
    }

    #[test]
    fn retain_consistent_drops_contradicted() {
        let mut i = Interpretations::new();
        i.add(finesse_record());
        i.retain_consistent(|step| step.slot != 3);
        assert_eq!(i.records[0].hypotheses.len(), 1);
        assert_eq!(i.records[0].hypotheses[0].tier, Tier::Direct);
    }
}
