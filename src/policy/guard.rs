//! Legibility guard (docs/architecture.md §7.5): a clue is permitted only if every seat
//! reads it as intended.
//!
//! The check is [`View::clue_views`], the same interpreter that reads real clues: each
//! teammate reads the clue in its own kept view (its true sight), the seats in between act
//! on their convention obligations (plays and sarcastic discards the clue's readings
//! require), and the clue counts as an error if the true reading does not survive or a
//! seat's next play would misplay. A clue is legible when it adds no error to the
//! position. The guard does not use the partner model, so it does not recurse.
//!
//! A discard is not permitted while we hold a convention play, unless it is the sarcastic
//! discard: from level 7 a discard instead of a known play is a Scream Discard Chop Move,
//! which the bot neither reads nor gives yet, so a human partner would misread it.
//!
//! Gap: an in-between seat's free choice (a clue or discard instead of an obligation) is
//! not enumerated over its top expected actions yet.

use smallvec::SmallVec;

use crate::conventions::{View, Worth};
use crate::domain::{ClueId, Seat};
use crate::engine::Action;

/// Whether our clue would be read as intended, given the errors the position already has.
pub fn legible_clue(view: &View, baseline: &Worth, target: Seat, clue: ClueId) -> bool {
    view.clue_views(target, clue, |_, upper| upper.no_more_errors_than(baseline))
        .is_some_and(|(worth, _)| worth.no_more_errors_than(baseline))
}

/// Whether the guard permits `action`: plays always, discards only without a convention
/// play (or as the sarcastic discard), clues when legible.
pub fn permitted(view: &View, baseline: &Worth, action: Action) -> bool {
    match action {
        Action::Clue { target, clue } => legible_clue(view, baseline, target, clue),
        Action::Play(_) => true,
        Action::Discard(card) => legible_discard(view, card),
    }
}

/// Whether discarding `card` reads as a plain discard: we hold no convention play, or the
/// discard is the sarcastic one the convention asks for.
pub fn legible_discard(view: &View, card: crate::domain::CardId) -> bool {
    view.next_play().is_none() || view.sarcastic_discard() == Some(card)
}

/// The blueprint's candidates that pass the guard, in blueprint order.
pub fn permitted_candidates(view: &View) -> SmallVec<[Action; 48]> {
    let baseline = view.worth(0);
    super::candidates(view)
        .into_iter()
        .filter(|&action| permitted(view, &baseline, action))
        .collect()
}
