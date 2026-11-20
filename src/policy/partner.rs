//! Partner model (docs/architecture.md §7.1): what a teammate does, predicted from that
//! teammate's own view only.
//!
//! For now this is the blueprint's first choice. A spread over the blueprint ranking and
//! convention obligations first are later refinements behind the same function.

use crate::conventions::View;
use crate::engine::Action;

/// The action the seat owning `view` is predicted to take. `view` must be that seat's own
/// view (its `me` is the actor); it never sees cards that seat cannot see.
pub fn partner_action(view: &View) -> Action {
    partner_decision(view).action
}

/// [`partner_action`] with the blueprint's reason, for traces and `explain`.
pub fn partner_decision(view: &View) -> super::Decision {
    super::choose_action(&view.decision_view(view.me))
}
