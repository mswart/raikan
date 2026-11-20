//! The first planner (docs/architecture.md §7.4): spend the whole budget at the root.
//!
//! 0. Candidates are the blueprint's candidates that pass the legibility guard
//!    ([`crate::policy::guard`]), plus the blueprint's own choice.
//! 1. Sample the worlds once and evaluate every candidate on the same ones (common random
//!    numbers), so candidates are compared pairwise on identical hidden cards.
//! 2. Sequential halving over all candidates: each gets a few worlds, the weaker half is
//!    dropped, the rest gets more. The blueprint's choice is never dropped, because the
//!    final comparison is paired against it.
//! 3. The best estimate replaces the blueprint's choice only when it is better by more
//!    than `blueprint_margin` paired standard errors.

use std::hash::{Hash, Hasher};

use crate::conventions::View;
use crate::engine::Action;
use crate::knowledge::{ExactSampler, MAX_EXACT_CONSTRAINED, SampleSet, WorldSampler};
use crate::policy::{Decision, permitted_candidates};

use super::SearchConfig;
use super::rollout::{seat_views, simulate};
use super::value::Breakdown;

/// Why the planner chose its action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PlanReason {
    /// Only one permitted candidate.
    Single,
    /// No world satisfies the physical constraints (or too many constrained cards).
    NoWorlds,
    /// The best estimate is the blueprint's choice, or not distinguishable from it.
    KeptBlueprint,
    /// The best estimate beat the blueprint's choice by more than the margin.
    Searched,
}

/// The statistics of one candidate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CandidateStats {
    pub action: Action,
    /// Position in the evaluation order; 0 is the blueprint's choice.
    pub blueprint_rank: usize,
    /// Leaf values, one per evaluated world (worlds `0..visits`).
    #[serde(skip)]
    pub values: Vec<f64>,
    pub visits: usize,
    pub mean: f64,
    pub stderr: f64,
    /// Weighted mean of the leaf terms.
    pub breakdown: Breakdown,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PlanResult {
    pub action: Action,
    pub reason: PlanReason,
    pub candidates: Vec<CandidateStats>,
    pub worlds: usize,
    pub effective_size: f64,
    /// Paired difference best − blueprint and its standard error, when they differ.
    pub paired_difference: Option<(f64, f64)>,
    pub seed: u64,
}

/// Weighted mean and standard error of `values` (with the first `values.len()` weights).
fn weighted_stats(values: &[f64], weights: &[f64]) -> (f64, f64) {
    let weights = &weights[..values.len()];
    let total: f64 = weights.iter().sum();
    if total == 0.0 {
        return (0.0, 0.0);
    }
    let mean = values.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>() / total;
    let variance = values
        .iter()
        .zip(weights)
        .map(|(v, w)| w * (v - mean) * (v - mean))
        .sum::<f64>()
        / total;
    let sum_sq: f64 = weights.iter().map(|w| w * w).sum();
    let effective = total * total / sum_sq;
    let stderr = if effective > 1.0 {
        (variance / (effective - 1.0)).sqrt()
    } else {
        f64::INFINITY
    };
    (mean, stderr)
}

/// The seed of a decision: fixed by the configuration, otherwise derived from the public
/// state (which includes the turn), so a decision is reproducible from the replay prefix.
fn decision_seed(view: &View, config: &SearchConfig) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    view.state.hash(&mut hasher);
    view.me.hash(&mut hasher);
    config.seed.unwrap_or(0).hash(&mut hasher);
    hasher.finish()
}

fn sample(view: &View, config: &SearchConfig, seed: u64) -> SampleSet {
    let store = view.constraint_store();
    if store.constrained().len() > MAX_EXACT_CONSTRAINED {
        return SampleSet::default();
    }
    ExactSampler::new(&store, config.epsilon).sample(config.worlds as usize, seed)
}

/// Choose an action for `view.me` by simulation. `blueprint` is the blueprint's decision
/// in the same view; it is the reference the search must beat.
pub fn plan(view: &View, blueprint: &Decision, config: &SearchConfig) -> PlanResult {
    // The blueprint's choice is the reference even when the guard would reject it (an
    // emergency); every other candidate must be legible (§7.5).
    let mut actions: Vec<Action> = vec![blueprint.action];
    actions.extend(
        permitted_candidates(view)
            .into_iter()
            .filter(|&a| a != blueprint.action),
    );
    let seed = decision_seed(view, config);
    let mut stats: Vec<CandidateStats> = actions
        .iter()
        .enumerate()
        .map(|(rank, &action)| CandidateStats {
            action,
            blueprint_rank: rank,
            values: Vec::new(),
            visits: 0,
            mean: 0.0,
            stderr: 0.0,
            breakdown: Breakdown::default(),
        })
        .collect();
    let result = |reason, stats, worlds, effective_size, paired_difference| PlanResult {
        action: blueprint.action,
        reason,
        candidates: stats,
        worlds,
        effective_size,
        paired_difference,
        seed,
    };
    if actions.len() == 1 {
        return result(PlanReason::Single, stats, 0, 0.0, None);
    }
    let samples = sample(view, config, seed);
    if samples.worlds.is_empty() {
        return result(PlanReason::NoWorlds, stats, 0, 0.0, None);
    }
    let worlds = samples.worlds.len();
    let weights = &samples.weights;
    let mut root_views: Vec<Option<Vec<View>>> = vec![None; worlds];
    let mut breakdowns: Vec<Breakdown> = vec![Breakdown::default(); actions.len()];

    // Sequential halving.
    let rounds = (actions.len() as f64).log2().ceil().max(1.0) as usize;
    let per_round = (config.simulations as usize / rounds).max(1);
    let mut alive: Vec<usize> = (0..actions.len()).collect();
    for round in 0..rounds {
        let target_add = (per_round / alive.len()).max(2);
        for &c in &alive {
            let start = stats[c].values.len();
            let end = (start + target_add).min(worlds);
            for w in start..end {
                let world = &samples.worlds[w];
                let views = root_views[w]
                    .get_or_insert_with(|| seat_views(view, world))
                    .clone();
                let leaf = simulate(view, world, views, actions[c]);
                breakdowns[c].add_scaled(&leaf, weights[w]);
                stats[c].values.push(leaf.total());
            }
            let (mean, stderr) = weighted_stats(&stats[c].values, weights);
            stats[c].mean = mean;
            stats[c].stderr = stderr;
            stats[c].visits = stats[c].values.len();
        }
        if round + 1 == rounds || alive.len() <= 2 {
            continue;
        }
        alive.sort_by(|&a, &b| stats[b].mean.total_cmp(&stats[a].mean).then(a.cmp(&b)));
        let keep = alive.len().div_ceil(2);
        let mut next: Vec<usize> = alive[..keep].to_vec();
        if !next.contains(&0) {
            next.push(0);
        }
        next.sort();
        alive = next;
    }
    for (c, candidate) in stats.iter_mut().enumerate() {
        let total: f64 = weights[..candidate.visits].iter().sum();
        if total > 0.0 {
            candidate.breakdown.add_scaled(&breakdowns[c], 1.0 / total);
        }
    }

    let best = alive
        .iter()
        .copied()
        .max_by(|&a, &b| stats[a].mean.total_cmp(&stats[b].mean).then(b.cmp(&a)))
        .expect("the blueprint candidate stays alive");
    let effective_size = samples.effective_size();
    if best == 0 {
        return result(
            PlanReason::KeptBlueprint,
            stats,
            worlds,
            effective_size,
            None,
        );
    }
    let n = stats[best].visits.min(stats[0].visits);
    let differences: Vec<f64> = (0..n)
        .map(|w| stats[best].values[w] - stats[0].values[w])
        .collect();
    let (difference, stderr) = weighted_stats(&differences, weights);
    let paired = Some((difference, stderr));
    if difference > 0.0 && difference > config.blueprint_margin * stderr {
        let action = actions[best];
        let mut planned = result(PlanReason::Searched, stats, worlds, effective_size, paired);
        planned.action = action;
        planned
    } else {
        result(
            PlanReason::KeptBlueprint,
            stats,
            worlds,
            effective_size,
            paired,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_stats_of_constant_values() {
        let (mean, stderr) = weighted_stats(&[2.0, 2.0, 2.0], &[1.0, 1.0, 1.0, 5.0]);
        assert_eq!(mean, 2.0);
        assert_eq!(stderr, 0.0);
    }

    #[test]
    fn weighted_stats_respects_weights() {
        let (mean, _) = weighted_stats(&[0.0, 10.0], &[3.0, 1.0]);
        assert!((mean - 2.5).abs() < 1e-12);
    }
}
