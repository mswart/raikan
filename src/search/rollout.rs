//! One simulation of a root candidate in one sampled world.
//!
//! Every seat acts from its own view and reads every event through its own interpreter:
//! our command is applied as a plain command (its intent is never transmitted, §7.1), the
//! other seats follow the partner model, and draws come from the sampled deck.

use crate::conventions::View;
use crate::domain::Seat;
use crate::engine::{self, Action, GameState, World};
use crate::policy::Reason;
use crate::policy::partner::{Prediction, partner_distribution, pick};

use super::SearchConfig;
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

/// Whether the rollouts of a decision play to the real end of the game instead of one
/// rotation: the final round has started, the deck is nearly empty, or the position is in
/// the end-game (pace below the number of players plus `config.to_end_pace`; the H-Group
/// end-game definition, independent of the level-8 rule). The leaf is then the real final
/// score, not an estimate.
pub fn plays_to_end(root: &View, config: &SearchConfig) -> bool {
    let state = &root.state;
    state.final_turns.is_some()
        || state.cards_left() <= state.num_seats
        || root.pace() < state.num_seats as i16 + config.to_end_pace
}

/// One step of a traced rollout.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RolloutStep {
    pub seat: Seat,
    /// The command actually applied.
    pub action: Action,
    /// The partner model's reason; `None` for the root candidate and for alternatives.
    pub reason: Option<Reason>,
    /// The partner model drew an alternative to the blueprint's choice (its spread).
    pub alternative: bool,
    /// The predicted action was illegal in this world and replaced by a fallback.
    pub fallback: bool,
}

/// How the other seats act in one rollout: the partner model's spread and the random
/// stream for its draws. The stream depends only on the world and the step, so every
/// candidate meets the same draws (common random numbers).
#[derive(Clone, Copy, Debug)]
pub struct Partners {
    pub spread: f64,
    pub seed: u64,
}

impl Partners {
    /// The blueprint alone.
    pub const BLUEPRINT: Partners = Partners {
        spread: 0.0,
        seed: 0,
    };

    /// The stream of world `w` of a decision with seed `seed`.
    pub fn for_world(spread: f64, seed: u64, w: usize) -> Self {
        Self {
            spread,
            seed: splitmix(seed ^ splitmix(w as u64 + 1)),
        }
    }

    fn uniform(&self, step: usize) -> f64 {
        (splitmix(self.seed ^ (step as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 11) as f64
            / (1u64 << 53) as f64
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Apply `action` for the root seat, then let the other seats act by the partner model
/// until the root seat is to move again (or, with `to_end`, until the game ends, the root
/// seat included, acting by the blueprint from its own view). Returns the leaf value.
pub fn simulate(
    root: &View,
    world: &World,
    views: Vec<View>,
    action: Action,
    partners: Partners,
    to_end: bool,
) -> Breakdown {
    run(root, world, views, action, partners, to_end, None)
}

/// [`simulate`], recording every step.
pub fn simulate_traced(
    root: &View,
    world: &World,
    views: Vec<View>,
    action: Action,
    partners: Partners,
    to_end: bool,
) -> (Breakdown, Vec<RolloutStep>) {
    let mut steps = Vec::new();
    let leaf = run(
        root,
        world,
        views,
        action,
        partners,
        to_end,
        Some(&mut steps),
    );
    (leaf, steps)
}

fn run(
    root: &View,
    world: &World,
    mut views: Vec<View>,
    action: Action,
    partners: Partners,
    to_end: bool,
    mut trace: Option<&mut Vec<RolloutStep>>,
) -> Breakdown {
    let mut state = root.state;
    let mut steps = 0;
    advance(
        root,
        world,
        &mut state,
        &mut views,
        Some(action),
        partners,
        to_end,
        &mut trace,
        &mut steps,
    );
    value::evaluate(&root.variant, &state, world, &views)
}

/// One candidate rolled out in one world twice over: the position where the search cuts
/// the rollout off (our next turn), and the real end of the game from there, every seat
/// (ours included) acting by the partner model from its own view.
#[derive(Clone, Copy, Debug)]
pub struct PairedOutcome {
    /// Score and leaf features at the cut-off; `None` if the game already ended.
    pub cutoff: Option<(f64, value::Features)>,
    /// The final score (0 after a strikeout).
    pub final_score: f64,
}

/// [`simulate`] cut off at our next turn, then continued to the end: the data a leaf value
/// is fitted on (`raikan fit-value --paired`).
pub fn simulate_paired(
    root: &View,
    world: &World,
    mut views: Vec<View>,
    action: Action,
    partners: Partners,
) -> PairedOutcome {
    let variant = &root.variant;
    let mut state = root.state;
    let mut steps = 0;
    let mut trace = None;
    advance(
        root,
        world,
        &mut state,
        &mut views,
        Some(action),
        partners,
        false,
        &mut trace,
        &mut steps,
    );
    let cutoff = (!state.is_over()).then(|| {
        (
            state.score(variant) as f64,
            value::features(variant, &state, world, &views),
        )
    });
    advance(
        root, world, &mut state, &mut views, None, partners, true, &mut trace, &mut steps,
    );
    PairedOutcome {
        cutoff,
        final_score: value::evaluate(variant, &state, world, &views).total(),
    }
}

/// Step the game from `state`: `next` for the seat to move, then the partner model, until
/// our next turn (or, with `to_end`, the end of the game).
#[allow(clippy::too_many_arguments)]
fn advance(
    root: &View,
    world: &World,
    state: &mut GameState,
    views: &mut [View],
    mut next: Option<Action>,
    partners: Partners,
    to_end: bool,
    trace: &mut Option<&mut Vec<RolloutStep>>,
    steps: &mut usize,
) {
    let variant = root.variant.clone();
    let me = root.me;
    // A guard against a cycle of clues that never ends the game.
    while !state.is_over() && *steps < 400 {
        let seat = state.current;
        let prediction = match next.take() {
            Some(action) => Prediction {
                action,
                probability: 1.0,
                reason: None,
            },
            None if seat == me && !to_end => break,
            None => {
                let distribution = partner_distribution(&views[seat.index()], partners.spread);
                pick(&distribution, partners.uniform(*steps))
            }
        };
        let predicted = prediction.action;
        let alternative = *steps > 0 && prediction.reason.is_none();
        let reason = prediction.reason;
        let command = legal_or_fallback(&variant, state, world, seat, predicted);
        if let Some(trace) = trace.as_deref_mut() {
            trace.push(RolloutStep {
                seat,
                action: command,
                reason,
                alternative,
                fallback: command != predicted,
            });
        }
        for event in engine::step(&variant, state, world, command) {
            for view in views.iter_mut() {
                view.observe(&event.observed_by(view.me));
            }
        }
        *steps += 1;
    }
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
