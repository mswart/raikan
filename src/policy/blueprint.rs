//! Blueprint policy: the conventional choice of action without search.
//!
//! It keeps the established convention order (plays, sarcastic discards, fixes, stalls by
//! the stall table, then the best-worth action) and uses a one-rotation look-ahead
//! (level 7 tactical support) to intervene in predictable emergencies. Every decision names
//! its reason, so it can be traced and later replaced by the planner.

use smallvec::SmallVec;

use crate::conventions::{PlayState, Rule, StallKind, StallSeverity, View, Worth, card_event};
use crate::domain::{CardId, ClueId, Identity, Seat};
use crate::engine::{Action, Event};

const LOST_POINT_COST: f64 = 25.0;
const STRIKE_COST: f64 = 50.0;
const STRIKEOUT_COST: f64 = 1000.0;

/// Why the blueprint chose an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Reason {
    /// A safe play or sarcastic discard in convention order.
    SafeConventionAction,
    /// A play of an unknown 1 out of order that chop moves a teammate's valuable chop.
    OrderChopMove,
    Fix,
    Stall(&'static str),
    /// Highest convention worth without a forecast emergency.
    Preferred,
    /// A forecast emergency: least danger.
    Emergency,
    /// Nothing evaluated; forced fallback.
    Fallback,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub action: Action,
    pub reason: Reason,
}

#[derive(Debug, Clone)]
struct Evaluation {
    priority: Worth,
    value: f64,
    danger: f64,
    urgent: f64,
}

fn stall_name(kind: StallKind) -> &'static str {
    match kind {
        StallKind::Five => "L9.stall.five",
        StallKind::Tempo => "L9.stall.tempo",
        StallKind::FillIn => "L9.stall.fill-in",
        StallKind::LockedSave => "L9.stall.locked-save",
        StallKind::EightClueSave => "L9.stall.eight-clue-save",
        StallKind::Burn => "L9.stall.burn",
    }
}

fn max_clues(view: &View) -> u8 {
    view.variant.tokens.max / view.variant.tokens.clue_cost
}

/// Candidate commands: the convention play, the convention discard and every clue that
/// touches a card.
pub fn candidates(view: &View) -> SmallVec<[Action; 48]> {
    let clues = view.state.clue_tokens();
    let mut actions = SmallVec::new();
    // Keep the convention's play order and discard position: another position could itself
    // communicate an unsupported convention.
    if let Some(card) = view.next_play()
        && (clues == max_clues(view) || view.sarcastic_discard().is_none())
    {
        actions.push(Action::Play(card));
    }
    if view.state.tokens < view.variant.tokens.max {
        actions.push(Action::Discard(view.discard_choice()));
    }
    if view.state.tokens >= view.variant.tokens.clue_cost {
        for target in view.others() {
            for clue in view.variant.clue_ids() {
                // alternating clues: not the type of the previous clue
                let rank = matches!(
                    view.variant.clue_value(clue),
                    crate::variants::ClueValue::Rank(_)
                );
                let legal =
                    !(view.variant.alternating_clues && view.state.last_clue_rank == Some(rank));
                if legal && !view.visible_touched(target, clue).is_empty() {
                    actions.push(Action::Clue { target, clue });
                }
            }
        }
    }
    actions
}

/// Choose an action for `view.me`.
pub fn choose_action(view: &View) -> Decision {
    decide(view, &mut Alternatives::new())
}

/// Actions the blueprint evaluated, with their value (quality minus danger), in
/// evaluation order. Candidates pruned before evaluation are missing.
pub type Alternatives = SmallVec<[(Action, f64); 16]>;

/// The blueprint's decision and the other actions it evaluated (for the partner model's
/// spread). A convention obligation (safe play, sarcastic discard) returns before the
/// remaining candidates are evaluated.
pub fn rank(view: &View) -> (Decision, Alternatives) {
    let mut alternatives = Alternatives::new();
    let decision = decide(view, &mut alternatives);
    alternatives.retain(|(action, _)| *action != decision.action);
    (decision, alternatives)
}

fn decide(view: &View, scored: &mut Alternatives) -> Decision {
    let prune = true;
    let clues = view.state.clue_tokens();
    let max = max_clues(view);
    let severity = view.stall_severity(view.me);
    let baseline = view.worth(0);
    let lookahead = view.rule(Rule::EmergencyLookahead);
    let sarcastic = view.sarcastic_discard();
    // A promised blind play that itself removes the position's errors (a teammate would
    // misread a clue unless we play into the finesse now) is still the convention play.
    let resolving_play = baseline.has_errors()
        && view
            .next_play()
            .is_some_and(|card| blind_play_resolves(view, card, &baseline));
    let mut normal_clue_available = false;
    let mut best_stall: Option<(StallKind, u8, Action, Evaluation)> = None;
    let mut best: Option<(Action, f64)> = None;
    let mut preferred: Option<(Action, Evaluation)> = None;
    let mut best_fix: Option<(Action, f64)> = None;
    for action in candidates(view) {
        let stall = match action {
            Action::Clue { target, clue } => view
                .clue_stall(target, clue)
                .filter(|kind| !(kind.is_general_save() && view.saves_critical_chop(target, clue))),
            _ => None,
        };
        let Some(evaluation) = evaluate(view, action, &baseline, lookahead, |upper| {
            !prune
                || !baseline.no_more_errors_than(upper)
                || stall.is_some()
                || best.is_none_or(|(_, value)| upper.utility() as f64 > value)
                || preferred.as_ref().is_none_or(|(previous, evaluation)| {
                    !matches!(previous, Action::Play(_)) && *upper > evaluation.priority
                })
                || (!normal_clue_available
                    && *upper > baseline
                    && upper.no_more_errors_than(&baseline))
        }) else {
            continue;
        };
        let value = evaluation.value;
        scored.push((action, value));
        if best.is_none_or(|(_, previous)| value > previous) {
            best = Some((action, value));
        }
        if let Action::Clue { target, clue } = action {
            if !baseline.no_more_errors_than(&evaluation.priority)
                && view.clue_fixes(target, clue)
                && evaluation.urgent == 0.0
                && best_fix.is_none_or(|(_, previous)| value > previous)
            {
                best_fix = Some((action, value));
            }
            if let Some(kind) = stall {
                if evaluation.urgent == 0.0 && evaluation.priority.no_more_errors_than(&baseline) {
                    let distance = if kind == StallKind::Five {
                        view.five_stall_distance(target)
                    } else {
                        0
                    };
                    if best_stall
                        .as_ref()
                        .is_none_or(|(old_kind, old_distance, _, old)| {
                            (kind.precedence(), distance) < (old_kind.precedence(), *old_distance)
                                || ((kind.precedence(), distance)
                                    == (old_kind.precedence(), *old_distance)
                                    && evaluation.value > old.value)
                        })
                    {
                        best_stall = Some((kind, distance, action, evaluation));
                    }
                }
                continue;
            }
            normal_clue_available |= evaluation.priority > baseline
                && evaluation.priority.no_more_errors_than(&baseline)
                && evaluation.urgent == 0.0;
        }
        // Preserve the expected play order when it is safe. Skipping a promised play can
        // retract a teammate's connection, so only an emergency warrants it.
        if ((matches!(action, Action::Play(_)) && (!baseline.has_errors() || resolving_play))
            || (clues < max && matches!(action, Action::Discard(c) if Some(c) == sarcastic)))
            && evaluation.danger == 0.0
        {
            if matches!(action, Action::Play(_))
                && let Some(card) = view.order_chop_move_play()
            {
                return Decision {
                    action: Action::Play(card),
                    reason: Reason::OrderChopMove,
                };
            }
            return Decision {
                action,
                reason: Reason::SafeConventionAction,
            };
        }
        if preferred
            .as_ref()
            .is_none_or(|(previous_action, previous)| {
                !matches!(previous_action, Action::Play(_))
                    && evaluation.priority > previous.priority
            })
        {
            preferred = Some((action, evaluation));
        }
    }
    if let Some((action, _)) = best_fix {
        return Decision {
            action,
            reason: Reason::Fix,
        };
    }
    if !normal_clue_available
        && (severity >= StallSeverity::DoubleDiscard
            || (severity == StallSeverity::Early && !view.context.early_five_stalled))
        && let Some((kind, _, action, _)) = best_stall
    {
        return Decision {
            action,
            reason: Reason::Stall(stall_name(kind)),
        };
    }
    // Unknown chop cards always carry some background risk; that alone must not replace
    // normal discards with weak clues.
    if let Some((action, evaluation)) = preferred
        && evaluation.urgent == 0.0
    {
        return Decision {
            action,
            reason: Reason::Preferred,
        };
    }
    if let Some((action, _)) = best {
        return Decision {
            action,
            reason: Reason::Emergency,
        };
    }
    // Inconsistent beliefs must not cause an illegal discard at maximum clues.
    let action = if view.state.tokens >= view.variant.tokens.max {
        Action::Play(
            view.newest_first(view.me)
                .next()
                .map(|(_, c)| c)
                .expect("non-empty hand"),
        )
    } else {
        Action::Discard(view.discard_choice())
    };
    Decision {
        action,
        reason: Reason::Fallback,
    }
}

/// Whether blind-playing our promised `card` as the identity it was promised as leaves
/// fewer convention errors than `baseline`.
fn blind_play_resolves(view: &View, card: CardId, baseline: &Worth) -> bool {
    let note = view.note(card);
    if note.clued || note.promised.is_none() {
        return false;
    }
    let Some(identity) = note.belief.identity() else {
        return false;
    };
    let mut after = view.detached();
    after.observe(&Event::Play {
        seat: view.me,
        card,
        identity: Some(identity),
        success: view.playable_set().contains(identity),
    });
    after.worth(0).errors < baseline.errors
}

fn evaluate(
    view: &View,
    action: Action,
    baseline: &Worth,
    lookahead: bool,
    accept: impl FnOnce(&Worth) -> bool,
) -> Option<Evaluation> {
    match action {
        Action::Clue { target, clue } => {
            let mut needs_fix = false;
            let (mut forecast, priority) = Forecast::clue(view, target, clue, |after, upper| {
                let gets_new_card = view.state.seats().any(|seat| {
                    view.state.hand(seat).iter().any(|&card| {
                        let before = view.note(card);
                        let after = after.note(card);
                        !before.clued
                            && before.promised.is_none()
                            && (after.clued
                                || after.promised.is_some()
                                || (after.chop_moved && !before.chop_moved))
                    })
                });
                // Minimum clue value: ordinary tempo clues must not replace a discard. Fixes,
                // stalls and valuable tempo clues have their own reason to spend a clue.
                needs_fix = !gets_new_card
                    && view.clue_stall(target, clue).is_none()
                    && after.last_rule() != "L6.valuable-tempo-clue";
                if needs_fix && baseline.no_more_errors_than(upper) {
                    return false;
                }
                accept(upper)
            })?;
            if needs_fix && baseline.no_more_errors_than(&priority) {
                return None;
            }
            let quality = priority.utility() as f64;
            if lookahead {
                forecast.rollout();
            }
            let danger = forecast.danger();
            Some(Evaluation {
                priority,
                value: quality - danger,
                danger,
                urgent: danger,
            })
        }
        Action::Play(card) | Action::Discard(card) => {
            let outcomes = own_card_outcomes(view, card);
            let total: u32 = outcomes.iter().map(|(_, weight)| *weight as u32).sum();
            if total == 0 {
                return None;
            }
            let play = matches!(action, Action::Play(_));
            let note = view.note(card);
            let mut value = 0.0;
            let mut danger = 0.0;
            let mut urgent = 0.0;
            let priority = baseline.clone();
            for (identity, weight) in outcomes {
                let mut forecast = Forecast::own_card(view, card, identity, play);
                let background_risk = if !play && !note.touched() && note.belief.size() > 1 {
                    forecast.lost_points as f64 * LOST_POINT_COST
                } else {
                    0.0
                };
                // Keep the information-value baseline: hypothetical discards must not earn
                // credit by making conflicting beliefs disappear.
                let quality = priority.utility() as f64
                    + if play && forecast.new_strikes == 0 {
                        10.0
                    } else {
                        0.0
                    };
                if lookahead {
                    forecast.rollout();
                }
                danger += forecast.danger() * weight as f64;
                urgent += (forecast.danger() - background_risk) * weight as f64;
                value += (quality - forecast.danger()) * weight as f64;
            }
            Some(Evaluation {
                priority,
                value: value / total as f64,
                danger: danger / total as f64,
                urgent: urgent / total as f64,
            })
        }
    }
}

/// Possible identities of our own card, weighted by unseen copies.
fn own_card_outcomes(view: &View, card: CardId) -> SmallVec<[(Identity, u8); 36]> {
    let note = view.note(card);
    let known = view.known_counts();
    let options = |hard: bool| {
        view.variant
            .all_identities()
            .iter()
            .filter_map(|identity| {
                let possible = if hard {
                    note.belief.contains_hard(identity)
                } else {
                    note.belief.contains(identity)
                };
                let remaining = view
                    .variant
                    .copies(identity)
                    .saturating_sub(known[identity.index()]);
                (possible && remaining > 0).then_some((identity, remaining))
            })
            .collect::<SmallVec<[(Identity, u8); 36]>>()
    };
    let inferred = options(false);
    // Known duplicates can have an empty inference; literal information still supplies
    // possible identities.
    if inferred.is_empty() {
        options(true)
    } else {
        inferred
    }
}

fn newly_dead(before: &View, after: &View) -> u8 {
    before
        .variant
        .all_identities()
        .iter()
        .filter(|&i| {
            before.play_state(i) != PlayState::Dead && after.play_state(i) == PlayState::Dead
        })
        .count() as u8
}

/// One-rotation forecast without draws: other seats play their convention plays and
/// sarcastic discards, and discard when out of clues; it stops at the first seat that could
/// give a clue.
struct Forecast {
    /// `views[k]` belongs to the seat `k` places after us.
    views: Vec<View>,
    /// Card-action forecasts often stop at the next seat: later views are built lazily from
    /// this position before our action, replaying `events`.
    initial: Option<View>,
    events: SmallVec<[Event; 5]>,
    strikes: u8,
    new_strikes: u8,
    lost_points: u8,
}

impl Forecast {
    fn clue(
        view: &View,
        target: Seat,
        clue: ClueId,
        accept: impl FnOnce(&View, &Worth) -> bool,
    ) -> Option<(Self, Worth)> {
        let (worth, views) = view.clue_views(target, clue, accept)?;
        Some((
            Self {
                views,
                initial: None,
                events: SmallVec::new(),
                strikes: view.state.strikes,
                new_strikes: 0,
                lost_points: 0,
            },
            worth,
        ))
    }

    fn own_card(view: &View, card: CardId, identity: Identity, play: bool) -> Self {
        let mut after = view.detached();
        let mut new_strikes = 0;
        let event = if play {
            let success = view.playable_set().contains(identity);
            if !success {
                new_strikes = 1;
            }
            Event::Play {
                seat: view.me,
                card,
                identity: Some(identity),
                success,
            }
        } else {
            Event::Discard {
                seat: view.me,
                card,
                identity: Some(identity),
            }
        };
        after.observe(&event);
        let lost_points = newly_dead(view, &after);
        // Teammate views come from the position before our action (the agent's kept views),
        // with every forecast event replayed on them.
        let initial = Some(view.clone());
        Self {
            views: vec![after],
            initial,
            events: smallvec::smallvec![event],
            strikes: view.state.strikes + new_strikes,
            new_strikes,
            lost_points,
        }
    }

    fn rollout(&mut self) {
        let root = &self.views[0];
        let n = root.num_seats() as usize;
        let me = root.me;
        for k in 1..n {
            let seat = me.after(k as u8, n as u8);
            if self.strikes >= 3 || self.views[0].score() == self.views[0].variant.max_score() {
                break;
            }
            if k == self.views.len() {
                // decision view of the original position, then the forecast's events
                let initial = self.initial.as_ref().expect("lazy card forecast");
                let mut view = initial.decision_view(seat);
                for event in &self.events {
                    view.observe(event);
                }
                self.views.push(view);
            }
            let tokens = self.views[0].state.tokens;
            let max = self.views[0].variant.tokens.max;
            let decision = &self.views[k];
            let action = if tokens < max
                && let Some(card) = decision.sarcastic_discard()
            {
                (card, false)
            } else if let Some(card) = decision.next_play() {
                (card, true)
            } else if self.views[0].state.clue_tokens() == 0 {
                (decision.discard_choice(), false)
            } else {
                // This seat can choose a clue; do not invent one.
                break;
            };
            // A hypothetical view (a teammate's, predicted by the legibility guard) does
            // not know the giver's unidentified cards: the forecast cannot go on.
            if self.views[0].identity(action.0).is_none() {
                break;
            }
            self.apply_visible_card(k, seat, action.0, action.1);
        }
    }

    fn apply_visible_card(&mut self, k: usize, seat: Seat, card: CardId, play: bool) {
        let event = card_event(&self.views[0], seat, card, play);
        let before = self.views[0].clone();
        for (observer, view) in self.views.iter_mut().enumerate() {
            // The forecast never revisits a seat: keep the root and future decision makers
            // current.
            if observer != 0 && observer <= k {
                continue;
            }
            view.observe(&event);
        }
        self.lost_points += newly_dead(&before, &self.views[0]);
        if self.initial.is_some() {
            self.events.push(event);
        }
        if let Event::Play { success: false, .. } = event {
            self.strikes += 1;
            self.new_strikes += 1;
        }
    }

    fn danger(&self) -> f64 {
        self.lost_points as f64 * LOST_POINT_COST
            + self.new_strikes as f64 * STRIKE_COST
            + if self.strikes >= 3 {
                STRIKEOUT_COST
            } else {
                0.0
            }
    }
}
