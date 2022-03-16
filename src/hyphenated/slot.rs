use super::card_states::CardStates;
use crate::card_quantum::CardQuantum;
use crate::game;

#[derive(PartialEq, Eq, Clone)]
pub struct Slot {
    pub card: game::Card,
    pub clued: bool,
    pub play: bool,
    pub trash: bool,
    pub quantum: CardQuantum,
    pub locked: bool,
    pub fixed: bool,
    pub turn: i8,
    pub delayed: u8,
    pub callbacks: bool,
}

impl Slot {
    pub fn update_slot_attributes(&mut self, card_states: &CardStates) {
        if self.delayed > 0 {
            return;
        }
        if self.quantum.size() == 0 {
            self.trash = true;
            return;
        }
        let mut all_trash = true;
        let mut all_playable = true;
        let mut non_playable = true;
        for card in self.quantum.iter() {
            match card_states[&card].play {
                game::CardPlayState::Playable() => {
                    all_trash = false;
                    non_playable = false;
                }
                game::CardPlayState::CriticalPlayable() => {
                    all_trash = false;
                    non_playable = false;
                }
                game::CardPlayState::Critical() => {
                    all_trash = false;
                    all_playable = false;
                }
                game::CardPlayState::Normal() => {
                    all_trash = false;
                    all_playable = false;
                }
                game::CardPlayState::Trash() => all_playable = false,
                game::CardPlayState::Dead() => all_playable = false,
            }
        }
        if non_playable {
            self.play = false;
        }
        if all_playable {
            self.play = true;
            self.trash = false;
        }
        if all_trash {
            self.trash = true;
        }
        if self.trash {
            self.play = false;
        }
    }
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.card.rank != 0 {
            std::fmt::Debug::fmt(&self.card, f)?;
            f.write_str(" ")?;
        } else {
            f.write_str("?? ")?;
        }
        std::fmt::Display::fmt(&self.quantum, f)?;
        if self.clued {
            f.write_str("'")?;
        } else {
            f.write_str(" ")?;
        }
        if self.trash {
            f.write_str("kt")?;
        } else if self.play {
            f.write_str("! ")?;
        } else {
            f.write_str("  ")?;
        }
        Ok(())
    }
}
