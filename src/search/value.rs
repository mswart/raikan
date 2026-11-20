//! Leaf value (docs/architecture.md §7.6): an estimate of the final team score, as a sum
//! of named terms.
//!
//! Terminal states use the real score (0 after a strikeout). A cut-off adds the plays the
//! team has already secured and subtracts the score that can no longer be reached and the
//! risk of strikes and exposed critical cards. The weights are hand-set: chosen by paired
//! self-play (3 players, fast search) among a few settings; fitting them against blueprint
//! games played to the end is open.

use crate::conventions::View;
use crate::domain::{CardSet, Seat};
use crate::engine::{EndReason, GameState, Status, World};
use crate::variants::Variant;

/// Weight of a play the holder already knows to make and that will succeed. Well below 1:
/// with a one-rotation horizon a known play kept in hand would otherwise look almost as
/// good as making it, and clues would crowd out plays.
const SECURED_PLAY: f64 = 0.6;
/// A card its holder will play although it does not fit the stacks after the secured
/// plays (a likely strike, and possibly a lost card).
const FALSE_PLAY: f64 = -1.0;
/// Per point of maximum score lost to discards.
const LOST_POINT: f64 = -1.0;
/// Risk of a later strikeout, by strikes so far.
const STRIKES: [f64; 3] = [0.0, -0.4, -1.2];
/// An unprotected critical card on chop.
const CRITICAL_CHOP: f64 = -0.6;
/// Per whole clue token: a token is the means to secure a later play, so it must not be
/// nearly free next to a secured play.
const TOKEN: f64 = 0.35;

/// The value of one leaf, by term. Each field is already weighted; the value is their sum.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct Breakdown {
    pub score: f64,
    pub secured_plays: f64,
    pub false_plays: f64,
    pub lost_max_score: f64,
    pub strikes: f64,
    pub critical_chops: f64,
    pub tokens: f64,
}

impl Breakdown {
    pub fn total(&self) -> f64 {
        self.terms().iter().map(|(_, v)| v).sum()
    }

    pub fn terms(&self) -> [(&'static str, f64); 7] {
        [
            ("score", self.score),
            ("secured_plays", self.secured_plays),
            ("false_plays", self.false_plays),
            ("lost_max_score", self.lost_max_score),
            ("strikes", self.strikes),
            ("critical_chops", self.critical_chops),
            ("tokens", self.tokens),
        ]
    }

    pub fn add_scaled(&mut self, other: &Breakdown, factor: f64) {
        self.score += other.score * factor;
        self.secured_plays += other.secured_plays * factor;
        self.false_plays += other.false_plays * factor;
        self.lost_max_score += other.lost_max_score * factor;
        self.strikes += other.strikes * factor;
        self.critical_chops += other.critical_chops * factor;
        self.tokens += other.tokens * factor;
    }
}

/// The highest score still reachable given the discards.
pub fn max_reachable(variant: &Variant, state: &GameState) -> u8 {
    variant.max_reachable(&state.stacks, &state.discarded)
}

/// Value of the position `state` in `world`. `views[s]` is seat `s`'s own view; it decides
/// which cards that seat already knows to play or protects.
pub fn evaluate(variant: &Variant, state: &GameState, world: &World, views: &[View]) -> Breakdown {
    let score = state.score(variant) as f64;
    match state.status {
        Status::Over(EndReason::Strikeout) | Status::Over(EndReason::Illegal) => {
            return Breakdown::default();
        }
        Status::Over(EndReason::Normal) => {
            return Breakdown {
                score,
                ..Breakdown::default()
            };
        }
        Status::Running => {}
    }
    let mut value = Breakdown {
        score,
        ..Breakdown::default()
    };

    // Plays the holders will make: chain them on a copy of the stacks.
    let mut stacks = state.stacks;
    let mut pending: CardSet = state
        .seats()
        .flat_map(|seat| {
            let view = &views[seat.index()];
            state
                .hand(seat)
                .iter()
                .copied()
                .filter(move |&c| view.note(c).play && !view.note(c).trash)
        })
        .collect();
    let mut secured = 0u8;
    loop {
        let playable = variant.playable(&stacks);
        let next = pending
            .iter()
            .find(|&c| playable.contains(world.identity(c)));
        let Some(card) = next else { break };
        let identity = world.identity(card);
        variant.play(&mut stacks, identity);
        pending.remove(card);
        secured += 1;
    }
    let max = max_reachable(variant, state);
    let secured = secured.min(max.saturating_sub(state.score(variant)));
    value.secured_plays = secured as f64 * SECURED_PLAY;
    // Cards believed playable that do not fit even after the secured plays.
    value.false_plays = pending.len() as f64 * FALSE_PLAY;
    value.lost_max_score = (variant.max_score() - max) as f64 * LOST_POINT;
    value.strikes = STRIKES[(state.strikes as usize).min(2)];
    value.tokens = state.clue_tokens() as f64 * TOKEN;

    let dead = variant.trash(&state.stacks, &state.discarded);
    for seat in state.seats() {
        if let Some(chop) = chop(state, &views[seat.index()], seat) {
            let identity = world.identity(chop);
            let left = variant
                .copies(identity)
                .saturating_sub(state.discarded[identity.index()]);
            if left == 1 && !dead.contains(identity) {
                value.critical_chops += CRITICAL_CHOP;
            }
        }
    }
    value
}

/// The oldest card its holder does not protect.
fn chop(state: &GameState, holder: &View, seat: Seat) -> Option<crate::domain::CardId> {
    let protected = holder.protected(seat);
    state
        .hand(seat)
        .iter()
        .copied()
        .find(|&c| !protected.contains(c) && !holder.note(c).trash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{deal, shuffled_world};

    #[test]
    fn max_reachable_drops_after_lost_card() {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 1);
        let (mut state, _) = deal(&variant, 3, &world);
        assert_eq!(max_reachable(&variant, &state), 25);
        let r3 = variant.identity(0, 3);
        state.discarded[r3.index()] = 2;
        assert_eq!(max_reachable(&variant, &state), 22);
        let b5 = variant.identity(4, 5);
        state.discarded[b5.index()] = 1;
        assert_eq!(max_reachable(&variant, &state), 21);
    }
}
