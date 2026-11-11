use super::card_states::CardStates;
use crate::card_quantum::CardQuantum;
use crate::game;

/// Stable identity of a drawn card. Unlike the storage index in `Hands`,
/// it is never reused, so long-lived references (e.g. clue interpretations)
/// can point at a card safely.
pub type SlotId = u16;

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct Slot {
    pub id: SlotId,
    pub card: game::Card,
    pub clued: bool,
    pub play: bool,
    pub trash: bool,
    pub quantum: CardQuantum,
    /// Clues and good-touch deductions, before applying live interpretations.
    pub(crate) base_quantum: CardQuantum,
    pub locked: bool,
    pub fixed: bool,
    pub turn: i8,
    pub delayed: u8,
    pub promised: Option<i8>,
}

impl Slot {
    pub(crate) fn remove_card(&mut self, card: &game::Card, soft: bool) {
        self.quantum.remove_card(card, soft);
        self.base_quantum.remove_card(card, soft);
    }

    pub(crate) fn apply_clue(&mut self, clue: game::Clue, touched: bool) {
        for quantum in [&mut self.quantum, &mut self.base_quantum] {
            match clue {
                game::Clue::Rank(rank) => quantum.limit_by_rank(rank as usize, touched),
                game::Clue::Color(color) => quantum.limit_by_suit(&color.suit(), touched),
            }
        }
    }

    pub fn update_slot_attributes(&mut self, card_states: &CardStates) {
        if self.delayed > 0 {
            return;
        }
        if self.quantum.size() == 0 {
            self.trash = true;
            self.play = false;
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
