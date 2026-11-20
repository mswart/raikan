//! Partner model (docs/architecture.md §7.1): what a teammate does, predicted from that
//! teammate's own view only.
//!
//! Convention obligations come first: a safe convention play, a sarcastic discard or a fix
//! is predicted with certainty. Otherwise the blueprint's choice gets `1 − spread` and the
//! best other actions the blueprint evaluated share `spread` (teammates are humans or other
//! bots, which do not always pick the blueprint's first choice). The model is frozen for
//! one planning call; it sees only the seat's own view, so it never depends on cards that
//! seat cannot see.

use smallvec::SmallVec;

use super::{Decision, Reason};
use crate::conventions::View;
use crate::engine::Action;

/// Alternatives that share the spread.
const ALTERNATIVES: usize = 2;

/// One predicted action and its probability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prediction {
    pub action: Action,
    pub probability: f64,
    /// The blueprint's reason for its own choice; `None` for an alternative.
    pub reason: Option<Reason>,
}

pub type Distribution = SmallVec<[Prediction; 3]>;

/// The action the seat owning `view` is predicted to take. `view` must be that seat's own
/// view (its `me` is the actor); it never sees cards that seat cannot see.
pub fn partner_action(view: &View) -> Action {
    partner_decision(view).action
}

/// [`partner_action`] with the blueprint's reason, for traces and `explain`.
pub fn partner_decision(view: &View) -> Decision {
    super::choose_action(&view.decision_view(view.me))
}

/// Whether the blueprint's reason is a convention obligation, which the model predicts
/// with certainty.
pub fn is_obligation(reason: Reason) -> bool {
    matches!(
        reason,
        Reason::SafeConventionAction | Reason::OrderChopMove | Reason::Fix
    )
}

/// The predicted distribution over the seat's actions; probabilities sum to 1, the
/// blueprint's choice first. `spread` 0 is the blueprint alone.
pub fn partner_distribution(view: &View, spread: f64) -> Distribution {
    let decision_view = view.decision_view(view.me);
    let only = |decision: Decision| {
        let mut d = Distribution::new();
        d.push(Prediction {
            action: decision.action,
            probability: 1.0,
            reason: Some(decision.reason),
        });
        d
    };
    if spread <= 0.0 {
        return only(super::choose_action(&decision_view));
    }
    let (decision, mut alternatives) = super::blueprint::rank(&decision_view);
    if is_obligation(decision.reason) || alternatives.is_empty() {
        return only(decision);
    }
    alternatives.sort_by(|a, b| b.1.total_cmp(&a.1));
    alternatives.truncate(ALTERNATIVES);
    let share = spread / alternatives.len() as f64;
    let mut d = Distribution::new();
    d.push(Prediction {
        action: decision.action,
        probability: 1.0 - spread,
        reason: Some(decision.reason),
    });
    for (action, _) in alternatives {
        d.push(Prediction {
            action,
            probability: share,
            reason: None,
        });
    }
    d
}

/// The prediction a uniform draw `u` in `[0, 1)` selects.
pub fn pick(distribution: &Distribution, u: f64) -> Prediction {
    let mut acc = 0.0;
    for p in distribution {
        acc += p.probability;
        if u < acc {
            return *p;
        }
    }
    *distribution.last().expect("a distribution is never empty")
}
