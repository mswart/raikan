//! Clue records and their readings, and how open readings react to later events.
//!
//! A record keeps every reading (hypothesis) of a play clue that is still consistent, ordered
//! by Occam tier. The lowest live tier is active; when all its readings die, the next tier
//! becomes active again.

use smallvec::SmallVec;

use super::belief::Belief;
use crate::domain::{CardId, Identity, IdentitySet, Seat};

/// Occam ordering of readings: prefer explanations that need fewer blind plays from the
/// receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Direct,
    SelfPrompt,
    SelfFinesse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepKind {
    /// Already queued to play by an earlier clue or known card.
    Queued,
    Prompt,
    Finesse,
    /// The clued card itself.
    Focus,
}

/// One connecting play of a reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    pub identity: Identity,
    pub holder: Seat,
    pub kind: StepKind,
    pub card: CardId,
    /// For a prompt: the finesse position to use instead if the prompt turns out wrong.
    pub fallback: Option<CardId>,
    /// Cards that must be played blind before this one (layered finesse).
    pub layers: u8,
    pub done: bool,
    /// A prompt on a card its holder already knew to play: that play would happen anyway,
    /// so it confirms nothing about this reading.
    pub known_play: bool,
}

/// One reading of a play clue.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hypothesis {
    pub focus_identity: Identity,
    pub steps: SmallVec<[Step; 5]>,
    pub tier: Tier,
}

impl Hypothesis {
    pub fn expected_now(&self) -> Option<&Step> {
        self.steps.iter().find(|step| !step.done)
    }

    /// Number of pending steps before the step on `card`.
    pub fn pending_before(&self, card: CardId) -> Option<u8> {
        let mut pending = 0;
        for step in &self.steps {
            if step.card == card && !step.done {
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

    pub fn pending_steps(&self) -> impl Iterator<Item = &Step> + Clone {
        self.steps.iter().filter(|step| !step.done)
    }
}

/// The record of one play clue: who gave it, which card it focused and its live readings.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClueRecord {
    pub turn: i16,
    pub giver: Seat,
    pub receiver: Seat,
    pub focus: CardId,
    pub hypotheses: Vec<Hypothesis>,
}

#[derive(Clone, Copy)]
pub(super) struct ActiveMeanings {
    tier: Option<Tier>,
    direct: bool,
}

impl ActiveMeanings {
    pub(super) fn contains(self, hypothesis: &Hypothesis) -> bool {
        Some(hypothesis.tier) == self.tier
            && !(self.direct && hypothesis.pending_steps().any(|s| s.layers > 0))
    }
}

impl ClueRecord {
    pub fn active_tier(&self) -> Option<Tier> {
        self.hypotheses.iter().map(|h| h.tier).min()
    }

    pub fn active(&self) -> impl Iterator<Item = &Hypothesis> + Clone {
        let active = self.active_meanings();
        self.hypotheses.iter().filter(move |h| active.contains(h))
    }

    pub(super) fn active_meanings(&self) -> ActiveMeanings {
        let tier = self.active_tier();
        let direct = self.hypotheses.iter().any(|h| {
            Some(h.tier) == tier && h.expected_now().is_some_and(|s| s.kind == StepKind::Focus)
        });
        ActiveMeanings { tier, direct }
    }

    /// The readings `seat` considers. Observers who can see the focus card condition on
    /// it; the receiver cannot.
    pub fn for_seat(
        &self,
        seat: Seat,
        known: Option<Identity>,
    ) -> impl Iterator<Item = &Hypothesis> + Clone {
        let known = known.filter(|_| seat != self.receiver);
        let tier = if let Some(identity) = known {
            self.hypotheses
                .iter()
                .filter(|h| h.focus_identity == identity)
                .map(|h| h.tier)
                .min()
        } else {
            self.active_tier()
        };
        let direct = known.is_none()
            && self.hypotheses.iter().any(|h| {
                Some(h.tier) == tier && h.expected_now().is_some_and(|s| s.kind == StepKind::Focus)
            });
        self.hypotheses.iter().filter(move |h| {
            Some(h.tier) == tier
                && known.is_none_or(|identity| h.focus_identity == identity)
                && !(direct && h.pending_steps().any(|s| s.layers > 0))
        })
    }

    pub fn resolved(&self) -> Option<&Hypothesis> {
        if self.hypotheses.len() == 1 {
            self.hypotheses.first()
        } else {
            None
        }
    }
}

/// A card in a hand after a play, as reactions need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandEntry {
    pub card: CardId,
    pub clued: bool,
    pub belief: Belief,
    pub drawn_at: i16,
    pub chop_moved: bool,
    pub starting: bool,
    pub one_chop_focus: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Interpretations {
    pub records: Vec<ClueRecord>,
}

impl Interpretations {
    pub fn add(&mut self, record: ClueRecord) {
        if !record.hypotheses.is_empty() {
            self.records.push(record);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn reserved_finesse_cards(&self, skip_record: usize) -> SmallVec<[CardId; 20]> {
        let mut reserved = SmallVec::new();
        for (index, record) in self.records.iter().enumerate() {
            if index == skip_record {
                continue;
            }
            for hypothesis in &record.hypotheses {
                for step in hypothesis.pending_steps() {
                    if step.kind == StepKind::Finesse && !reserved.contains(&step.card) {
                        reserved.push(step.card);
                    }
                }
            }
        }
        reserved
    }

    /// React to a play: confirm, advance, decline or kill readings; retarget layered
    /// finesses when a blind play turned out to be a layer. `prompt_like` holds the
    /// identities the played card could be if it was a clued card: playing such a card is
    /// the holder answering a perceived prompt (a hidden finesse), not declining.
    #[allow(clippy::too_many_arguments)]
    pub fn on_play(
        &mut self,
        player: Seat,
        played: CardId,
        identity: Identity,
        successful: bool,
        hand_after: &[HandEntry],
        ones: Option<IdentitySet>,
        prompt_like: IdentitySet,
    ) {
        let prior_obligation = self
            .records
            .iter()
            .filter(|record| {
                record.active().any(|h| {
                    h.expected_now()
                        .is_some_and(|step| step.holder == player && step.card == played)
                })
            })
            .map(|record| record.turn)
            .min();
        for record_index in 0..self.records.len() {
            let needs_reservations = self.records[record_index].hypotheses.iter().any(|h| {
                h.pending_steps().any(|s| {
                    s.holder == player
                        && (s.layers > 0 || (s.card == played && s.identity != identity))
                })
            });
            let reserved = if needs_reservations {
                self.reserved_finesse_cards(record_index)
            } else {
                SmallVec::new()
            };
            let record = &mut self.records[record_index];
            let active = record.active_meanings();
            let record_turn = record.turn;
            let matches_play = |step: &Step| {
                (step.card == played && step.identity == identity)
                    || (step.holder == player
                        && step.layers > 0
                        && hand_after.iter().any(|entry| entry.card == step.card)
                        && remaining_layers(step, hand_after, record_turn, &reserved) < step.layers)
            };
            let active_tier = record.active_tier();
            let active_identities: SmallVec<[Identity; 4]> =
                record.active().map(|h| h.focus_identity).collect();
            let confirmed = successful
                && record.hypotheses.iter().any(|h| {
                    (Some(h.tier) == active_tier || active_identities.contains(&h.focus_identity))
                        && h.expected_now().is_some_and(|s| {
                            matches!(s.kind, StepKind::Finesse | StepKind::Prompt)
                                && !s.known_play
                                && matches_play(s)
                        })
                });
            let receiver = record.receiver;
            record.hypotheses.retain_mut(|hypothesis| {
                let declined = active.contains(hypothesis)
                    && successful
                    && prior_obligation.is_none()
                    && hypothesis.expected_now().is_some_and(|s| {
                        s.holder == player
                            && s.layers == 0
                            && (s.kind == StepKind::Finesse
                                || (s.kind == StepKind::Prompt && player != receiver))
                            && s.card != played
                            && !matches_play(s)
                            && !prompt_like.contains(s.identity)
                    });
                if declined {
                    return false;
                }
                if confirmed && !hypothesis.pending_steps().any(matches_play) {
                    return false;
                }
                apply_play(
                    hypothesis,
                    player,
                    played,
                    identity,
                    successful,
                    hand_after,
                    record_turn,
                    &reserved,
                    ones,
                )
            });
        }
        self.prune();
    }

    /// A discarded card can no longer fulfil any reading.
    pub fn on_discard(&mut self, card: CardId) {
        for record in &mut self.records {
            record
                .hypotheses
                .retain(|hypothesis| !hypothesis.pending_steps().any(|step| step.card == card));
        }
        self.prune();
    }

    /// A finessed card that gets clued becomes a prompt.
    pub fn on_clue(&mut self, touched: &[CardId]) {
        for record in &mut self.records {
            for hypothesis in &mut record.hypotheses {
                for step in &mut hypothesis.steps {
                    if !step.done && step.kind == StepKind::Finesse && touched.contains(&step.card)
                    {
                        step.kind = StepKind::Prompt;
                        step.layers = 0;
                    }
                }
            }
        }
    }

    /// A seat that was expected to play blind (or to answer a prompt as a third party) did
    /// something else: those readings die. Returns whether anything changed.
    pub fn on_non_play(&mut self, player: Seat) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            let active = record.active_meanings();
            let receiver = record.receiver;
            let previous = record.hypotheses.len();
            record.hypotheses.retain(|hypothesis| {
                !(active.contains(hypothesis)
                    && hypothesis.expected_now().is_some_and(|step| {
                        step.holder == player
                            && (step.kind == StepKind::Finesse
                                || (step.kind == StepKind::Prompt && player != receiver))
                    }))
            });
            changed |= previous != record.hypotheses.len();
        }
        changed |= self.prune();
        changed
    }

    pub fn retain_consistent<F: FnMut(&Step) -> bool>(&mut self, mut consistent: F) {
        for record in &mut self.records {
            record
                .hypotheses
                .retain(|hypothesis| hypothesis.pending_steps().all(&mut consistent));
        }
        self.prune();
    }

    /// Drop dead and completed records. Returns whether any record was dropped.
    pub fn prune(&mut self) -> bool {
        let before = self.records.len();
        self.records.retain(|record| {
            !record.hypotheses.is_empty() && !record.hypotheses.iter().all(|h| h.is_complete())
        });
        before != self.records.len()
    }

    pub fn iter_active(&self) -> impl Iterator<Item = (&ClueRecord, &Hypothesis)> {
        self.records
            .iter()
            .flat_map(|record| record.active().map(move |h| (record, h)))
    }
}

pub(super) fn remaining_layers(
    step: &Step,
    hand: &[HandEntry],
    turn: i16,
    reserved: &[CardId],
) -> u8 {
    hand.iter()
        .take_while(|entry| entry.card != step.card)
        .filter(|entry| {
            !entry.clued
                && !entry.chop_moved
                && entry.drawn_at <= turn
                && !reserved.contains(&entry.card)
        })
        .count() as u8
}

#[allow(clippy::too_many_arguments)]
fn apply_play(
    hypothesis: &mut Hypothesis,
    player: Seat,
    played: CardId,
    identity: Identity,
    successful: bool,
    hand_after: &[HandEntry],
    record_turn: i16,
    reserved: &[CardId],
    ones: Option<IdentitySet>,
) -> bool {
    let mut layered = false;
    for step in &mut hypothesis.steps {
        if step.done {
            continue;
        }
        if step.card == played {
            if successful && step.identity == identity {
                step.done = true;
            } else if successful
                && matches!(
                    step.kind,
                    StepKind::Finesse | StepKind::Prompt | StepKind::Queued
                )
            {
                layered = true;
            } else {
                return false;
            }
        } else if successful && step.identity == identity {
            return false;
        } else if step.holder == player
            && step.kind == StepKind::Finesse
            && step.layers > 0
            && hand_after.iter().any(|entry| entry.card == step.card)
        {
            let remaining = remaining_layers(step, hand_after, record_turn, reserved);
            if !successful && remaining < step.layers {
                return false;
            }
            step.layers = step.layers.min(remaining);
        }
    }
    if layered {
        return retarget_finesses(
            hypothesis,
            player,
            played,
            hand_after,
            record_turn,
            reserved,
            ones,
        );
    }
    true
}

/// `hand_after` is newest first (position 0 = newest), like hand positions.
fn retarget_finesses(
    hypothesis: &mut Hypothesis,
    holder: Seat,
    played: CardId,
    hand_after: &[HandEntry],
    record_turn: i16,
    reserved: &[CardId],
    ones: Option<IdentitySet>,
) -> bool {
    let Some(index) = hypothesis
        .steps
        .iter()
        .position(|s| !s.done && s.card == played)
    else {
        return true;
    };
    let expected = hypothesis.steps[index].identity;
    let prompt = hypothesis.steps[index].kind != StepKind::Finesse;
    let mut used = SmallVec::<[CardId; 20]>::from_slice(reserved);
    for step in &hypothesis.steps[..index] {
        if !step.done {
            used.push(step.card);
        }
    }
    for step in &mut hypothesis.steps[index..] {
        if step.done
            || step.holder != holder
            || (step.card != played && step.kind != StepKind::Finesse)
        {
            continue;
        }
        let rank_one =
            |entry: &HandEntry| ones.is_some_and(|ones| entry.belief.soft.is_subset(ones));
        let candidate = hand_after
            .iter()
            .filter(|entry| {
                entry.drawn_at <= record_turn
                    && !used.contains(&entry.card)
                    && entry.belief.contains_hard(step.identity)
            })
            .filter(|entry| prompt && step.identity == expected && entry.clued)
            .min_by_key(|entry| {
                let pos = hand_after
                    .iter()
                    .position(|other| other.card == entry.card)
                    .unwrap();
                if !ones.is_some_and(|ones| ones.contains(step.identity))
                    || !rank_one(entry)
                    || entry.one_chop_focus
                {
                    (0, pos)
                } else if !entry.starting {
                    (1, pos)
                } else {
                    (2, usize::MAX - pos)
                }
            })
            .or_else(|| {
                hand_after.iter().find(|entry| {
                    !entry.clued
                        && !entry.chop_moved
                        && entry.drawn_at <= record_turn
                        && !used.contains(&entry.card)
                        && entry.belief.contains_hard(step.identity)
                })
            });
        let Some(entry) = candidate else {
            return false;
        };
        step.card = entry.card;
        step.known_play = false;
        step.kind = if entry.clued {
            StepKind::Prompt
        } else {
            StepKind::Finesse
        };
        step.layers = step.layers.saturating_sub(1);
        used.push(entry.card);
    }
    true
}
