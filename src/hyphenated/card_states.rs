use crate::card_quantum::Variant;
use crate::game::{self, CardPlayState};
use crate::CardQuantum;

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct CardState {
    pub play: CardPlayState,
    pub clued: Option<u8>,
    pub locked: Option<(u8, i8)>,
    pub tracked_count: u8,
    pub tracked_places: [i8; 3],
}

impl CardState {
    fn new() -> Self {
        Self {
            play: CardPlayState::Normal(),
            clued: None,
            locked: None,
            tracked_count: 0,
            tracked_places: [-2; 3],
        }
    }
}

impl std::fmt::Debug for CardState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.play {
            CardPlayState::Trash() => return f.write_str("✓"),
            CardPlayState::Dead() => f.write_str("🗑")?,
            CardPlayState::Critical() => f.write_str("⚠ ")?,
            CardPlayState::CriticalPlayable() => f.write_str("⚠▶")?,
            CardPlayState::Playable() => f.write_str("▶ ")?,
            CardPlayState::Normal() => (),
        }
        if let Some(clued) = self.clued {
            f.write_str("'")?;
            if clued < 255 {
                std::fmt::Debug::fmt(&clued, f)?;
            }
        }
        if let Some(turn) = self.locked {
            f.write_str("L")?;
            std::fmt::Debug::fmt(&turn, f)?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone)]
pub struct CardStates {
    variant: Variant,
    states: [CardState; 25],
    first_one_discard: [bool; 5],
    pub trash_quantum: CardQuantum,
    pub play_quantum: CardQuantum,
}

impl std::fmt::Debug for CardStates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(
                self.iter()
                    .filter(|(_card, state)| state.play != game::CardPlayState::Trash()),
            )
            .finish()
    }
}

impl CardStates {
    pub fn new() -> Self {
        let v = Variant {};
        let mut states = [CardState::new(); 25];
        for suit in v.suits().iter() {
            states[v.suit_index(suit) * 5].play = CardPlayState::Playable();
            states[v.suit_index(suit) * 5 + 5 - 1].play = CardPlayState::Critical();
        }
        let mut trash_quantum = CardQuantum::new(v);
        trash_quantum.clear();
        let mut play_quantum = CardQuantum::new(v);
        play_quantum.limit_by_rank(1, true);
        Self {
            variant: v,
            states,
            first_one_discard: [false; 5],
            trash_quantum,
            play_quantum,
        }
    }

    pub fn played(&mut self, card: &game::Card) {
        let offset = self.variant.suit_index(&card.suit) * 5;
        self.states[offset + card.rank as usize - 1].play = CardPlayState::Trash();
        self.trash_quantum.add_card(card, false);
        self.play_quantum.remove_card(card, false);
        if card.rank == 5 {
            return;
        }
        match self.states[offset + card.rank as usize].play {
            CardPlayState::Normal() => {
                self.play_quantum.add_card(
                    &game::Card {
                        suit: card.suit,
                        rank: card.rank + 1,
                    },
                    false,
                );
                self.states[offset + card.rank as usize].play = CardPlayState::Playable()
            }
            CardPlayState::Critical() => {
                self.play_quantum.add_card(
                    &game::Card {
                        suit: card.suit,
                        rank: card.rank + 1,
                    },
                    false,
                );
                self.states[offset + card.rank as usize].play = CardPlayState::CriticalPlayable()
            }
            _ => {}
        }
    }

    pub fn discarded(&mut self, card: &game::Card) {
        let offset = self.variant.suit_index(&card.suit) * 5;
        if card.rank == 1 && !self.first_one_discard[self.variant.suit_index(&card.suit)] {
            self.first_one_discard[self.variant.suit_index(&card.suit)] = true;
            return;
        }
        match self.states[offset + card.rank as usize - 1].play {
            CardPlayState::Normal() => {
                self.states[offset + card.rank as usize - 1].play = CardPlayState::Critical()
            }
            CardPlayState::Playable() => {
                self.states[offset + card.rank as usize - 1].play =
                    CardPlayState::CriticalPlayable()
            }
            CardPlayState::Critical() => {
                for higher_rank in card.rank..=5 {
                    self.states[offset + higher_rank as usize - 1].play = CardPlayState::Dead();
                    self.trash_quantum.add_card(
                        &game::Card {
                            suit: card.suit,
                            rank: higher_rank,
                        },
                        false,
                    );
                }
            }
            CardPlayState::CriticalPlayable() => {
                for higher_rank in card.rank..=5 {
                    self.states[offset + higher_rank as usize - 1].play = CardPlayState::Dead();
                    self.play_quantum.remove_card(card, false);
                    self.trash_quantum.add_card(
                        &game::Card {
                            suit: card.suit,
                            rank: higher_rank,
                        },
                        false,
                    );
                }
            }
            _ => {}
        }
    }

    pub fn iter(&self) -> CardStateIterator<'_> {
        CardStateIterator {
            card_states: self,
            next_pos: 0,
            only_clued: false,
        }
    }

    pub fn iter_clued(&self) -> CardStateIterator<'_> {
        CardStateIterator {
            card_states: self,
            next_pos: 0,
            only_clued: true,
        }
    }
}

impl core::ops::Index<&game::Card> for CardStates {
    type Output = CardState;

    fn index(&self, card: &game::Card) -> &Self::Output {
        &self.states[self.variant.suit_index(&card.suit) * 5 + card.rank as usize - 1]
    }
}

impl core::ops::IndexMut<&game::Card> for CardStates {
    fn index_mut(&mut self, card: &game::Card) -> &mut Self::Output {
        &mut self.states[self.variant.suit_index(&card.suit) * 5 + card.rank as usize - 1]
    }
}

pub struct CardStateIterator<'a> {
    card_states: &'a CardStates,
    next_pos: u8,
    only_clued: bool,
}

impl<'a> Iterator for CardStateIterator<'a> {
    type Item = (game::Card, &'a CardState);

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_pos < 25 {
            let card_state = &self.card_states.states[self.next_pos as usize];
            if self.only_clued && card_state.clued.is_none() {
                self.next_pos += 1;
                continue;
            }
            let card = game::Card {
                rank: self.next_pos % 5 + 1,
                suit: self.card_states.variant.suits()[(self.next_pos / 5) as usize],
            };
            self.next_pos += 1;
            return Some((card, card_state));
        }
        None
    }
}

impl Default for CardStates {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let suit = game::Suit::Red();
        let p = CardStates::new();
        assert_eq!(
            p[&game::Card { rank: 1, suit }].play,
            CardPlayState::Playable()
        );
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::Normal()
        );
        assert_eq!(
            p[&game::Card { rank: 3, suit }].play,
            CardPlayState::Normal()
        );
        assert_eq!(
            p[&game::Card { rank: 4, suit }].play,
            CardPlayState::Normal()
        );
        assert_eq!(
            p[&game::Card { rank: 5, suit }].play,
            CardPlayState::Critical()
        );
    }

    #[test]
    fn play_card() {
        let suit = game::Suit::Red();
        let mut p = CardStates::new();
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::Normal()
        );
        p.played(&game::Card { rank: 1, suit });
        assert_eq!(
            p[&game::Card { rank: 1, suit }].play,
            CardPlayState::Trash()
        );
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::Playable()
        );
        p.played(&game::Card { rank: 2, suit });
        assert_eq!(
            p[&game::Card { rank: 3, suit }].play,
            CardPlayState::Playable()
        );
        p.played(&game::Card { rank: 3, suit });
        assert_eq!(
            p[&game::Card { rank: 4, suit }].play,
            CardPlayState::Playable()
        );
        p.played(&game::Card { rank: 4, suit });
        assert_eq!(
            p[&game::Card { rank: 5, suit }].play,
            CardPlayState::CriticalPlayable()
        );
    }

    #[test]
    fn discard_card() {
        let suit = game::Suit::Blue();
        let mut p = CardStates::new();
        p.discarded(&game::Card { rank: 3, suit });
        assert_eq!(
            p[&game::Card { rank: 3, suit }].play,
            CardPlayState::Critical()
        );
        p.discarded(&game::Card { rank: 3, suit });
        assert_eq!(p[&game::Card { rank: 3, suit }].play, CardPlayState::Dead());
        assert_eq!(p[&game::Card { rank: 4, suit }].play, CardPlayState::Dead());
        assert_eq!(p[&game::Card { rank: 5, suit }].play, CardPlayState::Dead());
    }

    #[test]
    fn play_critical() {
        let suit = game::Suit::Yellow();
        let mut p = CardStates::new();
        p.discarded(&game::Card { rank: 2, suit });
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::Critical()
        );
        p.played(&game::Card { rank: 1, suit });
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::CriticalPlayable()
        );
        p.played(&game::Card { rank: 2, suit });
        assert_eq!(
            p[&game::Card { rank: 2, suit }].play,
            CardPlayState::Trash()
        );
    }
}
