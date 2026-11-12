use crate::game;
use colored::*;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Variant {}

#[allow(clippy::len_without_is_empty)]
impl Variant {
    pub fn len(&self) -> usize {
        5
    }

    pub fn suits(&self) -> [game::Suit; 5] {
        [
            game::Suit::Red(),
            game::Suit::Yellow(),
            game::Suit::Green(),
            game::Suit::Blue(),
            game::Suit::Purple(),
        ]
    }

    pub fn suit_index(&self, suit: &game::Suit) -> usize {
        match suit {
            game::Suit::Red() => 0,
            game::Suit::Yellow() => 1,
            game::Suit::Green() => 2,
            game::Suit::Blue() => 3,
            game::Suit::Purple() => 4,
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct CardQuantum {
    variant: Variant,
    hard_cards: u32,
    soft_cards: u32,
}

impl std::fmt::Display for CardQuantum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, suit) in self.variant.suits().iter().enumerate() {
            for (bit, char) in ['1', '2', '3', '4', '5'].iter().enumerate() {
                std::fmt::Display::fmt(
                    &if (1 << (index * 5 + bit)) & self.soft_cards > 0 {
                        char.to_string().color(suit.color()).bold()
                    } else if (1 << (index * 5 + bit)) & self.hard_cards > 0 {
                        char.to_string()
                            .color(colored::Color::BrightBlack)
                            .strikethrough()
                    } else {
                        " ".to_string().strikethrough()
                        // char.to_string()
                        //     .color(colored::Color::BrightBlack)
                        //     .strikethrough()
                    },
                    f,
                )?;
            }
        }
        Ok(())
    }
}

impl CardQuantum {
    const ALL: u32 = (1 << 25) - 1;
    const ONES: u32 = 1 | (1 << 5) | (1 << 10) | (1 << 15) | (1 << 20);

    pub fn new(variant: Variant) -> Self {
        Self {
            variant,
            hard_cards: Self::ALL,
            soft_cards: Self::ALL,
        }
    }

    /// Remove all identities, including literal possibilities.
    pub fn clear(&mut self) {
        self.hard_cards = 0;
        self.soft_cards = 0;
    }

    fn limit(&mut self, mask: u32, effect: bool) {
        let allowed = if effect { mask } else { !mask };
        self.hard_cards &= allowed;
        self.soft_cards &= allowed;
    }

    pub fn limit_by_suit(&mut self, suit: &game::Suit, effect: bool) {
        self.limit(0b11111 << (self.variant.suit_index(suit) * 5), effect);
    }

    pub fn limit_by_rank(&mut self, rank: usize, effect: bool) {
        self.limit(Self::ONES << (rank - 1), effect);
    }

    fn bit(&self, card: &game::Card) -> u32 {
        1 << (self.variant.suit_index(&card.suit) * 5 + card.rank as usize - 1)
    }

    pub fn add_card(&mut self, card: &game::Card, soft: bool) {
        let bit = self.bit(card);
        self.soft_cards |= bit;
        if !soft {
            self.hard_cards |= bit;
        }
    }

    pub fn remove_card(&mut self, card: &game::Card, soft: bool) {
        let bit = self.bit(card);
        self.soft_cards &= !bit;
        if !soft {
            self.hard_cards &= !bit;
        }
    }

    pub fn contains(&self, card: &game::Card) -> bool {
        self.soft_cards & self.bit(card) != 0
    }

    pub fn contains_hard(&self, card: &game::Card) -> bool {
        self.hard_cards & self.bit(card) != 0
    }

    pub fn is_rank(&self, rank: u8) -> bool {
        self.soft_cards & !(Self::ONES << (rank - 1)) == 0
    }

    pub fn size(&self) -> u8 {
        self.soft_cards.count_ones() as u8
    }
    pub fn hard_size(&self) -> u8 {
        self.hard_cards.count_ones() as u8
    }
    pub fn reset_soft(&mut self) {
        self.soft_cards = self.hard_cards;
    }
    pub fn soft_clear(&mut self) {
        self.soft_cards = 0;
    }
    pub fn soft_limit(&mut self, other: Self) {
        self.soft_cards &= other.soft_cards;
    }

    /// Exclude inferred identities while retaining literal clue evidence.
    pub(crate) fn soft_exclude(&mut self, other: Self) {
        self.soft_cards &= !other.soft_cards;
    }

    pub fn superset(&self, set: Self) -> bool {
        set.soft_cards & !self.soft_cards == 0
    }
    pub fn interset(&self, set: Self) -> bool {
        self.soft_cards & set.soft_cards != 0
    }
    pub fn to_vec(&self) -> Vec<game::Card> {
        self.iter().collect()
    }

    pub fn iter(&self) -> CardIterator<'_> {
        CardIterator {
            variant: &self.variant,
            cards: self.soft_cards,
        }
    }
}

#[derive(Clone)]
pub struct CardIterator<'a> {
    variant: &'a Variant,
    cards: u32,
}

impl Iterator for CardIterator<'_> {
    type Item = game::Card;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cards == 0 {
            return None;
        }
        let index = self.cards.trailing_zeros();
        self.cards &= self.cards - 1;
        Some(game::Card {
            suit: self.variant.suits()[index as usize / 5],
            rank: (index % 5) as u8 + 1,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.cards.count_ones() as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for CardIterator<'_> {}
impl std::iter::FusedIterator for CardIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_initials_with_everything() {
        let variant = Variant {};
        let c = CardQuantum::new(variant);
        assert_eq!(c.size(), 25);
        assert_eq!(c.hard_size(), 25);
        for suit in variant.suits() {
            for rank in 1..=5 {
                assert!(c.contains(&game::Card { suit, rank }));
                assert!(c.contains_hard(&game::Card { suit, rank }));
            }
        }
    }

    #[test]
    fn it_clears() {
        let mut c = CardQuantum::new(Variant {});
        c.clear();
        assert_eq!(c.size(), 0);
        assert_eq!(c.hard_size(), 0);
        assert_eq!(c.iter().next(), None);
        c.reset_soft();
        assert_eq!(c.size(), 0);
    }

    #[test]
    fn contains() {
        let variant = Variant {};
        let mut c = CardQuantum::new(variant);
        let card1 = game::Card {
            rank: 1,
            suit: variant.suits()[0],
        };
        assert!(c.contains(&card1));
        c.clear();
        assert!(!c.contains(&card1));
        c.add_card(
            &game::Card {
                rank: 1,
                suit: variant.suits()[0],
            },
            true,
        );
        assert!(c.contains(&card1));
    }

    #[test]
    fn size() {
        let variant = Variant {};
        let mut c = CardQuantum::new(variant);
        assert_eq!(c.size(), 25);
        c.clear();
        assert_eq!(c.size(), 0);
        c.add_card(
            &game::Card {
                rank: 1,
                suit: variant.suits()[0],
            },
            true,
        );
        assert_eq!(c.size(), 1);
    }

    #[test]
    fn literal_clues_survive_soft_exclusion_and_reset() {
        for suit in (Variant {}).suits() {
            for rank in 1..=5 {
                let expected = game::Card { suit, rank };
                let mut quantum = CardQuantum::new(Variant {});
                quantum.limit_by_suit(&suit, true);
                quantum.limit_by_rank(rank as usize, false);
                assert!(!quantum.contains_hard(&expected));
                assert_eq!(quantum.size(), 4);
                let literal = quantum.to_vec();
                let mut trash = CardQuantum::new(Variant {});
                trash.limit_by_rank((rank % 5 + 1) as usize, true);
                quantum.soft_exclude(trash);
                assert_eq!(quantum.size(), 3);
                assert_eq!(quantum.hard_size(), 4);
                quantum.reset_soft();
                assert_eq!(quantum.to_vec(), literal);
                quantum.limit_by_rank(rank as usize, true);
                assert_eq!(quantum.size(), 0);
                assert_eq!(quantum.hard_size(), 0);
            }
        }
    }

    #[test]
    fn iteration_preserves_suit_rank_order_and_remaining_length() {
        let quantum = CardQuantum::new(Variant {});
        let mut iter = quantum.iter();
        let mut remaining = 25;
        for suit in (Variant {}).suits() {
            for rank in 1..=5 {
                assert_eq!(iter.len(), remaining);
                assert_eq!(iter.next(), Some(game::Card { suit, rank }));
                remaining -= 1;
            }
        }
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }
}
