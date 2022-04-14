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
            game::Suit::Yellow(),
            game::Suit::Green(),
            game::Suit::Blue(),
            game::Suit::Purple(),
        ]
    }

    pub fn suit_index(&self, suit: &game::Suit) -> usize {
        let suits = self.suits();
        for (index, _suit) in suits.iter().enumerate() {
            if suits[index] == *suit {
                return index;
            }
        }
        0
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct CardQuantum {
    variant: Variant,
    hard_cards: [u8; 6],
    soft_size: u8,
    soft_cards: [u8; 6],
}

impl std::fmt::Display for CardQuantum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, suit) in self.variant.suits().iter().enumerate() {
            for (bit, char) in ['1', '2', '3', '4', '5'].iter().enumerate() {
                std::fmt::Display::fmt(
                    &if (1 << bit) & self.soft_cards[index] > 0 {
                        char.to_string().color(suit.color()).bold()
                    } else if (1 << bit) & self.hard_cards[index] > 0 {
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
    pub fn new(variant: Variant) -> Self {
        let mut hard_cards = [255; 6];
        let mut soft_cards = [255; 6];
        for i in 0..variant.len() {
            hard_cards[i] = 0b11111;
            soft_cards[i] = 0b11111;
        }
        Self {
            variant,
            hard_cards,
            soft_cards,
            soft_size: variant.len() as u8 * 5,
        }
    }

    /// Unset all possible cards. Use this function if you want to manually
    /// define the set of possible cards with `.add_card`
    pub fn clear(&mut self) {
        for i in 0..self.variant.len() {
            self.hard_cards[i] = 0;
            self.soft_cards[i] = 0;
        }
        self.soft_size = 0;
    }

    pub fn limit_by_suit(&mut self, suit: &game::Suit, effect: bool) {
        let index = self.variant.suit_index(suit);
        if effect {
            self.soft_size = self.soft_cards[index].count_ones() as u8;
        } else {
            self.soft_size -= self.soft_cards[index].count_ones() as u8;
        }
        for i in 0..self.variant.len() {
            if (i != index) == effect {
                self.hard_cards[i] = 0;
                self.soft_cards[i] = 0;
            }
        }
    }

    pub fn limit_by_rank(&mut self, rank: usize, effect: bool) {
        let rank_bit = 1 << (rank - 1);
        let rank_modifier = if effect {
            self.soft_size = 0;
            rank_bit
        } else {
            !rank_bit
        };
        for i in 0..self.variant.len() {
            self.hard_cards[i] &= rank_modifier;
            if self.soft_cards[i] & rank_bit > 0 {
                if effect {
                    self.soft_size += 1;
                } else {
                    self.soft_size -= 1;
                }
            }
            self.soft_cards[i] &= rank_modifier;
        }
    }

    pub fn add_card(&mut self, card: &game::Card, soft: bool) {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        if self.soft_cards[index] & rank_bit == 0 {
            self.soft_size += 1;
        }
        self.soft_cards[index] |= rank_bit;
        if !soft {
            self.hard_cards[index] |= rank_bit;
        }
    }

    pub fn remove_card(&mut self, card: &game::Card, soft: bool) {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        if self.soft_cards[index] & rank_bit > 0 {
            self.soft_size -= 1;
        }
        let rank_mask = !rank_bit;
        self.soft_cards[index] &= rank_mask;
        if !soft {
            self.hard_cards[index] &= rank_mask;
        }
    }

    pub fn contains(&self, card: &game::Card) -> bool {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        self.soft_cards[index] & rank_bit > 0
    }

    pub fn contains_hard(&self, card: &game::Card) -> bool {
        let index = self.variant.suit_index(&card.suit);
        let rank_bit = 1 << (card.rank - 1);
        self.hard_cards[index] & rank_bit > 0
    }

    pub fn is_rank(&self, rank: u8) -> bool {
        let bit_test = !(1 << (rank - 1));
        for suit_index in 0..self.variant.suits().len() {
            if self.soft_cards[suit_index] & bit_test > 0 {
                return false;
            }
        }
        true
    }

    pub fn size(&self) -> u8 {
        self.soft_size
    }

    pub fn hard_size(&self) -> u8 {
        let mut set = 0;
        for suit_index in 0..self.variant.suits().len() {
            set += self.hard_cards[suit_index].count_ones()
        }
        set as u8
    }

    pub fn reset_soft(&mut self) {
        for suit_index in 0..self.variant.suits().len() {
            self.soft_cards[suit_index] = self.hard_cards[suit_index];
        }
        self.update_soft_count();
    }

    fn update_soft_count(&mut self) {
        self.soft_size = 0;
        for suit_index in 0..self.variant.suits().len() {
            self.soft_size += self.soft_cards[suit_index].count_ones() as u8
        }
    }

    pub fn soft_clear(&mut self) {
        for suit_index in 0..self.variant.suits().len() {
            self.soft_cards[suit_index] = 0;
        }
        self.soft_size = 0;
    }

    pub fn soft_limit(&mut self, other: Self) {
        for suit_index in 0..self.variant.suits().len() {
            self.soft_cards[suit_index] &= other.soft_cards[suit_index];
        }
        self.update_soft_count();
    }

    pub fn superset(&self, set: Self) -> bool {
        for suit_index in 0..self.variant.suits().len() {
            if !self.soft_cards[suit_index] & set.soft_cards[suit_index] > 0 {
                return false;
            }
        }
        true
    }

    pub fn interset(&self, set: Self) -> bool {
        for suit_index in 0..self.variant.suits().len() {
            if self.soft_cards[suit_index] & set.soft_cards[suit_index] > 0 {
                return true;
            }
        }
        false
    }

    pub fn to_vec(&self) -> Vec<game::Card> {
        self.iter().collect()
    }
}

impl<'a> CardQuantum {
    pub fn iter(&'a self) -> CardIterator<'a> {
        CardIterator {
            variant: &self.variant,
            cards: &self.soft_cards,
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
        assert_eq!(c.soft_cards[0], 0b11111);
        assert_eq!(c.soft_cards[1], 0b11111);
        assert_eq!(c.soft_cards[2], 0b11111);
        assert_eq!(c.soft_cards[3], 0b11111);
        assert_eq!(c.soft_cards[4], 0b11111);
        assert_eq!(c.hard_cards[0], 0b11111);
        assert_eq!(c.hard_cards[1], 0b11111);
        assert_eq!(c.hard_cards[2], 0b11111);
        assert_eq!(c.hard_cards[3], 0b11111);
        assert_eq!(c.hard_cards[4], 0b11111);
    }

    #[test]
    fn it_clears() {
        let variant = Variant {};
        let mut c = CardQuantum::new(variant);
        assert_eq!(c.soft_cards[0], 0b11111);
        assert_eq!(c.soft_cards[1], 0b11111);
        assert_eq!(c.soft_cards[2], 0b11111);
        assert_eq!(c.soft_cards[3], 0b11111);
        assert_eq!(c.soft_cards[4], 0b11111);
        assert_eq!(c.hard_cards[0], 0b11111);
        assert_eq!(c.hard_cards[1], 0b11111);
        assert_eq!(c.hard_cards[2], 0b11111);
        assert_eq!(c.hard_cards[3], 0b11111);
        assert_eq!(c.hard_cards[4], 0b11111);
        c.clear();
        assert_eq!(c.soft_cards[0], 0);
        assert_eq!(c.soft_cards[1], 0);
        assert_eq!(c.soft_cards[2], 0);
        assert_eq!(c.soft_cards[3], 0);
        assert_eq!(c.soft_cards[4], 0);
        assert_eq!(c.hard_cards[0], 0);
        assert_eq!(c.hard_cards[1], 0);
        assert_eq!(c.hard_cards[2], 0);
        assert_eq!(c.hard_cards[3], 0);
        assert_eq!(c.hard_cards[4], 0);
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
}
