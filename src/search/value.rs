//! Leaf value (docs/architecture.md §7.6): an estimate of the final team score, as a sum
//! of named terms.
//!
//! Terminal states use the real score (0 after a strikeout). A cut-off adds to the score a
//! linear estimate of the points still to come, from raw [`Features`] of the position: the
//! plays the team has already secured, cards believed playable that are not, the score
//! that can no longer be reached, strikes, exposed critical cards, clue tokens and the
//! cards left in the deck. Weights fitted by least squares against blueprint self-play
//! games played to the end (`raikan fit-value`) can be stored per player count in
//! `data/value-weights.json`; player counts without a fit use [`Weights::HAND_SET`]. The
//! file ships empty: fitted weights predicted the score better but decided worse.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::conventions::View;
use crate::domain::{CardSet, Seat};
use crate::engine::{EndReason, GameState, Status, World};
use crate::variants::Variant;

const WEIGHTS_JSON: &str = include_str!("../../data/value-weights.json");

/// Number of features, the constant included.
pub const NUM_FEATURES: usize = 11;

/// Raw features of a running position; [`Weights`] turn them into expected further points.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct Features {
    /// Plays the holders already know to make and that will succeed (chained).
    pub secured_plays: f64,
    /// Cards their holders will play although they do not fit after the secured plays.
    pub false_plays: f64,
    /// Points of maximum score lost to discards.
    pub lost_max_score: f64,
    /// Exactly one strike.
    pub strike_one: f64,
    /// Two strikes.
    pub strike_two: f64,
    /// Unprotected critical cards on chop.
    pub critical_chops: f64,
    /// Whole clue tokens.
    pub tokens: f64,
    /// Clue tokens times the fraction of the deck left: tokens matter more early.
    pub tokens_early: f64,
    /// Cards left in the deck (pace: every clue keeps a turn).
    pub cards_left: f64,
    /// Reachable points that no longer fit into the turns left (one play per turn:
    /// the cards left plus one turn per seat).
    pub shortfall: f64,
}

impl Features {
    /// The features as a vector, the constant last.
    pub fn vector(&self) -> [f64; NUM_FEATURES] {
        [
            self.secured_plays,
            self.false_plays,
            self.lost_max_score,
            self.strike_one,
            self.strike_two,
            self.critical_chops,
            self.tokens,
            self.tokens_early,
            self.cards_left,
            self.shortfall,
            1.0,
        ]
    }
}

/// Feature names, in [`Features::vector`] order.
pub const FEATURE_NAMES: [&str; NUM_FEATURES] = [
    "secured_plays",
    "false_plays",
    "lost_max_score",
    "strike_one",
    "strike_two",
    "critical_chops",
    "tokens",
    "tokens_early",
    "cards_left",
    "shortfall",
    "constant",
];

/// The sign the rules give each weight: `1` not negative, `-1` not positive, `0` free.
/// A strike, a lost card or a false play never helps; a secured play or a token never
/// hurts. The fit is observational, so without these a rare event that correlates with
/// the game phase can get the wrong sign.
pub const FEATURE_SIGNS: [i8; NUM_FEATURES] = [1, -1, -1, -1, -1, -1, 1, 1, 0, -1, 0];

/// Weights of the features, in expected further points per unit.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Weights {
    pub secured_plays: f64,
    pub false_plays: f64,
    pub lost_max_score: f64,
    pub strike_one: f64,
    pub strike_two: f64,
    pub critical_chops: f64,
    pub tokens: f64,
    pub tokens_early: f64,
    pub cards_left: f64,
    #[serde(default)]
    pub shortfall: f64,
    pub constant: f64,
}

impl Weights {
    /// The hand-set weights used before the fit, and for player counts without one.
    /// `secured_plays` is well below 1: with a one-rotation horizon a known play kept in
    /// hand would otherwise look almost as good as making it.
    pub const HAND_SET: Weights = Weights {
        secured_plays: 0.6,
        false_plays: -1.0,
        lost_max_score: -1.0,
        strike_one: -0.4,
        strike_two: -1.2,
        critical_chops: -0.6,
        tokens: 0.35,
        tokens_early: 0.0,
        cards_left: 0.0,
        shortfall: 0.0,
        constant: 0.0,
    };

    pub fn from_vector(w: &[f64; NUM_FEATURES]) -> Self {
        Self {
            secured_plays: w[0],
            false_plays: w[1],
            lost_max_score: w[2],
            strike_one: w[3],
            strike_two: w[4],
            critical_chops: w[5],
            tokens: w[6],
            tokens_early: w[7],
            cards_left: w[8],
            shortfall: w[9],
            constant: w[10],
        }
    }

    pub fn vector(&self) -> [f64; NUM_FEATURES] {
        [
            self.secured_plays,
            self.false_plays,
            self.lost_max_score,
            self.strike_one,
            self.strike_two,
            self.critical_chops,
            self.tokens,
            self.tokens_early,
            self.cards_left,
            self.shortfall,
            self.constant,
        ]
    }

    /// Expected further points of a position with `features`.
    pub fn predict(&self, features: &Features) -> f64 {
        self.vector()
            .iter()
            .zip(features.vector())
            .map(|(w, f)| w * f)
            .sum()
    }

    /// The weights for a table of `num_seats` players: fitted if `data/value-weights.json`
    /// has them, otherwise [`Weights::HAND_SET`].
    pub fn for_players(num_seats: u8) -> Weights {
        static TABLE: OnceLock<BTreeMap<u8, Weights>> = OnceLock::new();
        TABLE
            .get_or_init(|| parse_table(WEIGHTS_JSON).expect("data/value-weights.json is valid"))
            .get(&num_seats)
            .copied()
            .unwrap_or(Self::HAND_SET)
    }
}

/// The weights file: `{"players": {"3": {<weights>}, ...}}` (other keys are notes).
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct WeightsFile {
    #[serde(default)]
    pub players: BTreeMap<String, Weights>,
    #[serde(flatten)]
    pub notes: BTreeMap<String, serde_json::Value>,
}

fn parse_table(json: &str) -> Result<BTreeMap<u8, Weights>, String> {
    let file: WeightsFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
    file.players
        .into_iter()
        .map(|(k, w)| {
            k.parse::<u8>()
                .map(|n| (n, w))
                .map_err(|_| format!("invalid player count {k}"))
        })
        .collect()
}

/// Least-squares weights for `rows` of (features, target) under [`FEATURE_SIGNS`]: a weight
/// with the wrong sign is fixed at 0 and the rest refitted (active set), until every sign
/// holds.
pub fn fit(rows: &[([f64; NUM_FEATURES], f64)]) -> [f64; NUM_FEATURES] {
    fit_signed(rows, &FEATURE_SIGNS)
}

/// [`fit`] with explicit sign constraints.
pub fn fit_signed(
    rows: &[([f64; NUM_FEATURES], f64)],
    signs: &[i8; NUM_FEATURES],
) -> [f64; NUM_FEATURES] {
    let mut fixed = [false; NUM_FEATURES];
    loop {
        let w = least_squares(rows, &fixed);
        let worst = (0..NUM_FEATURES)
            .filter(|&i| !fixed[i] && (signs[i] as f64) * w[i] < 0.0)
            .max_by(|&a, &b| w[a].abs().total_cmp(&w[b].abs()));
        match worst {
            Some(i) => fixed[i] = true,
            None => return w,
        }
    }
}

/// Least squares with the `fixed` weights held at 0, and a tiny ridge term for numerical
/// stability (features that never vary get weight 0).
fn least_squares(
    rows: &[([f64; NUM_FEATURES], f64)],
    fixed: &[bool; NUM_FEATURES],
) -> [f64; NUM_FEATURES] {
    const N: usize = NUM_FEATURES;
    let mut a = [[0.0f64; N]; N];
    let mut b = [0.0f64; N];
    for (x, y) in rows {
        for i in 0..N {
            b[i] += x[i] * y;
            for j in 0..N {
                a[i][j] += x[i] * x[j];
            }
        }
    }
    for i in (0..N).filter(|&i| fixed[i]) {
        a[i] = [0.0; N];
        for row in a.iter_mut() {
            row[i] = 0.0;
        }
        a[i][i] = 1.0;
        b[i] = 0.0;
    }
    let ridge = 1e-6 * rows.len().max(1) as f64;
    for (i, row) in a.iter_mut().enumerate().take(N - 1) {
        row[i] += ridge;
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..N {
        let pivot = (col..N)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .expect("non-empty range");
        a.swap(col, pivot);
        b.swap(col, pivot);
        if a[col][col].abs() < 1e-12 {
            continue;
        }
        let pivot_row = a[col];
        for row in col + 1..N {
            let factor = a[row][col] / pivot_row[col];
            for (cell, p) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *cell -= factor * p;
            }
            b[row] -= factor * b[col];
        }
    }
    let mut w = [0.0f64; N];
    for col in (0..N).rev() {
        if a[col][col].abs() < 1e-12 {
            continue;
        }
        let rest: f64 = (col + 1..N).map(|k| a[col][k] * w[k]).sum();
        w[col] = (b[col] - rest) / a[col][col];
    }
    w
}

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
    /// Cards left in the deck, the pace shortfall and the constant of the fit.
    pub future: f64,
}

impl Breakdown {
    pub fn total(&self) -> f64 {
        self.terms().iter().map(|(_, v)| v).sum()
    }

    pub fn terms(&self) -> [(&'static str, f64); 8] {
        [
            ("score", self.score),
            ("secured_plays", self.secured_plays),
            ("false_plays", self.false_plays),
            ("lost_max_score", self.lost_max_score),
            ("strikes", self.strikes),
            ("critical_chops", self.critical_chops),
            ("tokens", self.tokens),
            ("future", self.future),
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
        self.future += other.future * factor;
    }

    /// Score plus the weighted features.
    pub fn weighted(score: f64, f: &Features, w: &Weights) -> Breakdown {
        Breakdown {
            score,
            secured_plays: f.secured_plays * w.secured_plays,
            false_plays: f.false_plays * w.false_plays,
            lost_max_score: f.lost_max_score * w.lost_max_score,
            strikes: f.strike_one * w.strike_one + f.strike_two * w.strike_two,
            critical_chops: f.critical_chops * w.critical_chops,
            tokens: f.tokens * w.tokens + f.tokens_early * w.tokens_early,
            future: f.cards_left * w.cards_left + f.shortfall * w.shortfall + w.constant,
        }
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
            Breakdown::default()
        }
        Status::Over(EndReason::Normal) => Breakdown {
            score,
            ..Breakdown::default()
        },
        Status::Running => Breakdown::weighted(
            score,
            &features(variant, state, world, views),
            &Weights::for_players(state.num_seats),
        ),
    }
}

/// The raw features of a running position (see [`evaluate`]).
pub fn features(variant: &Variant, state: &GameState, world: &World, views: &[View]) -> Features {
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

    let dead = variant.trash(&state.stacks, &state.discarded);
    let mut critical_chops = 0.0;
    for seat in state.seats() {
        if let Some(chop) = chop(state, &views[seat.index()], seat) {
            let identity = world.identity(chop);
            let left = variant
                .copies(identity)
                .saturating_sub(state.discarded[identity.index()]);
            if left == 1 && !dead.contains(identity) {
                critical_chops += 1.0;
            }
        }
    }
    let tokens = state.clue_tokens() as f64;
    let deck_fraction = state.cards_left() as f64 / variant.num_cards().max(1) as f64;
    Features {
        secured_plays: secured as f64,
        false_plays: pending.len() as f64,
        lost_max_score: (variant.max_score() - max) as f64,
        strike_one: (state.strikes == 1) as u8 as f64,
        strike_two: (state.strikes >= 2) as u8 as f64,
        critical_chops,
        tokens,
        tokens_early: tokens * deck_fraction,
        cards_left: state.cards_left() as f64,
        shortfall: (max.saturating_sub(state.score(variant)) as f64
            - (state.cards_left() as f64 + state.num_seats as f64))
            .max(0.0),
    }
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
    fn fit_recovers_known_weights() {
        let truth = [0.9, -1.1, -0.8, -0.3, -1.5, -0.4, 0.2, 0.5, 0.05, -0.7, 3.0];
        let mut rows = Vec::new();
        let mut x = 12345u64;
        let mut next = || {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (x >> 33) as f64 / (1u64 << 31) as f64
        };
        for _ in 0..2000 {
            let mut f = [0.0; NUM_FEATURES];
            for v in f.iter_mut().take(NUM_FEATURES - 1) {
                *v = (next() * 5.0).floor();
            }
            f[NUM_FEATURES - 1] = 1.0;
            let y: f64 = f.iter().zip(truth).map(|(a, b)| a * b).sum();
            rows.push((f, y));
        }
        let w = fit(&rows);
        for (got, want) in w.iter().zip(truth) {
            assert!((got - want).abs() < 1e-3, "{w:?}");
        }
    }

    #[test]
    fn fit_respects_signs() {
        // y = 2·x0 − x1: forbidding a positive x0 weight leaves x0 at 0.
        let rows: Vec<_> = (0..50)
            .map(|i| {
                let mut f = [0.0; NUM_FEATURES];
                f[0] = (i % 7) as f64;
                f[1] = (i % 5) as f64;
                f[NUM_FEATURES - 1] = 1.0;
                (f, 2.0 * f[0] - f[1])
            })
            .collect();
        let mut signs = [0i8; NUM_FEATURES];
        signs[0] = -1;
        let w = fit_signed(&rows, &signs);
        assert_eq!(w[0], 0.0);
        assert!((w[1] + 1.0).abs() < 0.1, "{w:?}");
    }

    #[test]
    fn weights_file_parses() {
        parse_table(WEIGHTS_JSON).unwrap();
        assert_eq!(Weights::for_players(6).vector().len(), NUM_FEATURES);
    }

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
