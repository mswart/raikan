//! Locator: finds the connecting cards of a play clue (queued, known, prompt, finesse by
//! seat with layers, own queued cards, self-prompt, self-finesse).

use smallvec::SmallVec;

use super::profile::Rule;
use super::records::{Hypothesis, Step, StepKind, Tier};
use super::view::{Clued, PlayState, View};
use crate::domain::{CardId, CardSet, Identity, Seat, SeatSet};

/// Which of the receiver's own cards a reading may use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FirstAction {
    NonSelf,
    SelfPrompt,
    SelfFinesse,
}

#[derive(Clone, Debug)]
pub(super) struct PlayEvaluation {
    giver: Seat,
    receiver: Seat,
    focus: CardId,
    identity: Identity,
    pub steps: SmallVec<[Step; 5]>,
    tier: Tier,
    marked: CardSet,
    excluded_finesses: SeatSet,
}

impl PlayEvaluation {
    fn add_step(
        &mut self,
        view: &View,
        identity: Identity,
        holder: Seat,
        card: CardId,
        kind: StepKind,
    ) {
        let fallback = (kind == StepKind::Prompt)
            .then(|| {
                view.newest_first(holder)
                    .find(|&(_, c)| {
                        let note = view.note(c);
                        !note.clued && note.promised.is_none() && !self.marked.contains(c)
                    })
                    .map(|(_, c)| c)
            })
            .flatten();
        self.steps.push(Step {
            identity,
            holder,
            kind,
            card,
            fallback,
            layers: 0,
            done: false,
        });
    }

    pub(super) fn into_hypothesis(mut self, view: &View) -> Hypothesis {
        self.steps
            .sort_by_key(|step| view.variant.rank_of(step.identity));
        Hypothesis {
            focus_identity: self.identity,
            steps: self.steps,
            tier: self.tier,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn test(
        view: &View,
        identity: Identity,
        giver: Seat,
        receiver: Seat,
        focus: CardId,
        premarked: CardSet,
        first_action: FirstAction,
        excluded_finesses: SeatSet,
    ) -> Option<Self> {
        let mut evaluation = Self {
            giver,
            receiver,
            focus,
            identity,
            steps: SmallVec::new(),
            tier: Tier::Direct,
            marked: CardSet::EMPTY,
            excluded_finesses,
        };
        evaluation
            .resolve(view, premarked, first_action)
            .then_some(evaluation)
    }

    /// Look for a card already clued in `holder`'s hand. `None`: not found but the giver
    /// cannot see it (assume a duplicate); `Some(false)`: the reading fails.
    fn find_clued_card(
        &mut self,
        view: &View,
        holder: Seat,
        previous: Identity,
        allowed: FirstAction,
    ) -> Option<bool> {
        let prompts = view.rule(Rule::Prompt);
        if let Some((_, card)) = view.newest_first(holder).find(|&(_, c)| {
            let note = view.note(c);
            !self.marked.contains(c)
                && (note.clued || note.promised.is_some())
                && view.identity(c) == Some(previous)
                && (note.play || note.delayed > 0)
        }) {
            self.marked.insert(card);
            self.add_step(view, previous, holder, card, StepKind::Queued);
            return Some(true);
        }
        let known_prompt = |this: &Self| {
            view.newest_first(holder).find(|&(_, c)| {
                let note = view.note(c);
                !this.marked.contains(c)
                    && note.clued
                    && note.belief.contains(previous)
                    && note.belief.size() == 1
                    && !note.play
                    && note.delayed == 0
            })
        };
        if holder == self.giver {
            if prompts && let Some((_, card)) = known_prompt(self) {
                self.marked.insert(card);
                self.add_step(view, previous, holder, card, StepKind::Prompt);
                return Some(true);
            }
            return None;
        }
        if holder == self.receiver && allowed == FirstAction::NonSelf {
            if prompts && let Some((_, card)) = known_prompt(self) {
                self.marked.insert(card);
                self.add_step(view, previous, holder, card, StepKind::Prompt);
                return Some(true);
            }
            return Some(false);
        }
        if prompts
            && let Some((_, card)) = view.newest_first(holder).find(|&(_, c)| {
                let note = view.note(c);
                !self.marked.contains(c)
                    && note.clued
                    && note.belief.contains(previous)
                    && !note.play
                    && note.delayed == 0
            })
        {
            self.marked.insert(card);
            self.add_step(view, previous, holder, card, StepKind::Prompt);
            return Some(true);
        }
        Some(false)
    }

    fn resolve(&mut self, view: &View, premarked: CardSet, first_action: FirstAction) -> bool {
        self.tier = match first_action {
            FirstAction::NonSelf => Tier::Direct,
            FirstAction::SelfPrompt => Tier::SelfPrompt,
            FirstAction::SelfFinesse => Tier::SelfFinesse,
        };
        self.marked = premarked;
        self.marked.insert(self.focus);
        self.add_step(
            view,
            self.identity,
            self.receiver,
            self.focus,
            StepKind::Focus,
        );
        let mut allowed = first_action;
        match view.play_state(self.identity) {
            PlayState::Playable | PlayState::CriticalPlayable => return true,
            PlayState::Trash | PlayState::Dead => return false,
            _ => {}
        }
        let me = view.me;
        let n = view.num_seats();
        let suit = view.variant.suit_of(self.identity);
        let finesses = view.rule(Rule::Finesse);
        let layered = view.rule(Rule::LayeredFinesse);
        let prompts = view.rule(Rule::Prompt);
        'rank_loop: for rank in 1..view.variant.rank_of(self.identity) {
            let previous = view.variant.identity(suit, rank);
            if view.play_state(previous) == PlayState::Trash {
                continue;
            }
            // An earlier clue may already reserve this connecting card, even while its other
            // meanings remain unresolved.
            let queued = (0..n).map(|offset| me.after(offset, n)).find_map(|holder| {
                view.state
                    .hand(holder)
                    .iter()
                    .rev()
                    .copied()
                    .find(|&c| {
                        let note = view.note(c);
                        note.promised.is_some()
                            && !self.marked.contains(c)
                            && note.belief.contains(previous)
                            && if holder == me {
                                note.belief.size() == 1
                            } else {
                                view.identity(c) == Some(previous)
                            }
                    })
                    .map(|c| (holder, c))
            });
            if let Some((holder, card)) = queued {
                self.marked.insert(card);
                self.add_step(view, previous, holder, card, StepKind::Queued);
                continue;
            }
            let info = view.info[previous.index()];
            // Everybody knows where it is.
            if info.clued == Some(Clued::Everyone) {
                if let Some((holder, card)) = info.locked
                    && view.state.hand(holder).contains(&card)
                {
                    let note = view.note(card);
                    let kind = if note.play || note.delayed > 0 {
                        StepKind::Queued
                    } else {
                        StepKind::Prompt
                    };
                    self.add_step(view, previous, holder, card, kind);
                }
                continue;
            }
            // Clued, but its holder does not know for sure.
            if let Some(Clued::Holder(holder)) = info.clued {
                match self.find_clued_card(view, holder, previous, allowed) {
                    Some(true) => continue,
                    Some(false) => return false,
                    None => {}
                }
            }
            // Finesse positions, from the seat before us backwards.
            if finesses {
                for offset in (1..n).rev() {
                    let seat = me.after(offset, n);
                    if seat == self.giver || self.excluded_finesses.contains(seat) {
                        // a giver does not know their own hand, so cannot target it
                        continue;
                    }
                    if seat == self.receiver && allowed != FirstAction::SelfFinesse {
                        continue;
                    }
                    let blocking_prompt = prompts
                        .then(|| {
                            view.newest_first(seat)
                                .find(|&(_, c)| {
                                    let note = view.note(c);
                                    !self.marked.contains(c)
                                        && note.clued
                                        && note.belief.contains(previous)
                                        && !view.identity(c).is_some_and(|i| {
                                            matches!(
                                                view.play_state(i),
                                                PlayState::Playable | PlayState::CriticalPlayable
                                            )
                                        })
                                })
                                .map(|(_, c)| c)
                        })
                        .flatten();
                    let mut layers = 0;
                    let mut layer_identities: SmallVec<[Identity; 5]> = SmallVec::new();
                    let mut layer_cards: SmallVec<[CardId; 5]> = SmallVec::new();
                    for (_, card) in view.newest_first(seat) {
                        let note = view.note(card);
                        if note.clued || note.promised.is_some() || self.marked.contains(card) {
                            continue;
                        }
                        let identity = view.identity(card);
                        if identity != Some(previous) {
                            if let Some(identity) = identity
                                && layered
                                && view.playable_set().contains(identity)
                                && view.info[identity.index()].clued.is_none()
                                && !view
                                    .state
                                    .seats()
                                    .flat_map(|s| view.state.hand(s).iter().copied())
                                    .any(|other| {
                                        other != card
                                            && (view.note(other).clued
                                                || view.note(other).promised.is_some())
                                            && view.identity(other) == Some(identity)
                                    })
                                && !self.steps.iter().any(|step| {
                                    step.kind != StepKind::Focus && step.identity == identity
                                })
                                && !layer_identities.contains(&identity)
                            {
                                layers += 1;
                                layer_identities.push(identity);
                                layer_cards.push(card);
                                continue;
                            }
                            break;
                        }
                        if let Some(prompt) = blocking_prompt {
                            // Even a mistaken clue prompts the newest matching clued card
                            // first. Keep that meaning so everybody can recognise a wrong
                            // prompt.
                            self.marked.insert(prompt);
                            self.add_step(view, previous, seat, prompt, StepKind::Prompt);
                            self.marked.insert(card);
                            allowed = FirstAction::SelfFinesse;
                            continue 'rank_loop;
                        }
                        self.marked.insert(card);
                        for &layer in &layer_cards {
                            self.marked.insert(layer);
                        }
                        self.add_step(view, previous, seat, card, StepKind::Finesse);
                        self.steps.last_mut().expect("just added").layers = layers;
                        allowed = FirstAction::SelfFinesse;
                        continue 'rank_loop;
                    }
                }
            }
            // Queued in our own hand.
            let ones = view.variant.rank_identities(1);
            if let Some((_, card)) = view
                .newest_first(me)
                .filter(|&(_, c)| {
                    let note = view.note(c);
                    !self.marked.contains(c)
                        && (note.clued || note.promised.is_some())
                        && note.belief.contains(previous)
                        && (self.giver != me || note.belief.size() == 1)
                        && (note.play || note.delayed > 0)
                })
                .min_by_key(|&(pos, c)| {
                    if ones.contains(previous) && view.note(c).belief.soft.is_subset(ones) {
                        view.one_order(me, c)
                    } else {
                        (0, pos)
                    }
                })
            {
                self.marked.insert(card);
                self.add_step(view, previous, me, card, StepKind::Queued);
                continue;
            }
            if allowed != FirstAction::NonSelf {
                if prompts {
                    for (_, card) in view.newest_first(me) {
                        let note = view.note(card);
                        if self.giver == me && note.belief.size() != 1 {
                            continue;
                        }
                        if !note.clued
                            || self.marked.contains(card)
                            || !note.belief.contains(previous)
                        {
                            continue;
                        }
                        self.marked.insert(card);
                        self.add_step(view, previous, me, card, StepKind::Prompt);
                        continue 'rank_loop;
                    }
                }
            } else {
                for (_, card) in view.newest_first(me) {
                    let note = view.note(card);
                    if self.marked.contains(card)
                        || note.belief.size() != 1
                        || !note.belief.contains(previous)
                    {
                        continue;
                    }
                    self.marked.insert(card);
                    let kind = if note.clued {
                        StepKind::Prompt
                    } else {
                        StepKind::Finesse
                    };
                    self.add_step(view, previous, me, card, kind);
                    continue 'rank_loop;
                }
            }
            if allowed == FirstAction::SelfFinesse
                && finesses
                && self.giver != me
                && !self.excluded_finesses.contains(me)
            {
                for (_, card) in view.newest_first(me) {
                    let note = view.note(card);
                    if note.clued || self.marked.contains(card) {
                        continue;
                    }
                    if !note.belief.contains(previous) {
                        break;
                    }
                    self.marked.insert(card);
                    self.add_step(view, previous, me, card, StepKind::Finesse);
                    continue 'rank_loop;
                }
            }
            return false;
        }
        true
    }
}
