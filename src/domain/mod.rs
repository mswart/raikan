//! Shared vocabulary of every layer: absolute ids and the bitsets over them.
//!
//! Seats are absolute (`0..n`), cards are identified by deck order ([`CardId`], the number
//! hanab.live calls `order`) and identities by a variant-defined dense index ([`Identity`]).
//! Relative seats and hand positions exist only in adapters.

use serde::{Deserialize, Serialize};

/// Maximum number of identities of any catalogue variant (6 suits × 6 ranks).
pub const MAX_IDENTITIES: usize = 36;
/// Maximum number of physical cards of any catalogue variant.
pub const MAX_CARDS: usize = 60;
/// Maximum number of seats at a table.
pub const MAX_SEATS: usize = 6;
/// Maximum hand size (2–3 players with the "one extra card" option).
pub const MAX_HAND: usize = 6;
/// Maximum number of suits.
pub const MAX_SUITS: usize = 6;

/// A seat at the table, absolute (`0..n`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Seat(pub u8);

impl Seat {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The seat `offset` places after this one.
    pub fn after(self, offset: u8, num_seats: u8) -> Seat {
        Seat((self.0 + offset) % num_seats)
    }

    /// How many places `other` sits after this seat (0 for the same seat).
    pub fn distance_to(self, other: Seat, num_seats: u8) -> u8 {
        (num_seats + other.0 - self.0) % num_seats
    }
}

/// A physical card, identified by its deck order (hanab.live `order`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardId(pub u8);

impl CardId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A card identity (suit × rank), a dense index defined by the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Identity(pub u8);

impl Identity {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A clue that exists in a variant, a dense index defined by the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClueId(pub u8);

impl ClueId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

macro_rules! bitset {
    ($(#[$meta:meta])* $name:ident, $item:ident, $max:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl $name {
            pub const EMPTY: Self = Self(0);

            /// All items with an index below `n`.
            pub fn first(n: usize) -> Self {
                debug_assert!(n <= $max);
                if n >= 64 { Self(u64::MAX) } else { Self((1u64 << n) - 1) }
            }

            pub fn single(item: $item) -> Self {
                Self(1u64 << item.0)
            }

            pub fn contains(self, item: $item) -> bool {
                self.0 & (1u64 << item.0) != 0
            }

            pub fn insert(&mut self, item: $item) {
                self.0 |= 1u64 << item.0;
            }

            pub fn remove(&mut self, item: $item) {
                self.0 &= !(1u64 << item.0);
            }

            pub fn len(self) -> usize {
                self.0.count_ones() as usize
            }

            pub fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// The only item, if the set has exactly one.
            pub fn single_item(self) -> Option<$item> {
                if self.0.count_ones() == 1 {
                    Some($item(self.0.trailing_zeros() as u8))
                } else {
                    None
                }
            }

            /// The item with the lowest index.
            pub fn min(self) -> Option<$item> {
                if self.0 == 0 { None } else { Some($item(self.0.trailing_zeros() as u8)) }
            }

            /// The item with the highest index.
            pub fn max(self) -> Option<$item> {
                if self.0 == 0 { None } else { Some($item(63 - self.0.leading_zeros() as u8)) }
            }

            pub fn is_subset(self, other: Self) -> bool {
                self.0 & !other.0 == 0
            }

            pub fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }

            pub fn iter(self) -> impl DoubleEndedIterator<Item = $item> + Clone {
                let bits = self.0;
                (0..64u8).filter(move |i| bits & (1u64 << i) != 0).map($item)
            }
        }

        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }
        impl std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0 }
        }
        impl std::ops::BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self { Self(self.0 & rhs.0) }
        }
        impl std::ops::BitAndAssign for $name {
            fn bitand_assign(&mut self, rhs: Self) { self.0 &= rhs.0 }
        }
        impl std::ops::Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self { Self(self.0 & !rhs.0) }
        }
        impl std::ops::SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) { self.0 &= !rhs.0 }
        }
        impl FromIterator<$item> for $name {
            fn from_iter<T: IntoIterator<Item = $item>>(iter: T) -> Self {
                let mut set = Self::EMPTY;
                for item in iter {
                    set.insert(item);
                }
                set
            }
        }
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_set().entries(self.iter().map(|i| i.0)).finish()
            }
        }
    };
}

bitset!(
    /// A set of identities. Every catalogue variant has at most 36, so it fits a `u64`.
    IdentitySet, Identity, MAX_IDENTITIES
);
bitset!(
    /// A set of physical cards. Every catalogue variant has at most 60, so it fits a `u64`.
    CardSet, CardId, MAX_CARDS
);
bitset!(
    /// A set of seats.
    SeatSet, Seat, MAX_SEATS
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitset_basics() {
        let mut set = IdentitySet::EMPTY;
        assert!(set.is_empty());
        set.insert(Identity(3));
        set.insert(Identity(35));
        assert_eq!(set.len(), 2);
        assert!(set.contains(Identity(35)));
        assert_eq!(set.min(), Some(Identity(3)));
        assert_eq!(set.max(), Some(Identity(35)));
        assert_eq!(set.single_item(), None);
        set.remove(Identity(3));
        assert_eq!(set.single_item(), Some(Identity(35)));
        assert_eq!(IdentitySet::first(25).len(), 25);
        assert_eq!(CardSet::first(60).len(), 60);
        let cards: CardSet = [CardId(1), CardId(59)].into_iter().collect();
        assert_eq!(
            cards.iter().collect::<Vec<_>>(),
            vec![CardId(1), CardId(59)]
        );
        assert_eq!(cards.iter().rev().next(), Some(CardId(59)));
    }

    #[test]
    fn seats() {
        assert_eq!(Seat(3).after(2, 4), Seat(1));
        assert_eq!(Seat(3).distance_to(Seat(1), 4), 2);
        assert_eq!(Seat(1).distance_to(Seat(1), 4), 0);
    }
}
