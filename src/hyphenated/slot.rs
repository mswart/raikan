use super::card_states::CardStates;
use crate::card_quantum::CardQuantum;
use crate::game;

#[derive(PartialEq, Eq, Copy, Clone)]
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
    pub promised: Option<i8>,
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
        if !card_states.play_quantum.interset(self.quantum) {
            self.play = false;
        }
        if card_states.play_quantum.superset(self.quantum) {
            self.play = true;
            self.trash = false;
        }
        if card_states.trash_quantum.superset(self.quantum) {
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
        } else if self.delayed > 0 {
            f.write_str("+")?;
            std::fmt::Display::fmt(&self.delayed, f)?;
        } else if self.play {
            f.write_str("▶ ")?;
        } else {
            f.write_str("  ")?;
        }
        if self.promised.is_some() {
            f.write_str("🔍")?;
        } else {
            f.write_str("  ")?;
        }
        Ok(())
    }
}
