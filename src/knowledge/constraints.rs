//! Constraint store with two strictly separate classes: physical constraints (what the
//! cards can be) and convention constraints (what the conventions say they are).

use crate::domain::{CardId, CardSet, Identity, IdentitySet, MAX_CARDS, MAX_IDENTITIES};
use crate::engine::GameState;
use crate::variants::Variant;

use super::{CardCount, KnownCards, PublicCards, Sight, possible_identities};

/// A constraint added by a convention. It can be wrong while the world stays valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConventionConstraint {
    /// The card is one of these identities.
    Restrict {
        card: CardId,
        identities: IdentitySet,
    },
    /// Exactly one of these cards is `identity` (e.g. "one of these is r3").
    Located { identity: Identity, cards: CardSet },
}

impl ConventionConstraint {
    pub fn satisfied_by(&self, identity_of: impl Fn(CardId) -> Identity) -> bool {
        match *self {
            ConventionConstraint::Restrict { card, identities } => {
                identities.contains(identity_of(card))
            }
            ConventionConstraint::Located { identity, cards } => {
                cards.iter().filter(|&c| identity_of(c) == identity).count() == 1
            }
        }
    }
}

/// Everything a viewpoint knows about the unseen cards.
#[derive(Clone, Debug)]
pub struct ConstraintStore {
    /// Cards whose identity the viewpoint does not know (own hand and the rest of the deck).
    pub unknown: CardSet,
    /// Physical domain of each unknown card (clues plus card counting).
    pub domains: [IdentitySet; MAX_CARDS],
    /// Copies of each identity among the unknown cards.
    pub remaining: [u8; MAX_IDENTITIES],
    /// Identities of the cards the viewpoint knows.
    pub known: KnownCards,
    pub convention: Vec<ConventionConstraint>,
    /// Size of the whole deck.
    pub num_cards: u8,
}

impl ConstraintStore {
    /// Physical constraints of a viewpoint with the given sight.
    pub fn new(
        variant: &Variant,
        state: &GameState,
        public: &PublicCards,
        sight: Sight,
        known: &KnownCards,
    ) -> Self {
        let dealt = state.dealt();
        let count = CardCount::new(sight, known, public, dealt);
        let all_cards = CardSet::first(variant.num_cards() as usize);
        let mut unknown = CardSet::EMPTY;
        let mut domains = [IdentitySet::EMPTY; MAX_CARDS];
        let mut known_cards = KnownCards::default();
        // Copies not accounted for by cards whose identity this viewpoint knows.
        let mut remaining = [0u8; MAX_IDENTITIES];
        for identity in variant.all_identities().iter() {
            remaining[identity.index()] = variant.copies(identity);
        }
        for card in all_cards.iter() {
            let identity = if !dealt.contains(card) {
                None
            } else if sight.contains(card) {
                known.get(card)
            } else {
                public.literal(card).single_item()
            };
            match identity {
                Some(identity) => {
                    known_cards.set(card, identity);
                    remaining[identity.index()] = remaining[identity.index()].saturating_sub(1);
                }
                None => {
                    unknown.insert(card);
                    domains[card.index()] = if dealt.contains(card) {
                        possible_identities(variant, public, &count, sight, known, card)
                    } else {
                        variant.all_identities()
                    };
                }
            }
        }
        let mut store = Self {
            unknown,
            domains,
            remaining,
            known: known_cards,
            convention: Vec::new(),
            num_cards: variant.num_cards(),
        };
        store.restrict_to_remaining();
        store
    }

    fn restrict_to_remaining(&mut self) {
        let available: IdentitySet = (0..MAX_IDENTITIES as u8)
            .map(Identity)
            .filter(|i| self.remaining[i.index()] > 0)
            .collect();
        for card in self.unknown.iter() {
            self.domains[card.index()] &= available;
        }
    }

    /// Identities that still have an unseen copy.
    pub fn available(&self) -> IdentitySet {
        (0..MAX_IDENTITIES as u8)
            .map(Identity)
            .filter(|i| self.remaining[i.index()] > 0)
            .collect()
    }

    /// Cards whose domain is narrower than "any remaining identity"; only these need care
    /// when sampling.
    pub fn constrained(&self) -> CardSet {
        let available = self.available();
        self.unknown
            .iter()
            .filter(|c| self.domains[c.index()] != available)
            .collect()
    }

    /// Domains narrowed by the `Restrict` convention constraints (trusting the conventions).
    pub fn trusting_domains(&self) -> [IdentitySet; MAX_CARDS] {
        let mut domains = self.domains;
        for constraint in &self.convention {
            if let ConventionConstraint::Restrict { card, identities } = *constraint
                && self.unknown.contains(card)
            {
                domains[card.index()] &= identities;
            }
        }
        domains
    }

    /// The identity of a card under a complete assignment of the unknown cards.
    pub fn satisfies_conventions(&self, identity_of: impl Fn(CardId) -> Identity + Copy) -> bool {
        self.convention.iter().all(|c| c.satisfied_by(identity_of))
    }
}
