use crate::game;
use colored::*;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Variant {}

impl Variant {
    pub fn len(&self) -> usize {
        5
    }

    pub fn suits(&self) -> [game::Suit; 5] {
        [
            game::Suit::Red(),
            game::Suit::Green(),
            game::Suit::Yellow(),
            game::Suit::Blue(),
            game::Suit::Purple(),
        ]
    }

    pub fn suit_index(&self, suit: &game::Suit) -> usize {
        let suits = self.suits();
        for index in 0..suits.len() {
            if suits[index] == *suit {
                return index;
            }
        }
        return 0;
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct CardQuantum {
    variant: Variant,
    cards: [u8; 6],
}

impl std::fmt::Display for CardQuantum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, suit) in self.variant.suits().iter().enumerate() {
            for (bit, char) in ['1', '2', '3', '4', '5'].iter().enumerate() {
                std::fmt::Display::fmt(
                    &if (1 << bit) & self.cards[index] > 0 {
                        char.to_string().color(suit.color()).bold()
                    } else {
                        char.to_string()
                            .color(colored::Color::BrightBlack)
                            .strikethrough()
                    },
                    f,
                )?;
            }
        }
        Ok(())
    }
}

impl CardQuantum {
    pub fn new(variant: Variant) -> Self {
        let mut cards = [255; 6];
        for i in 0..variant.len() {
            cards[i] = 0b11111;
        }
        Self {
            variant: variant,
            cards,
        }
    }

    /// Unset all possible cards. Use this function if you want to manually
    /// define the set of possible cards with `.add_card`
    pub fn clear(&mut self) {
        for i in 0..self.variant.len() {
            self.cards[i] = 0;
        }
    }

    pub fn limit_by_suit(&mut self, suit: &game::Suit, effect: bool) {
        let index = self.variant.suit_index(suit);
        for i in 0..self.variant.len() {
            if (i != index) == effect {
                self.cards[i] = 0;
            }
        }
    }

    pub fn limit_by_rank(&mut self, rank: usize, effect: bool) {
        let mut rank_modifier = 1 << (rank - 1);
        if !effect {
            rank_modifier = !rank_modifier;
        }
        for i in 0..self.variant.len() {
            self.cards[i] &= rank_modifier
        }
    }

    pub fn add_card(&mut self, card: &game::Card) {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        self.cards[index] |= rank_bit;
    }

    pub fn remove_card(&mut self, card: &game::Card) {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = !(1 << (card.rank - 1));
        self.cards[index] &= rank_bit;
    }

    pub fn contains(&self, card: &game::Card) -> bool {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        self.cards[index] & rank_bit > 0
    }

    pub fn is_rank(&self, rank: u8) -> bool {
        let bit_test = !(1 << (rank - 1));
        for suit_index in 0..self.variant.suits().len() {
            if self.cards[suit_index] & bit_test > 0 {
                return false;
            }
        }
        true
    }

    pub fn size(&self) -> u8 {
        let mut set = 0;
        for suit_index in 0..self.variant.suits().len() {
            set += self.cards[suit_index].count_ones()
        }
        set as u8
    }
}

impl<'a> CardQuantum {
    pub fn iter(&'a self) -> CardIterator<'a> {
        CardIterator {
            variant: &self.variant,
            cards: &self.cards,
            current_card: game::Card {
                suit: self.variant.suits()[0],
                rank: 0,
            },
        }
    }
}

pub struct CardIterator<'a> {
    variant: &'a Variant,
    cards: &'a [u8; 6],
    current_card: game::Card,
}
impl<'a> Iterator for CardIterator<'a> {
    type Item = game::Card;

    fn next(&mut self) -> Option<Self::Item> {
        let mut current_index = self.variant.suit_index(&self.current_card.suit);

        let remaining_bits = self.cards[current_index] >> self.current_card.rank;
        if remaining_bits != 0 {
            // card in same suit in quantum
            self.current_card.rank += remaining_bits.trailing_zeros() as u8 + 1;
            return Some(self.current_card);
        }
        let suits = self.variant.suits();
        // switch to next suits
        while current_index + 1 < suits.len() {
            current_index += 1;
            if self.cards[current_index] == 0 {
                continue;
            }
            self.current_card = game::Card {
                suit: self.variant.suits()[current_index],
                rank: self.cards[current_index].trailing_zeros() as u8 + 1,
            };
            return Some(self.current_card);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_initials_with_everything() {
        let variant = Variant {};
        let c = CardQuantum::new(variant);
        assert_eq!(c.cards[0], 0b11111);
        assert_eq!(c.cards[1], 0b11111);
        assert_eq!(c.cards[2], 0b11111);
        assert_eq!(c.cards[3], 0b11111);
        assert_eq!(c.cards[4], 0b11111);
    }

    #[test]
    fn it_clears() {
        let variant = Variant {};
        let mut c = CardQuantum::new(variant);
        assert_eq!(c.cards[0], 0b11111);
        assert_eq!(c.cards[1], 0b11111);
        assert_eq!(c.cards[2], 0b11111);
        assert_eq!(c.cards[3], 0b11111);
        assert_eq!(c.cards[4], 0b11111);
        c.clear();
        assert_eq!(c.cards[0], 0);
        assert_eq!(c.cards[1], 0);
        assert_eq!(c.cards[2], 0);
        assert_eq!(c.cards[3], 0);
        assert_eq!(c.cards[4], 0);
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
        c.add_card(&game::Card {
            rank: 1,
            suit: variant.suits()[0],
        });
        assert!(c.contains(&card1));
    }

    #[test]
    fn size() {
        let variant = Variant {};
        let mut c = CardQuantum::new(variant);
        assert_eq!(c.size(), 25);
        c.clear();
        assert_eq!(c.size(), 0);
        c.add_card(&game::Card {
            rank: 1,
            suit: variant.suits()[0],
        });
        assert_eq!(c.size(), 1);
    }
}
