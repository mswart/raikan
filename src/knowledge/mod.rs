//! Convention-free reasoning: sight, literal information, constraints and world sampling.
//!
//! Nothing here knows about conventions. Convention constraints are stored and weighed, but
//! never produced, by this layer.

mod constraints;
mod sampler;

pub use constraints::{ConstraintStore, ConventionConstraint};
pub use sampler::{ExactSampler, MAX_EXACT_CONSTRAINED, SampleSet, WorldSampler};

use crate::domain::{CardId, CardSet, Identity, IdentitySet, MAX_CARDS, MAX_IDENTITIES, Seat};
use crate::engine::{Event, GameState};
use crate::variants::Variant;

/// Where a card currently is (public).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Location {
    Deck,
    Hand(Seat),
    Played,
    Discarded,
}

/// Public knowledge about every card: location, literal clue information and revealed
/// identities. It reads nothing hidden, so it is the same for every viewpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PublicCards {
    location: [Location; MAX_CARDS],
    /// Identities allowed by positive and negative clue touches.
    literal: [IdentitySet; MAX_CARDS],
    /// Cards whose identity is public (played or discarded).
    pub revealed: CardSet,
    /// Cards positively touched by at least one clue.
    pub clued: CardSet,
    /// Identities allowed by positive clues alone (negative information ignored).
    positive: [IdentitySet; MAX_CARDS],
    /// The clues (bit per `ClueId`) that positively touched each card.
    touched_by: [u64; MAX_CARDS],
}

impl PublicCards {
    pub fn new(variant: &Variant) -> Self {
        Self {
            location: [Location::Deck; MAX_CARDS],
            literal: [variant.all_identities(); MAX_CARDS],
            revealed: CardSet::EMPTY,
            clued: CardSet::EMPTY,
            positive: [variant.all_identities(); MAX_CARDS],
            touched_by: [0; MAX_CARDS],
        }
    }

    /// Apply the public part of an event. `state` is the public state *before* the event
    /// (needed to know the untouched cards of a clue).
    pub fn apply(&mut self, variant: &Variant, before: &GameState, event: &Event) {
        match *event {
            Event::Draw { seat, card, .. } => self.location[card.index()] = Location::Hand(seat),
            Event::Play {
                card,
                identity,
                success,
                ..
            } => {
                self.location[card.index()] = if success {
                    Location::Played
                } else {
                    Location::Discarded
                };
                if let Some(identity) = identity {
                    self.literal[card.index()] = IdentitySet::single(identity);
                    self.revealed.insert(card);
                }
            }
            Event::Discard { card, identity, .. } => {
                self.location[card.index()] = Location::Discarded;
                if let Some(identity) = identity {
                    self.literal[card.index()] = IdentitySet::single(identity);
                    self.revealed.insert(card);
                }
            }
            Event::Clue {
                target,
                clue,
                touched,
                ..
            } => {
                let touch = variant.touch(clue);
                for &card in before.hand(target) {
                    if touched.contains(card) {
                        self.literal[card.index()] &= touch;
                        self.positive[card.index()] &= touch;
                        self.touched_by[card.index()] |= 1 << clue.0;
                    } else {
                        self.literal[card.index()] -= touch;
                    }
                }
                self.clued |= touched;
            }
            Event::GameOver { .. } => {}
        }
    }

    pub fn location(&self, card: CardId) -> Location {
        self.location[card.index()]
    }

    /// Identities allowed by positive clues alone.
    pub fn positive(&self, card: CardId) -> IdentitySet {
        self.positive[card.index()]
    }

    /// The clues that positively touched `card`, one bit per `ClueId`.
    pub fn touched_by(&self, card: CardId) -> u64 {
        self.touched_by[card.index()]
    }

    /// Identities allowed by clues alone.
    pub fn literal(&self, card: CardId) -> IdentitySet {
        self.literal[card.index()]
    }
}

/// A viewpoint's sight: the cards whose identity it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sight(pub CardSet);

impl Sight {
    /// What every seat knows: only revealed cards.
    pub fn common(public: &PublicCards) -> Sight {
        Sight(public.revealed)
    }

    /// Everything dealt except the hands of the given seats. One seat is that seat's own
    /// sight; two seats T and U is what T can assume U knows.
    pub fn excluding_hands(state: &GameState, seats: &[Seat]) -> Sight {
        let mut cards = state.dealt();
        for &seat in seats {
            cards -= state.hand_set(seat);
        }
        Sight(cards)
    }

    pub fn of_seat(state: &GameState, seat: Seat) -> Sight {
        Self::excluding_hands(state, &[seat])
    }

    pub fn contains(&self, card: CardId) -> bool {
        self.0.contains(card)
    }
}

/// Identities of the cards a concrete observer has seen (its own sight, or a sampled world).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct KnownCards {
    identities: [Option<Identity>; MAX_CARDS],
}

impl Default for KnownCards {
    fn default() -> Self {
        Self {
            identities: [None; MAX_CARDS],
        }
    }
}

impl std::fmt::Debug for KnownCards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(
                self.identities
                    .iter()
                    .enumerate()
                    .filter_map(|(c, i)| i.map(|i| (c, i.0))),
            )
            .finish()
    }
}

impl KnownCards {
    pub fn get(&self, card: CardId) -> Option<Identity> {
        self.identities[card.index()]
    }

    pub fn set(&mut self, card: CardId, identity: Identity) {
        self.identities[card.index()] = Some(identity);
    }

    /// Record the identities an event reveals to its observer.
    pub fn observe(&mut self, event: &Event) {
        match *event {
            Event::Draw {
                card,
                identity: Some(identity),
                ..
            }
            | Event::Play {
                card,
                identity: Some(identity),
                ..
            }
            | Event::Discard {
                card,
                identity: Some(identity),
                ..
            } => self.set(card, identity),
            _ => {}
        }
    }

    pub fn from_world(world: &crate::engine::World) -> Self {
        let mut known = Self::default();
        for (c, &identity) in world.as_slice().iter().enumerate() {
            known.set(CardId(c as u8), identity);
        }
        known
    }
}

/// Card counting for one sight: how many copies of each identity the viewpoint can see.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CardCount {
    visible: [u8; MAX_IDENTITIES],
}

impl CardCount {
    /// Count the copies visible in `sight` (identities taken from `known`), plus cards whose
    /// identity is publicly determined by clues.
    pub fn new(sight: Sight, known: &KnownCards, public: &PublicCards, dealt: CardSet) -> Self {
        let mut visible = [0u8; MAX_IDENTITIES];
        for card in dealt.iter() {
            let identity = if sight.contains(card) {
                known.get(card)
            } else {
                public.literal(card).single_item()
            };
            if let Some(identity) = identity {
                visible[identity.index()] += 1;
            }
        }
        Self { visible }
    }

    pub fn visible_copies(&self, identity: Identity) -> u8 {
        self.visible[identity.index()]
    }

    /// Identities of which every copy is visible.
    pub fn exhausted(&self, variant: &Variant) -> IdentitySet {
        variant
            .all_identities()
            .iter()
            .filter(|&i| self.visible[i.index()] >= variant.copies(i))
            .collect()
    }

    /// Copies of each identity that are not visible.
    pub fn remaining(&self, variant: &Variant) -> [u8; MAX_IDENTITIES] {
        let mut remaining = [0u8; MAX_IDENTITIES];
        for identity in variant.all_identities().iter() {
            remaining[identity.index()] = variant
                .copies(identity)
                .saturating_sub(self.visible[identity.index()]);
        }
        remaining
    }
}

/// Literal information of a card in a sight: clue information narrowed by card counting.
/// A card that is itself visible is not counted against its own identity.
pub fn possible_identities(
    variant: &Variant,
    public: &PublicCards,
    count: &CardCount,
    sight: Sight,
    known: &KnownCards,
    card: CardId,
) -> IdentitySet {
    if sight.contains(card)
        && let Some(identity) = known.get(card)
    {
        return IdentitySet::single(identity);
    }
    let literal = public.literal(card);
    if literal.len() == 1 {
        return literal;
    }
    literal - count.exhausted(variant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Action, deal, shuffled_world, step, touched_cards};

    #[test]
    fn literal_and_counting() {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 5);
        let (mut state, events) = deal(&variant, 3, &world);
        let mut public = PublicCards::new(&variant);
        let mut known = KnownCards::default();
        let blank = GameState::new(&variant, 3);
        for event in &events {
            public.apply(&variant, &blank, event);
            known.observe(&event.observed_by(Seat(0)));
        }
        // Seat 0 clues seat 1 with the first clue that touches something.
        let clue = variant
            .clue_ids()
            .find(|&c| !touched_cards(&variant, &state, &world, Seat(1), c).is_empty())
            .unwrap();
        let before = state;
        for event in step(
            &variant,
            &mut state,
            &world,
            Action::Clue {
                target: Seat(1),
                clue,
            },
        ) {
            public.apply(&variant, &before, &event);
        }
        for &card in state.hand(Seat(1)) {
            let literal = public.literal(card);
            assert!(literal.contains(world.identity(card)));
            assert_eq!(
                public.clued.contains(card),
                literal.is_subset(variant.touch(clue))
            );
        }

        // Seat 0's sight: everything outside its own hand.
        let sight = Sight::of_seat(&state, Seat(0));
        assert!(!sight.0.intersects(state.hand_set(Seat(0))));
        let count = CardCount::new(sight, &known, &public, state.dealt());
        let total: u32 = variant
            .all_identities()
            .iter()
            .map(|i| count.visible_copies(i) as u32)
            .sum();
        assert!(total >= 10, "sees both other hands");
        for &card in state.hand(Seat(0)) {
            let possible = possible_identities(&variant, &public, &count, sight, &known, card);
            assert!(possible.contains(world.identity(card)));
        }
        assert_eq!(Sight::common(&public).0, CardSet::EMPTY);
        let pair = Sight::excluding_hands(&state, &[Seat(1), Seat(2)]);
        assert_eq!(pair.0, state.hand_set(Seat(0)));
    }

    #[test]
    fn sampled_worlds_respect_sight() {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 11);
        let (state, events) = deal(&variant, 4, &world);
        let mut public = PublicCards::new(&variant);
        let mut known = KnownCards::default();
        let blank = GameState::new(&variant, 4);
        for event in &events {
            public.apply(&variant, &blank, event);
            known.observe(&event.observed_by(Seat(2)));
        }
        let sight = Sight::of_seat(&state, Seat(2));
        let store = ConstraintStore::new(&variant, &state, &public, sight, &known);
        assert_eq!(store.unknown.len(), 50 - 12);
        let samples = ExactSampler::new(&store, 0.0).sample(50, 1);
        for sample in &samples.worlds {
            assert_eq!(sample.len(), 50);
            let mut counts = [0u8; crate::domain::MAX_IDENTITIES];
            for (c, &identity) in sample.as_slice().iter().enumerate() {
                counts[identity.index()] += 1;
                if sight.contains(CardId(c as u8)) {
                    assert_eq!(identity, world.identity(CardId(c as u8)));
                }
            }
            for identity in variant.all_identities().iter() {
                assert_eq!(counts[identity.index()], variant.copies(identity));
            }
        }
    }
}
