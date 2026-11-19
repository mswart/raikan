use crate::domain::{Identity, IdentitySet};

/// What a viewpoint believes a card can be.
///
/// `hard` is literal information (clues and card counting) and survives every correction of
/// a convention mistake. `soft` adds convention inferences. Normally `soft ⊆ hard`, but a
/// convention may assert an identity the literal information already excludes (a mistake by
/// the clue giver); such a contradiction is visible as a soft identity outside `hard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Belief {
    pub hard: IdentitySet,
    pub soft: IdentitySet,
}

impl Belief {
    pub fn new(all: IdentitySet) -> Self {
        Self {
            hard: all,
            soft: all,
        }
    }

    pub fn single(identity: Identity) -> Self {
        Self {
            hard: IdentitySet::single(identity),
            soft: IdentitySet::single(identity),
        }
    }

    pub fn size(&self) -> usize {
        self.soft.len()
    }

    pub fn hard_size(&self) -> usize {
        self.hard.len()
    }

    pub fn contains(&self, identity: Identity) -> bool {
        self.soft.contains(identity)
    }

    pub fn contains_hard(&self, identity: Identity) -> bool {
        self.hard.contains(identity)
    }

    /// Apply a clue: keep `touch` if touched, otherwise remove it (both parts).
    pub fn apply_clue(&mut self, touch: IdentitySet, touched: bool) {
        if touched {
            self.hard &= touch;
            self.soft &= touch;
        } else {
            self.hard -= touch;
            self.soft -= touch;
        }
    }

    /// Remove an identity from the inference (`soft`) or from both parts.
    pub fn remove(&mut self, identity: Identity, soft_only: bool) {
        self.soft.remove(identity);
        if !soft_only {
            self.hard.remove(identity);
        }
    }

    pub fn add_soft(&mut self, identity: Identity) {
        self.soft.insert(identity);
    }

    pub fn reset_soft(&mut self) {
        self.soft = self.hard;
    }

    pub fn soft_clear(&mut self) {
        self.soft = IdentitySet::EMPTY;
    }

    pub fn soft_limit(&mut self, set: IdentitySet) {
        self.soft &= set;
    }

    pub fn soft_exclude(&mut self, set: IdentitySet) {
        self.soft -= set;
    }

    /// The single inferred identity, if there is exactly one.
    pub fn identity(&self) -> Option<Identity> {
        self.soft.single_item()
    }

    pub fn is_subset_of(&self, set: IdentitySet) -> bool {
        self.soft.is_subset(set)
    }

    pub fn intersects(&self, set: IdentitySet) -> bool {
        self.soft.intersects(set)
    }
}
