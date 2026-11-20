//! One simulation of a root candidate in one sampled world.
//!
//! Every seat acts from its own view and reads every event through its own interpreter:
//! our command is applied as a plain command (its intent is never transmitted, §7.1), the
//! other seats follow the partner model, and draws come from the sampled deck.

use crate::conventions::View;
use crate::domain::Seat;
use crate::engine::{self, Action, GameState, World};
use crate::policy::{Reason, partner_decision};

use super::value::{self, Breakdown};

/// The seat views at the root, for one world: each teammate's kept view with our own cards
/// revealed as the world has them (teammates see our hand). `views[s]` belongs to seat `s`.
pub fn seat_views(root: &View, world: &World) -> Vec<View> {
    let me = root.me;
    root.state
        .seats()
        .map(|seat| {
            if seat == me {
                return root.detached();
            }
            let mut view = match root.seat_view(seat) {
                Some(kept) => kept.detached(),
                None => root.perspective(seat),
            };
            for &card in root.state.hand(me) {
                view.known.set(card, world.identity(card));
            }
            view
        })
        .collect()
}

/// Whether the rollout plays to the real end instead of one rotation.
pub fn plays_to_end(state: &GameState) -> bool {
    state.final_turns.is_some() || state.cards_left() <= state.num_seats
}

/// One step of a traced rollout.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RolloutStep {
    pub seat: Seat,
    /// The command actually applied.
    pub action: Action,
    /// The partner model's reason; `None` for the root candidate.
    pub reason: Option<Reason>,
    /// The predicted action was illegal in this world and replaced by a fallback.
    pub fallback: bool,
}

/// Apply `action` for the root seat, then let the other seats act by the partner model
/// until the root seat is to move again (or, near the end of the deck, until the game
/// ends, the root seat included). Returns the leaf value.
pub fn simulate(root: &View, world: &World, views: Vec<View>, action: Action) -> Breakdown {
    run(root, world, views, action, None)
}

/// [`simulate`], recording every step.
pub fn simulate_traced(
    root: &View,
    world: &World,
    views: Vec<View>,
    action: Action,
) -> (Breakdown, Vec<RolloutStep>) {
    let mut steps = Vec::new();
    let leaf = run(root, world, views, action, Some(&mut steps));
    (leaf, steps)
}

fn run(
    root: &View,
    world: &World,
    mut views: Vec<View>,
    action: Action,
    mut trace: Option<&mut Vec<RolloutStep>>,
) -> Breakdown {
    let variant = root.variant.clone();
    let me = root.me;
    let mut state = root.state;
    let to_end = plays_to_end(&state);
    let mut next = Some(action);
    // A guard against a cycle of clues that never ends the game.
    let mut steps = 0;
    while !state.is_over() && steps < 200 {
        let seat = state.current;
        let (predicted, reason) = match next.take() {
            Some(action) => (action, None),
            None if seat == me && !to_end => break,
            None => {
                let decision = partner_decision(&views[seat.index()]);
                (decision.action, Some(decision.reason))
            }
        };
        let command = legal_or_fallback(&variant, &state, world, seat, predicted);
        if let Some(trace) = trace.as_deref_mut() {
            trace.push(RolloutStep {
                seat,
                action: command,
                reason,
                fallback: command != predicted,
            });
        }
        for event in engine::step(&variant, &mut state, world, command) {
            for view in views.iter_mut() {
                view.observe(&event.observed_by(view.me));
            }
        }
        steps += 1;
    }
    value::evaluate(&variant, &state, world, &views)
}

/// A partner prediction can be illegal in a sampled world (an inconsistent belief); the
/// seat then discards its oldest card, or plays it at maximum tokens.
fn legal_or_fallback(
    variant: &crate::variants::Variant,
    state: &GameState,
    world: &World,
    seat: Seat,
    action: Action,
) -> Action {
    let legal = engine::check_legal(variant, state, action).is_ok()
        && match action {
            Action::Clue { target, clue } => {
                !engine::touched_cards(variant, state, world, target, clue).is_empty()
            }
            _ => true,
        };
    if legal {
        return action;
    }
    let oldest = state.hand(seat)[0];
    if state.tokens < variant.tokens.max {
        Action::Discard(oldest)
    } else {
        Action::Play(oldest)
    }
}
