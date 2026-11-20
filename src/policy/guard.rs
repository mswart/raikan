//! Legibility guard (docs/architecture.md §7.5): a clue is permitted only if every seat
//! reads it as intended.
//!
//! The check is [`View::clue_views_expecting`], the same interpreter that reads real
//! clues: each teammate reads the clue in its own kept view (its true sight), the seats in
//! between act on their convention obligations (plays and sarcastic discards the clue's
//! readings require), and the clue counts as an error if the true reading does not survive
//! or a seat's next play would misplay. A seat in between without an obligation has a free
//! choice: if the blueprint predicts a clue from its view, the check also runs with that
//! clue given, and the clue must be safe in both branches. The prediction does not use
//! this guard, so the recursion depth is one. A clue is legible when it adds no error to
//! the position.
//!
//! A discard is not permitted while we hold a convention play, unless it is the sarcastic
//! discard or the card is known trash. A discard that teammates read as a scream, shout or
//! generation discard (level 7) is permitted only when we mean to give that signal, and no
//! discard is permitted while a scream asks us to clue.
//!
//! A clue is not permitted while we hold a convention play when its focus is a potential
//! finesse: a playable, non-critical card on the receiver's finesse position. Nothing is
//! lost by playing first, because the card can still be played through a finesse of its
//! successor (or clued later), while the delayed play costs tempo.
//!
//! A discard that fills the clue tokens to the maximum is not permitted while a legible
//! play clue is available (unless it is the sarcastic discard): the next seat then has to
//! clue, and typically gives our play clue for us, so the discard only passes our clue on
//! and wastes that seat's turn. A one-rotation rollout misses this cost, because the
//! clue the next seat would otherwise have given pays off beyond the horizon, and credits
//! the discard with two tokens.

use smallvec::SmallVec;

use crate::conventions::{PlayState, View, Worth};
use crate::domain::{CardId, ClueId, Seat};
use crate::engine::Action;

/// The blueprint's prediction of a seat's free choice, from that seat's view.
pub fn expected_action(view: &View) -> Option<Action> {
    Some(super::choose_action(&view.decision_view(view.me)).action)
}

/// Whether our clue would be read as intended, given the errors the position already has.
pub fn legible_clue(view: &View, baseline: &Worth, target: Seat, clue: ClueId) -> bool {
    clue_errors(view, baseline, target, clue).is_some_and(|errors| errors <= baseline.errors)
}

/// The convention errors of the position after our clue, with the in-between seats'
/// free choices; `None` when the clue touches nothing or already the optimistic grade
/// has more errors than `baseline`.
pub fn clue_errors(view: &View, baseline: &Worth, target: Seat, clue: ClueId) -> Option<u8> {
    clue_worth(view, baseline, target, clue).map(|worth| worth.errors)
}

/// The worth of the position after our clue, as [`clue_errors`] grades it.
fn clue_worth(view: &View, baseline: &Worth, target: Seat, clue: ClueId) -> Option<Worth> {
    view.clue_views_expecting(
        target,
        clue,
        |_, upper| upper.no_more_errors_than(baseline),
        Some(&expected_action),
    )
    .map(|(worth, _)| worth)
}

/// The clue is permitted, and the play clue flag: it gets more cards played.
fn grade_clue(view: &View, baseline: &Worth, target: Seat, clue: ClueId) -> (bool, bool) {
    if delays_play_for_finesse(view, target, clue) {
        return (false, false);
    }
    match clue_worth(view, baseline, target, clue) {
        Some(worth) if worth.errors <= baseline.errors => (true, worth.plays > baseline.plays),
        _ => (false, false),
    }
}

/// Whether a discard would fill the clue tokens to the maximum.
pub fn discard_fills_tokens(view: &View) -> bool {
    let tokens = &view.variant.tokens;
    view.state.tokens < tokens.max
        && view.state.tokens.saturating_add(tokens.discard_gain) >= tokens.max
}

/// Whether discarding `card` would fill the clue tokens to the maximum while a legible
/// play clue is available (not the sarcastic discard).
pub fn discard_passes_play_clue(view: &View, card: CardId) -> bool {
    if !discard_fills_tokens(view) || view.sarcastic_discard() == Some(card) {
        return false;
    }
    let baseline = view.worth(0);
    super::candidates(view)
        .into_iter()
        .any(|action| match action {
            Action::Clue { target, clue } => {
                grade_clue(view, &baseline, target, clue) == (true, true)
            }
            _ => false,
        })
}

/// Whether the guard permits `action`: plays always, discards only without a convention
/// play (or as the sarcastic discard, or of known trash), clues when legible.
pub fn permitted(view: &View, baseline: &Worth, action: Action) -> bool {
    match action {
        Action::Clue { target, clue } => {
            !delays_play_for_finesse(view, target, clue)
                && legible_clue(view, baseline, target, clue)
        }
        Action::Play(_) => true,
        Action::Discard(card) => legible_discard(view, card),
    }
}

/// The card of `seat` a finesse would blind-play: the newest card that is neither clued,
/// promised nor already called to play.
pub fn finesse_position(view: &View, seat: Seat) -> Option<CardId> {
    view.newest_first(seat).map(|(_, card)| card).find(|&card| {
        let note = view.note(card);
        !note.touched() && !note.play
    })
}

/// Whether `clue` would delay our convention play only to clue a potential finesse: its
/// focus is a playable, non-critical card on the receiver's finesse position, not on chop.
pub fn delays_play_for_finesse(view: &View, target: Seat, clue: ClueId) -> bool {
    if view.next_play().is_none() {
        return false;
    }
    let touched = view.visible_touched(target, clue);
    let Some(focus) = view.clue_focus(target, touched) else {
        return false;
    };
    finesse_position(view, target) == Some(focus)
        && view.chop(target) != Some(focus)
        && view
            .identity(focus)
            .is_some_and(|identity| view.play_state(identity) == PlayState::Playable)
}

/// Whether discarding `card` reads as intended: we hold no convention play (and no scream
/// asks us to clue), the discard is the sarcastic one the convention asks for, the card
/// is known trash (a shout discard from level 7, so only when we mean one), or it is the
/// scream, shout or generation discard we mean to give.
pub fn legible_discard(view: &View, card: CardId) -> bool {
    if view.context.must_clue == Some(view.me) && view.state.clue_tokens() > 0 {
        return false;
    }
    if view.sarcastic_discard() == Some(card) {
        return true;
    }
    if view.positional_slot(view.me, card, false).is_some() {
        return view.positional_signal() == Some((card, false));
    }
    if view.discard_signal(view.me, card, None).is_some() {
        return view.signal_discard().map(|(c, _)| c) == Some(card)
            || view.generation_discard() == Some(card);
    }
    view.next_play().is_none() || known_trash(view, card)
}

/// Every identity our literal information allows is playable: playing it out of the
/// convention order cannot be misread and cannot fail.
pub fn known_playable(view: &View, card: CardId) -> bool {
    let hard = view.note(card).belief.hard;
    !hard.is_empty() && (hard - view.playable_set()).is_empty()
}

/// Every identity our literal information allows is trash, or the conventions marked it
/// trash; discarding it is always allowed.
pub fn known_trash(view: &View, card: CardId) -> bool {
    let note = view.note(card);
    let hard = note.belief.hard;
    note.trash || (!hard.is_empty() && (hard - view.trash_set()).is_empty())
}

/// The blueprint's candidates that pass the guard, in blueprint order, followed by plays
/// of other known-playable cards and discards of other known-trash cards (any slot, §7.1).
/// Discards that fill the clue tokens while a play clue is permitted are dropped.
pub fn permitted_candidates(view: &View) -> SmallVec<[Action; 48]> {
    let baseline = view.worth(0);
    let mut play_clue = false;
    let mut actions: SmallVec<[Action; 48]> = super::candidates(view)
        .into_iter()
        .filter(|&action| match action {
            Action::Clue { target, clue } => {
                let (permitted, plays) = grade_clue(view, &baseline, target, clue);
                play_clue |= plays;
                permitted
            }
            _ => permitted(view, &baseline, action),
        })
        .collect();
    let passes_clue =
        |card| play_clue && discard_fills_tokens(view) && view.sarcastic_discard() != Some(card);
    actions.retain(|action| !matches!(*action, Action::Discard(card) if passes_clue(card)));
    let can_discard = view.state.tokens < view.variant.tokens.max;
    for &card in view.state.hand(view.me) {
        let extra = [
            known_playable(view, card).then_some(Action::Play(card)),
            (can_discard && known_trash(view, card) && !passes_clue(card))
                .then_some(Action::Discard(card)),
        ];
        for action in extra.into_iter().flatten() {
            if !actions.contains(&action) {
                actions.push(action);
            }
        }
    }
    actions
}
