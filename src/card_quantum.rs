use crate::game;
use colored::*;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Variant {}

impl Variant {
    pub fn len(&self) -> usize {
        6
    }

    pub fn suites(&self) -> [game::Suite; 5] {
        [
            game::Suite::Red(),
            game::Suite::Green(),
            game::Suite::Yellow(),
            game::Suite::Blue(),
            game::Suite::Purple(),
        ]
    }

    pub fn suite_index(&self, suite: &game::Suite) -> usize {
        let suites = self.suites();
        for index in 0..suites.len() {
            if suites[index] == *suite {
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
        for (index, suite) in self.variant.suites().iter().enumerate() {
            for (bit, char) in ['1', '2', '3', '4', '5'].iter().enumerate() {
                std::fmt::Display::fmt(
                    &if (1 << bit) & self.cards[index] > 0 {
                        char.to_string().color(suite.color()).bold()
                    } else {
                        char.to_string().color(suite.color()).strikethrough()
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

    pub fn limit_by_suite(&mut self, suite: &game::Suite, effect: bool) {
        let index = self.variant.suite_index(suite);
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
        let index = self.variant.suite_index(&card.suite);
        let rank_bit = 1 << (card.rank - 1);
        self.cards[index] |= rank_bit;
    }

    pub fn remove_card(&mut self, card: &game::Card) {
        let index = self.variant.suite_index(&card.suite);
        let rank_bit = !(1 << (card.rank - 1));
        self.cards[index] &= rank_bit;
    }

    pub fn is_rank(&self, rank: u8) -> bool {
        let bit_test = !(1 << (rank - 1));
        for suite_index in 0..self.variant.suites().len() {
            if self.cards[suite_index] & bit_test > 0 {
                return false;
            }
        }
        true
    }
}

impl<'a> CardQuantum {
    pub fn iter(&'a self) -> CardIterator<'a> {
        CardIterator {
            variant: &self.variant,
            cards: &self.cards,
            current_card: game::Card {
                suite: self.variant.suites()[0],
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
        let mut current_index = self.variant.suite_index(&self.current_card.suite);

        let remaining_bits = self.cards[current_index] >> self.current_card.rank;
        if remaining_bits != 0 {
            // card in same suite in quantum
            self.current_card.rank += remaining_bits.trailing_zeros() as u8 + 1;
            return Some(self.current_card);
        }
        let suites = self.variant.suites();
        // switch to next suites
        while current_index + 1 < suites.len() {
            current_index += 1;
            if self.cards[current_index] == 0 {
                continue;
            }
            self.current_card = game::Card {
                suite: self.variant.suites()[current_index],
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
        assert_eq!(c.cards[5], 0b11111);
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
        assert_eq!(c.cards[5], 0b11111);
        c.clear();
        assert_eq!(c.cards[0], 0);
        assert_eq!(c.cards[1], 0);
        assert_eq!(c.cards[2], 0);
        assert_eq!(c.cards[3], 0);
        assert_eq!(c.cards[4], 0);
        assert_eq!(c.cards[5], 0);
    }
}
