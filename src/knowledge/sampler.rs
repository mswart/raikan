//! Joint, unbiased world sampling.
//!
//! Only the constrained cards (narrowed domains) need care: a dynamic programme over
//! identities × subset of constrained cards already assigned counts the physical deals
//! exactly (weight `copies_left falling |subset|`), and a world is drawn backwards from it.
//! The remaining copies are then shuffled onto the unconstrained cards.
//!
//! Convention constraints are weights: a world satisfying all of them has weight 1, a world
//! satisfying only the physical constraints weight `epsilon`. `Restrict` constraints are
//! folded into a second, trusting table so that the sampler stays exact for them; `Located`
//! constraints only enter as weights.

use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};

use crate::domain::{CardId, Identity, IdentitySet, MAX_CARDS, MAX_IDENTITIES};
use crate::engine::World;

use super::ConstraintStore;

/// The largest number of constrained cards the exact table supports.
pub const MAX_EXACT_CONSTRAINED: usize = 16;

/// Sampled worlds with importance weights.
#[derive(Clone, Debug, Default)]
pub struct SampleSet {
    pub worlds: Vec<World>,
    pub weights: Vec<f64>,
}

impl SampleSet {
    /// Effective sample size (Kish).
    pub fn effective_size(&self) -> f64 {
        let sum: f64 = self.weights.iter().sum();
        let sum_sq: f64 = self.weights.iter().map(|w| w * w).sum();
        if sum_sq == 0.0 {
            0.0
        } else {
            sum * sum / sum_sq
        }
    }

    /// Weighted probability that `card` has an identity in `set`.
    pub fn probability(&self, card: CardId, set: IdentitySet) -> f64 {
        let total: f64 = self.weights.iter().sum();
        if total == 0.0 {
            return 0.0;
        }
        self.worlds
            .iter()
            .zip(&self.weights)
            .filter(|(w, _)| set.contains(w.identity(card)))
            .map(|(_, weight)| weight)
            .sum::<f64>()
            / total
    }
}

pub trait WorldSampler {
    /// Draw `count` worlds deterministically from `seed`.
    fn sample(&self, count: usize, seed: u64) -> SampleSet;
}

/// Counting table for one set of domains.
#[derive(Clone, Debug)]
struct Table {
    /// Constrained cards, in table bit order.
    cards: Vec<CardId>,
    /// `table[i][mask]`: weight of assigning the constrained cards outside `mask` using
    /// identities `i..`.
    table: Vec<Vec<f64>>,
    /// For each identity, the constrained cards whose domain contains it.
    holders: [u32; MAX_IDENTITIES],
}

fn falling(n: u8, k: u32) -> f64 {
    (0..k).map(|j| (n as f64) - j as f64).product()
}

impl Table {
    fn build(
        cards: Vec<CardId>,
        domains: &[IdentitySet; MAX_CARDS],
        remaining: &[u8; MAX_IDENTITIES],
    ) -> Self {
        let n = cards.len();
        assert!(
            n <= MAX_EXACT_CONSTRAINED,
            "{n} constrained cards exceed the exact sampler"
        );
        let full = (1u32 << n) - 1;
        let mut holders = [0u32; MAX_IDENTITIES];
        for (bit, card) in cards.iter().enumerate() {
            for identity in domains[card.index()].iter() {
                holders[identity.index()] |= 1 << bit;
            }
        }
        let size = 1usize << n;
        let mut table = vec![vec![0.0; size]; MAX_IDENTITIES + 1];
        table[MAX_IDENTITIES][full as usize] = 1.0;
        for i in (0..MAX_IDENTITIES).rev() {
            let (lower, upper) = table.split_at_mut(i + 1);
            let (current, next) = (&mut lower[i], &upper[0]);
            let r = remaining[i];
            for mask in 0..size as u32 {
                let free = holders[i] & !mask & full;
                let mut total = 0.0;
                // enumerate subsets of `free` with at most `r` cards
                let mut sub = free;
                loop {
                    let k = sub.count_ones();
                    if k <= r as u32 {
                        let rest = next[(mask | sub) as usize];
                        if rest != 0.0 {
                            total += falling(r, k) * rest;
                        }
                    }
                    if sub == 0 {
                        break;
                    }
                    sub = (sub - 1) & free;
                }
                current[mask as usize] = total;
            }
        }
        Self {
            cards,
            table,
            holders,
        }
    }

    fn total(&self) -> f64 {
        self.table[0][0]
    }

    /// Draw an assignment of the constrained cards; returns the copies used per identity.
    fn draw(
        &self,
        rng: &mut impl RngExt,
        remaining: &[u8; MAX_IDENTITIES],
        world: &mut World,
    ) -> [u8; MAX_IDENTITIES] {
        let full = (1u32 << self.cards.len()) - 1;
        let mut used = [0u8; MAX_IDENTITIES];
        let mut mask = 0u32;
        for i in 0..MAX_IDENTITIES {
            let total = self.table[i][mask as usize];
            let r = remaining[i];
            let free = self.holders[i] & !mask & full;
            let mut target = rng.random::<f64>() * total;
            let mut chosen = 0;
            let mut sub = free;
            loop {
                let k = sub.count_ones();
                if k <= r as u32 {
                    let weight = falling(r, k) * self.table[i + 1][(mask | sub) as usize];
                    if weight > 0.0 {
                        chosen = sub;
                        if target < weight {
                            break;
                        }
                        target -= weight;
                    }
                }
                if sub == 0 {
                    break;
                }
                sub = (sub - 1) & free;
            }
            for bit in 0..self.cards.len() {
                if chosen & (1 << bit) != 0 {
                    world.set(self.cards[bit], Identity(i as u8));
                }
            }
            used[i] = chosen.count_ones() as u8;
            mask |= chosen;
        }
        debug_assert_eq!(mask, full);
        used
    }
}

/// Exact sampler for up to [`MAX_EXACT_CONSTRAINED`] constrained cards.
#[derive(Clone, Debug)]
pub struct ExactSampler {
    store: ConstraintStore,
    physical: Table,
    trusting: Option<Table>,
    epsilon: f64,
}

impl ExactSampler {
    /// Build the tables. `epsilon` is the weight of a world that violates a convention
    /// constraint.
    pub fn new(store: &ConstraintStore, epsilon: f64) -> Self {
        let constrained: Vec<CardId> = store.constrained().iter().collect();
        let physical = Table::build(constrained, &store.domains, &store.remaining);
        let trusting = if store
            .convention
            .iter()
            .any(|c| matches!(c, super::ConventionConstraint::Restrict { .. }))
        {
            let domains = store.trusting_domains();
            let available = store.available();
            let cards: Vec<CardId> = store
                .unknown
                .iter()
                .filter(|c| domains[c.index()] != available)
                .collect();
            (cards.len() <= MAX_EXACT_CONSTRAINED)
                .then(|| Table::build(cards, &domains, &store.remaining))
        } else {
            None
        };
        Self {
            store: store.clone(),
            physical,
            trusting,
            epsilon,
        }
    }

    /// Number of physically possible deals (up to the constant factor of the deck order).
    pub fn physical_weight(&self) -> f64 {
        self.physical.total()
    }

    fn complete(&self, rng: &mut rand_pcg::Pcg64, table: &Table) -> World {
        let mut identities = [Identity(0); MAX_CARDS];
        for (card, slot) in identities.iter_mut().enumerate() {
            if let Some(identity) = self.store.known.get(CardId(card as u8)) {
                *slot = identity;
            }
        }
        let num_cards = self.store.num_cards as usize;
        let mut world = World::new(&identities[..num_cards]);
        let used = table.draw(rng, &self.store.remaining, &mut world);
        let mut pool = Vec::new();
        for (i, (&used, &remaining)) in used.iter().zip(&self.store.remaining).enumerate() {
            for _ in used..remaining {
                pool.push(Identity(i as u8));
            }
        }
        pool.shuffle(rng);
        let mut pool = pool.into_iter();
        for card in self.store.unknown.iter() {
            if !table.cards.contains(&card) {
                world.set(card, pool.next().expect("copies match unknown cards"));
            }
        }
        world
    }
}

impl ExactSampler {
    fn satisfies_restricts(&self, world: &World) -> bool {
        self.store
            .convention
            .iter()
            .filter(|c| matches!(c, super::ConventionConstraint::Restrict { .. }))
            .all(|c| c.satisfied_by(|card| world.identity(card)))
    }
}

impl WorldSampler for ExactSampler {
    fn sample(&self, count: usize, seed: u64) -> SampleSet {
        let mut rng = rand_pcg::Pcg64::seed_from_u64(seed);
        let mut set = SampleSet::default();
        if self.physical.total() == 0.0 {
            return set;
        }
        let (z_trust, z_all) = match &self.trusting {
            Some(t) => (t.total(), self.physical.total()),
            None => (0.0, self.physical.total()),
        };
        for _ in 0..count {
            let world = match &self.trusting {
                // Mixture of the convention-satisfying part (mass z_trust) and the rest
                // (mass epsilon * (z_all - z_trust)), sampled exactly for Restrict.
                Some(trusting) if z_trust > 0.0 => {
                    let p_trust = z_trust / (z_trust + self.epsilon * (z_all - z_trust));
                    if rng.random::<f64>() < p_trust {
                        self.complete(&mut rng, trusting)
                    } else {
                        // the violating part: physical worlds rejected while they satisfy
                        // every Restrict constraint
                        let mut tries = 0;
                        loop {
                            let world = self.complete(&mut rng, &self.physical);
                            tries += 1;
                            if !self.satisfies_restricts(&world) || tries >= 1000 {
                                break world;
                            }
                        }
                    }
                }
                _ => self.complete(&mut rng, &self.physical),
            };
            let ok = self.store.satisfies_conventions(|c| world.identity(c));
            let weight = if self.trusting.is_some() && z_trust > 0.0 {
                // Restrict constraints are accounted for by the mixture; Located ones by weight.
                let located_ok = self
                    .store
                    .convention
                    .iter()
                    .filter(|c| matches!(c, super::ConventionConstraint::Located { .. }))
                    .all(|c| c.satisfied_by(|card| world.identity(card)));
                if located_ok { 1.0 } else { self.epsilon }
            } else if ok {
                1.0
            } else {
                self.epsilon
            };
            set.worlds.push(world);
            set.weights.push(weight);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CardSet;
    use crate::knowledge::{ConventionConstraint, KnownCards};
    use std::collections::HashMap;

    /// A tiny deck: identities 0 (3 copies), 1 (2 copies), 2 (1 copy) over 6 unknown cards.
    fn tiny_store(domains: &[(u8, &[u8])]) -> ConstraintStore {
        let mut remaining = [0u8; MAX_IDENTITIES];
        remaining[0] = 3;
        remaining[1] = 2;
        remaining[2] = 1;
        let mut all = [IdentitySet::EMPTY; MAX_CARDS];
        let available: IdentitySet = [Identity(0), Identity(1), Identity(2)]
            .into_iter()
            .collect();
        for domain in all.iter_mut().take(6) {
            *domain = available;
        }
        for &(card, ids) in domains {
            all[card as usize] = ids.iter().map(|&i| Identity(i)).collect();
        }
        ConstraintStore {
            unknown: CardSet::first(6),
            domains: all,
            remaining,
            known: KnownCards::default(),
            convention: Vec::new(),
            num_cards: 6,
        }
    }

    /// Exact distribution by enumerating every permutation of the multiset.
    fn enumerate(store: &ConstraintStore) -> HashMap<Vec<u8>, f64> {
        fn rec(
            pos: usize,
            counts: &mut [u8; 3],
            cur: &mut Vec<u8>,
            store: &ConstraintStore,
            out: &mut HashMap<Vec<u8>, f64>,
        ) {
            if pos == 6 {
                if store.satisfies_conventions(|c| Identity(cur[c.index()])) {
                    *out.entry(cur.clone()).or_default() += 1.0;
                }
                return;
            }
            for i in 0..3 {
                if counts[i] > 0 && store.domains[pos].contains(Identity(i as u8)) {
                    counts[i] -= 1;
                    cur.push(i as u8);
                    rec(pos + 1, counts, cur, store, out);
                    cur.pop();
                    counts[i] += 1;
                }
            }
        }
        let mut out = HashMap::new();
        rec(0, &mut [3, 2, 1], &mut Vec::new(), store, &mut out);
        let total: f64 = out.values().sum();
        out.values_mut().for_each(|v| *v /= total);
        out
    }

    fn empirical(sampler: &ExactSampler, n: usize) -> HashMap<Vec<u8>, f64> {
        let set = sampler.sample(n, 1);
        let mut out: HashMap<Vec<u8>, f64> = HashMap::new();
        let total: f64 = set.weights.iter().sum();
        for (w, weight) in set.worlds.iter().zip(&set.weights) {
            *out.entry(w.as_slice().iter().map(|i| i.0).collect())
                .or_default() += weight / total;
        }
        out
    }

    #[test]
    fn matches_enumeration() {
        let store = tiny_store(&[(0, &[0, 1]), (1, &[1, 2]), (2, &[0, 2])]);
        let sampler = ExactSampler::new(&store, 0.0);
        let exact = enumerate(&store);
        let sampled = empirical(&sampler, 200_000);
        assert_eq!(exact.len(), sampled.len());
        for (deal, p) in &exact {
            let q = sampled.get(deal).copied().unwrap_or(0.0);
            assert!((p - q).abs() < 0.01, "{deal:?}: exact {p}, sampled {q}");
        }
    }

    #[test]
    fn never_samples_impossible_copies() {
        let store = tiny_store(&[(0, &[2]), (3, &[0, 2])]);
        let set = ExactSampler::new(&store, 0.0).sample(2000, 3);
        for world in &set.worlds {
            let mut counts = [0u8; 3];
            for &i in world.as_slice() {
                counts[i.index()] += 1;
            }
            assert_eq!(counts, [3, 2, 1]);
            assert_eq!(world.identity(CardId(0)), Identity(2));
            assert_eq!(
                world.identity(CardId(3)),
                Identity(0),
                "id 2 is used by card 0"
            );
        }
    }

    #[test]
    fn convention_constraints_are_weights() {
        let mut store = tiny_store(&[(0, &[0, 1])]);
        store.convention.push(ConventionConstraint::Restrict {
            card: CardId(0),
            identities: IdentitySet::single(Identity(1)),
        });
        // Trusting completely: card 0 is always identity 1.
        let set = ExactSampler::new(&store, 0.0).sample(500, 9);
        assert!((set.probability(CardId(0), IdentitySet::single(Identity(1))) - 1.0).abs() < 1e-9);
        // With epsilon the violating worlds keep a small weight, matching enumeration.
        let sampler = ExactSampler::new(&store, 0.1);
        let exact_sat: f64 = {
            let physical = enumerate(&tiny_store(&[(0, &[0, 1])]));
            physical
                .iter()
                .filter(|(d, _)| d[0] == 1)
                .map(|(_, p)| p)
                .sum()
        };
        let expected = exact_sat / (exact_sat + 0.1 * (1.0 - exact_sat));
        let p = sampler
            .sample(100_000, 4)
            .probability(CardId(0), IdentitySet::single(Identity(1)));
        assert!((p - expected).abs() < 0.01, "{p} vs {expected}");

        // Located constraints enter as weights.
        let mut store = tiny_store(&[]);
        store.convention.push(ConventionConstraint::Located {
            identity: Identity(2),
            cards: [CardId(4), CardId(5)].into_iter().collect(),
        });
        let set = ExactSampler::new(&store, 0.0).sample(5000, 2);
        assert!((set.probability(CardId(4), IdentitySet::single(Identity(2))) - 0.5).abs() < 0.05);
        assert!(set.effective_size() > 1000.0);
    }
}
