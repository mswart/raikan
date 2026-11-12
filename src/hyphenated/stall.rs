//! Stalling contexts and conservative, non-forcing clue interpretations.
//! https://hanabi.github.io/level-9#allowable-stall-clues-stall-table
//!
//! Anxiety plays, scream discards and the finesse-position exception are
//! separate conventions, not implemented here.

use super::Line;
use crate::{
    PositionSet,
    game::{Card, CardPlayState, Clue},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TurnContext {
    pub clues: u8,
    pub actions_taken: u16,
    pub early_game: bool,
    pub early_five_stalled: bool,
    pub last_loss: Option<(u8, Card)>,
}

impl Default for TurnContext {
    fn default() -> Self {
        Self {
            clues: 8,
            actions_taken: 0,
            early_game: true,
            early_five_stalled: false,
            last_loss: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum StallSeverity {
    Normal,
    Early,
    DoubleDiscard,
    Locked,
    EightClues,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StallKind {
    Five,
    Tempo,
    FillIn,
    LockedSave,
    EightClueSave,
    Burn,
}

impl StallKind {
    pub fn precedence(self) -> u8 {
        match self {
            Self::Five => 0,
            Self::Tempo => 1,
            Self::FillIn | Self::LockedSave | Self::EightClueSave => 2,
            Self::Burn => 3,
        }
    }

    pub fn is_general_save(self) -> bool {
        matches!(self, Self::LockedSave | Self::EightClueSave)
    }
}

impl Line {
    pub(super) fn stall_severity(&self, giver: u8) -> StallSeverity {
        use StallSeverity::*;
        if self.context.clues == 0 {
            return Normal;
        }
        let rotated;
        let view = if giver == 0 {
            self
        } else {
            rotated = self.decision_view(giver);
            &rotated
        };
        if view.next_play().is_some() {
            return Normal;
        }
        if self.context.clues == 8 && self.context.actions_taken > 0 {
            return EightClues;
        }
        if self.context.clues < 8 && view.hands.iter_hand(0).any(|(_, s)| s.trash) {
            return Normal;
        }
        if view.chop(0).is_none() {
            return Locked;
        }
        if let Some((previous, card)) = self.context.last_loss
            && (previous + 1) % self.hands.num_players == giver
            && matches!(
                self.card_states[&card].play,
                CardPlayState::Critical() | CardPlayState::CriticalPlayable()
            )
        {
            let crate::game::Move::Discard(pos) = view.discard() else {
                unreachable!()
            };
            let visible_elsewhere = (1..view.hands.num_players)
                .any(|p| view.hands.iter_hand(p).any(|(_, s)| s.card == card));
            if !visible_elsewhere && view.hands.slot(0, pos).quantum.contains(&card) {
                return DoubleDiscard;
            }
        }
        if self.context.early_game {
            Early
        } else {
            Normal
        }
    }

    pub(super) fn stall_kind(
        &self,
        giver: u8,
        receiver: u8,
        clue: Clue,
        touched: PositionSet,
    ) -> Option<StallKind> {
        if self.fix_kind(receiver, clue, touched).is_some() {
            return None;
        }
        let severity = self.stall_severity(giver);
        if severity == StallSeverity::Normal || touched.is_empty() {
            return None;
        }
        let newly_touched = !self.newly_touched(receiver, touched).is_empty();
        if newly_touched {
            let chop = self.chop(receiver);
            if clue == Clue::Rank(5) && chop.is_none_or(|pos| !touched.contains(pos)) {
                return Some(StallKind::Five);
            }
            let on_chop = chop.is_some_and(|pos| touched.contains(pos));
            // Rank-2 and rank-5 saves on chop retain their ordinary meaning.
            if on_chop && matches!(clue, Clue::Rank(2 | 5)) {
                return None;
            }
            let focus = self.clue_focus(receiver, touched)?;
            let mut possibilities = self.hands.slot(receiver, focus).quantum;
            match clue {
                Clue::Rank(rank) => possibilities.limit_by_rank(rank as usize, true),
                Clue::Color(color) => possibilities.limit_by_suit(&color.suit(), true),
            }
            if possibilities.size() > 0 && self.card_states.play_quantum.superset(possibilities) {
                return None;
            }
            if severity == StallSeverity::Locked
                && on_chop
                && self.hands.iter_hand(receiver).any(|(pos, slot)| {
                    !slot.clued && slot.promised.is_none() && !touched.contains(pos)
                })
            {
                return Some(StallKind::LockedSave);
            }
            if severity == StallSeverity::EightClues && focus != 0 {
                return Some(StallKind::EightClueSave);
            }
            return None;
        }
        if severity < StallSeverity::DoubleDiscard {
            return None;
        }
        let mut informative = false;
        for pos in touched.iter() {
            let slot = self.hands.slot(receiver, pos);
            let mut narrowed = slot.quantum;
            match clue {
                Clue::Rank(rank) => narrowed.limit_by_rank(rank as usize, true),
                Clue::Color(color) => narrowed.limit_by_suit(&color.suit(), true),
            }
            informative |= narrowed.hard_size() < slot.quantum.hard_size();
        }
        let focus = self.stall_focus(receiver, touched).expect("nonempty clue");
        let slot = self.hands.slot(receiver, focus);
        let mut narrowed = slot.quantum;
        match clue {
            Clue::Rank(rank) => narrowed.limit_by_rank(rank as usize, true),
            Clue::Color(color) => narrowed.limit_by_suit(&color.suit(), true),
        }
        let playable =
            !slot.play && slot.delayed == 0 && self.card_states.play_quantum.interset(narrowed);
        Some(if playable {
            StallKind::Tempo
        } else if informative {
            StallKind::FillIn
        } else {
            StallKind::Burn
        })
    }

    pub(super) fn stall_focus(&self, receiver: u8, touched: PositionSet) -> Option<u8> {
        touched
            .iter()
            .find(|pos| {
                let slot = self.hands.slot(receiver, *pos);
                !slot.play || slot.delayed > 0
            })
            .or_else(|| touched.first())
    }

    pub(super) fn clue_stall(&self, receiver: u8, clue: Clue) -> Option<StallKind> {
        let mut touched = PositionSet::new(self.hands.hand_sizes[receiver as usize]);
        for (pos, slot) in self.hands.iter_hand(receiver) {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        self.stall_kind(0, receiver, clue, touched)
    }

    pub(super) fn five_stall_distance(&self, receiver: u8) -> u8 {
        let mut distance = 0;
        for (_, slot) in self.hands.iter_hand(receiver).rev() {
            if slot.clued || slot.promised.is_some() {
                continue;
            }
            if slot.card.rank == 5 {
                return distance;
            }
            distance += 1;
        }
        u8::MAX
    }

    /// A general save can also protect a normally saveable critical card. Its
    /// interpretation stays conservative, but generation must prefer that
    /// ordinary save over lower-precedence stalls (including off-chop fives).
    pub(super) fn saves_critical_chop(&self, receiver: u8, clue: Clue) -> bool {
        self.chop(receiver)
            .map(|pos| self.hands.slot(receiver, pos))
            .is_some_and(|slot| {
                slot.card.rank != 0
                    && slot.card.affected(clue)
                    && (slot.card.rank != 5 || clue == Clue::Rank(5))
                    && matches!(
                        self.card_states[&slot.card].play,
                        CardPlayState::Critical() | CardPlayState::CriticalPlayable()
                    )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::super::search;
    use super::*;
    use crate::game::{ClueColor, GameStatus, Move, Suit};

    fn c(suit: Suit, rank: u8) -> Card {
        Card { suit, rank }
    }

    fn hands() -> [[Card; 4]; 4] {
        use Suit::*;
        [
            [c(Red(), 3), c(Red(), 4), c(Yellow(), 3), c(Yellow(), 4)],
            [c(Green(), 3), c(Blue(), 5), c(Green(), 4), c(Blue(), 3)],
            [c(Purple(), 5), c(Blue(), 4), c(Purple(), 3), c(Purple(), 4)],
            [c(Red(), 3), c(Yellow(), 3), c(Green(), 3), c(Blue(), 3)],
        ]
    }

    fn deal(hands: [[Card; 4]; 4]) -> [Line; 4] {
        let mut views = std::array::from_fn(|i| Line::new(4, i as u8));
        for (player, hand) in hands.iter().enumerate() {
            for &card in hand.iter().rev() {
                for (observer, view) in views.iter_mut().enumerate() {
                    if player == observer {
                        view.own_drawn();
                    } else {
                        view.drawn((4 + player - observer) % 4, card);
                    }
                }
            }
        }
        views
    }

    fn clue(views: &mut [Line; 4], giver: usize, receiver: usize, clue: Clue) {
        let mut touched =
            PositionSet::new(views[giver].hands.hand_sizes[(4 + receiver - giver) % 4]);
        for (pos, slot) in views[giver]
            .hands
            .iter_hand(((4 + receiver - giver) % 4) as u8)
        {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        for (observer, view) in views.iter_mut().enumerate() {
            view.clued(
                (4 + giver - observer) % 4,
                (4 + receiver - observer) % 4,
                clue,
                touched,
            );
        }
    }

    fn discard(views: &mut [Line; 4], player: usize, pos: u8) {
        let card = views[(player + 1) % 4].hands.slot(3, pos).card;
        for (observer, view) in views.iter_mut().enumerate() {
            view.discarded((4 + player - observer) % 4, pos as usize, card);
        }
    }

    fn know(views: &mut [Line; 4], player: usize, pos: u8, clue: Clue) {
        for (observer, view) in views.iter_mut().enumerate() {
            let slot = view
                .hands
                .slot_mut(((4 + player - observer) % 4) as u8, pos);
            slot.clued = true;
            slot.apply_clue(clue, true);
            slot.update_slot_attributes(&view.card_states);
        }
    }

    fn lock_giver(views: &mut [Line; 4]) {
        for view in views.iter_mut() {
            view.context.clues = 3;
            view.context.early_game = false;
        }
        for pos in 0..4 {
            know(views, 0, pos, Clue::Rank(hands()[0][pos as usize].rank));
        }
    }

    fn choose(line: &Line) -> Move {
        search::choose_action(
            line,
            &GameStatus {
                turn: 0,
                score: 0,
                max_score: 25,
                num_strikes: 0,
                clues: line.context.clues,
                blind_plays: 0,
            },
            false,
        )
    }

    #[test]
    fn early_five_stall_is_saved_in_every_players_view() {
        let mut views = deal(hands());
        assert_eq!(
            choose(&views[0]),
            Move::Clue(1, Clue::Rank(5)),
            "choose the five closest to chop"
        );
        clue(&mut views, 0, 1, Clue::Rank(5));
        for (observer, view) in views.iter().enumerate() {
            let slot = view.hands.slot(((5 - observer) % 4) as u8, 1);
            assert!(slot.clued && slot.quantum.is_rank(5));
            assert!(!slot.play && slot.promised.is_none());
            assert!(view.interpretations.is_empty());
            assert!(view.context.early_game && view.context.early_five_stalled);
            assert_eq!(view.context.clues, 7);
        }
        assert!(
            matches!(choose(&views[1]), Move::Discard(_)),
            "a second early five stall is optional"
        );
    }

    #[test]
    fn normal_play_clue_precedes_a_five_stall() {
        let mut hands = hands();
        hands[3][0] = c(Suit::Red(), 1);
        let views = deal(hands);
        let action = choose(&views[0]);
        assert!(matches!(
            action,
            Move::Clue(3, Clue::Rank(1) | Clue::Color(ClueColor::Red()))
        ));
    }

    #[test]
    fn off_chop_fives_are_not_stalls_in_an_ordinary_midgame() {
        let mut views = deal(hands());
        for view in &mut views {
            view.context.early_game = false;
            view.context.clues = 3;
        }
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), None);
        assert!(
            views[0]
                .clone()
                .clue(1, Clue::Rank(5))
                .unwrap()
                .has_errors()
        );
    }

    #[test]
    fn on_chop_five_remains_a_normal_save() {
        let mut hands = hands();
        hands[1].swap(1, 3);
        let views = deal(hands);
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), None);
        assert!(
            !views[0]
                .clone()
                .clue(1, Clue::Rank(5))
                .unwrap()
                .has_errors()
        );
    }

    #[test]
    fn locked_hand_fill_and_burn_do_not_create_play_instructions() {
        let mut hands = hands();
        hands[1][3] = c(Suit::Yellow(), 2);
        let mut views = deal(hands);
        lock_giver(&mut views);
        know(&mut views, 1, 1, Clue::Rank(5));
        assert_eq!(views[0].stall_severity(0), StallSeverity::Locked);
        assert_eq!(
            views[0].clue_stall(1, Clue::Color(ClueColor::Blue())),
            Some(StallKind::FillIn)
        );
        clue(&mut views, 0, 1, Clue::Color(ClueColor::Blue()));
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), Some(StallKind::Burn));
        clue(&mut views, 0, 1, Clue::Rank(5));
        for (observer, view) in views.iter().enumerate() {
            let slot = view.hands.slot(((5 - observer) % 4) as u8, 1);
            assert_eq!(slot.quantum.to_vec(), vec![c(Suit::Blue(), 5)]);
            assert!(!slot.play && slot.promised.is_none());
            assert!(view.interpretations.is_empty());
        }
    }

    #[test]
    fn tempo_stall_can_make_a_saved_card_literally_playable() {
        let mut hands = hands();
        hands[1][2] = c(Suit::Yellow(), 2);
        let mut views = deal(hands);
        lock_giver(&mut views);
        know(&mut views, 1, 0, Clue::Rank(3));
        for view in &mut views {
            view.card_states.played(&c(Suit::Green(), 1));
            view.card_states.played(&c(Suit::Green(), 2));
        }
        assert_eq!(
            views[0].clue_stall(1, Clue::Color(ClueColor::Green())),
            Some(StallKind::Tempo)
        );
        clue(&mut views, 0, 1, Clue::Color(ClueColor::Green()));
        for (observer, view) in views.iter().enumerate() {
            let slot = view.hands.slot(((5 - observer) % 4) as u8, 0);
            assert!(slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![c(Suit::Green(), 3)]);
            assert!(view.interpretations.is_empty());
        }
    }

    #[test]
    fn ambiguous_tempo_promises_only_the_leftmost_unplayed_focus() {
        let mut cards = hands();
        cards[1][0] = c(Suit::Blue(), 5);
        cards[1][1] = c(Suit::Green(), 5);
        let mut views = deal(cards);
        lock_giver(&mut views);
        know(&mut views, 1, 0, Clue::Rank(5));
        know(&mut views, 1, 1, Clue::Rank(5));
        for view in &mut views {
            for rank in 1..5 {
                view.card_states.played(&c(Suit::Blue(), rank));
            }
        }
        assert_eq!(
            views[0].clue_stall(1, Clue::Rank(5)),
            Some(StallKind::Tempo)
        );
        clue(&mut views, 0, 1, Clue::Rank(5));
        for (observer, view) in views.iter().enumerate() {
            let receiver = ((5 - observer) % 4) as u8;
            let focus = view.hands.slot(receiver, 0);
            assert_eq!(focus.quantum.to_vec(), vec![c(Suit::Blue(), 5)]);
            assert!(focus.play);
            assert!(
                !view.hands.slot(receiver, 1).play,
                "the ancillary five is not promised to play"
            );
            assert!(
                view.interpretations.is_empty(),
                "tempo does not create a finesse chain"
            );
        }
    }

    #[test]
    fn eight_clue_stalls_apply_after_the_opening_turn_even_in_early_game() {
        let mut views = deal(hands());
        know(&mut views, 1, 1, Clue::Rank(5));
        assert_eq!(views[0].stall_severity(0), StallSeverity::Early);
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), None);
        for view in &mut views {
            view.context.actions_taken = 2;
        }
        assert_eq!(views[0].stall_severity(0), StallSeverity::EightClues);
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), Some(StallKind::Burn));
        assert!(matches!(choose(&views[0]), Move::Clue(..)));
    }

    #[test]
    fn double_discard_uses_the_givers_information_in_all_views() {
        let mut hands = hands();
        hands[3][3] = c(Suit::Red(), 4);
        let mut views = deal(hands);
        for view in &mut views {
            view.context.clues = 2;
            view.context.early_game = false;
        }
        discard(&mut views, 3, 3);
        for (observer, view) in views.iter().enumerate() {
            assert_eq!(
                view.stall_severity(((4 - observer) % 4) as u8),
                StallSeverity::DoubleDiscard
            );
        }
        assert_eq!(choose(&views[0]), Move::Clue(1, Clue::Rank(5)));
        clue(&mut views, 0, 1, Clue::Rank(5));
        for (observer, view) in views.iter().enumerate() {
            assert!(!view.hands.slot(((5 - observer) % 4) as u8, 1).play);
            assert!(view.context.last_loss.is_none());
        }
    }

    #[test]
    fn visible_copy_or_negative_information_excludes_double_discard() {
        let mut cards = hands();
        cards[3][3] = c(Suit::Red(), 4);
        cards[0][1] = c(Suit::Green(), 2);
        cards[1][0] = c(Suit::Red(), 4);
        let mut views = deal(cards);
        for view in &mut views {
            view.context.clues = 2;
            view.context.early_game = false;
        }
        discard(&mut views, 3, 3);
        assert_eq!(views[0].stall_severity(0), StallSeverity::Normal);

        let mut cards = hands();
        cards[3][3] = c(Suit::Red(), 4);
        let mut views = deal(cards);
        for (observer, view) in views.iter_mut().enumerate() {
            view.context.clues = 2;
            view.context.early_game = false;
            view.hands
                .slot_mut(((4 - observer) % 4) as u8, 3)
                .apply_clue(Clue::Color(ClueColor::Red()), false);
        }
        discard(&mut views, 3, 3);
        assert_eq!(views[0].stall_severity(0), StallSeverity::Normal);
    }

    #[test]
    fn only_the_second_discarded_one_creates_double_discard_danger() {
        let mut cards = hands();
        cards[3][2] = c(Suit::Red(), 1);
        cards[3][3] = c(Suit::Red(), 1);
        let mut views = deal(cards);
        for view in &mut views {
            view.context.clues = 2;
        }
        discard(&mut views, 3, 3);
        assert_eq!(views[0].stall_severity(0), StallSeverity::Normal);
        discard(&mut views, 3, 2);
        assert_eq!(views[0].stall_severity(0), StallSeverity::DoubleDiscard);
    }

    #[test]
    fn public_actions_track_tokens_and_the_end_of_early_game() {
        let mut cards = hands();
        cards[0][0] = c(Suit::Red(), 1);
        let mut views = deal(cards);
        know(&mut views, 0, 0, Clue::Rank(1));
        for view in &mut views {
            view.context.clues = 2;
            view.card_states.played(&c(Suit::Red(), 1));
        }
        discard(&mut views, 0, 0);
        assert!(
            views
                .iter()
                .all(|v| v.context.early_game && v.context.clues == 3)
        );
        discard(&mut views, 1, 3);
        assert!(
            views
                .iter()
                .all(|v| !v.context.early_game && v.context.clues == 4)
        );

        let mut cards = hands();
        cards[0][0] = c(Suit::Red(), 5);
        let mut views = deal(cards);
        for (observer, view) in views.iter_mut().enumerate() {
            view.context.clues = 7;
            for rank in 1..5 {
                view.card_states.played(&c(Suit::Red(), rank));
            }
            view.played((4 - observer) % 4, 0, c(Suit::Red(), 5), true);
            assert_eq!(view.context.clues, 8);
            view.played((5 - observer) % 4, 0, c(Suit::Green(), 3), false);
            assert_eq!(view.context.clues, 8);
            assert!(view.context.early_game, "misplays do not end early game");
        }
    }

    #[test]
    fn known_safe_discard_precedes_an_early_five_stall() {
        let mut cards = hands();
        cards[0][0] = c(Suit::Red(), 1);
        let mut views = deal(cards);
        know(&mut views, 0, 0, Clue::Rank(1));
        know(&mut views, 0, 0, Clue::Color(ClueColor::Red()));
        for (observer, view) in views.iter_mut().enumerate() {
            view.context.clues = 3;
            view.card_states.played(&c(Suit::Red(), 1));
            // Refresh the test position after the public stack change.
            let own = ((4 - observer) % 4) as u8;
            let slot = view.hands.slot_mut(own, 0);
            slot.update_slot_attributes(&view.card_states);
        }
        assert_eq!(choose(&views[0]), Move::Discard(0));
    }

    #[test]
    fn five_stalls_precede_fill_ins_and_general_saves() {
        let mut cards = hands();
        cards[1][3] = c(Suit::Green(), 4);
        let mut views = deal(cards);
        lock_giver(&mut views);
        know(&mut views, 1, 1, Clue::Rank(5));
        assert_eq!(choose(&views[0]), Move::Clue(2, Clue::Rank(5)));
        know(&mut views, 2, 0, Clue::Rank(5));
        let Move::Clue(receiver, clue) = choose(&views[0]) else {
            panic!("a locked hand should give a clue instead of discarding a protected card");
        };
        assert_eq!(
            views[0].clue_stall(receiver, clue),
            Some(StallKind::LockedSave)
        );
        assert_eq!(
            views[0].clue_stall(1, Clue::Color(ClueColor::Blue())),
            Some(StallKind::FillIn),
            "the save may be chosen even when a fill-in is available"
        );
    }

    #[test]
    fn locked_hand_saves_accept_color_and_rank_without_promising_a_play() {
        for save in [Clue::Rank(3), Clue::Color(ClueColor::Blue())] {
            let mut views = deal(hands());
            lock_giver(&mut views);
            assert_eq!(views[0].clue_stall(1, save), Some(StallKind::LockedSave));
            clue(&mut views, 0, 1, save);
            for (observer, view) in views.iter().enumerate() {
                let receiver = ((5 - observer) % 4) as u8;
                let saved = view.hands.slot(receiver, 3);
                assert!(saved.clued && saved.quantum.contains(&c(Suit::Blue(), 3)));
                assert!(!saved.play && saved.promised.is_none());
                assert!(view.interpretations.is_empty());
                assert!(view.hands.iter_hand(receiver).any(|(_, slot)| !slot.clued));
                assert_eq!(view.context.clues, 2);
            }
            assert_ne!(views[1].discard(), Move::Discard(3));
        }
    }

    #[test]
    fn locked_hand_saves_must_touch_chop_and_leave_an_unclued_card() {
        let mut views = deal(hands());
        lock_giver(&mut views);
        assert_eq!(views[0].clue_stall(1, Clue::Rank(4)), None, "off chop");
        // Blue touches BOTH remaining unclued cards. Checking only the number
        // of unclued cards before the clue would incorrectly allow this save.
        know(&mut views, 1, 0, Clue::Rank(3));
        know(&mut views, 1, 2, Clue::Rank(4));
        let save = Clue::Color(ClueColor::Blue());
        assert_eq!(views[0].clue_stall(1, save), None);
        clue(&mut views, 0, 1, save);
        assert!(
            !views[1].interpretations.is_empty(),
            "a clue that locks the receiver keeps its normal play interpretation"
        );
    }

    #[test]
    fn eight_clue_saves_accept_chop_and_older_off_chop_cards() {
        for (pos, save) in [
            (2, Clue::Rank(4)),
            (1, Clue::Color(ClueColor::Yellow())),
            (3, Clue::Rank(3)),
            (3, Clue::Color(ClueColor::Blue())),
        ] {
            let mut cards = hands();
            cards[1][1] = c(Suit::Yellow(), 2);
            let mut views = deal(cards);
            for view in &mut views {
                view.context.actions_taken = 2;
            }
            assert_eq!(views[0].clue_stall(1, save), Some(StallKind::EightClueSave));
            clue(&mut views, 0, 1, save);
            for (observer, view) in views.iter().enumerate() {
                let receiver = ((5 - observer) % 4) as u8;
                let saved = view.hands.slot(receiver, pos);
                assert!(saved.clued && saved.quantum.contains(&cards[1][pos as usize]));
                assert!(!saved.play && saved.promised.is_none());
                assert!(view.interpretations.is_empty());
                assert_eq!(view.context.clues, 7);
            }
            assert_ne!(views[1].discard(), Move::Discard(pos));
        }
    }

    #[test]
    fn eight_clue_save_excludes_the_opening_turn_and_the_newest_focus() {
        let mut cards = hands();
        cards[1][0] = c(Suit::Yellow(), 4);
        let mut views = deal(cards);
        assert_eq!(views[0].clue_stall(1, Clue::Rank(3)), None);
        for view in &mut views {
            view.context.actions_taken = 2;
        }
        let play = Clue::Color(ClueColor::Yellow());
        assert_eq!(views[0].clue_stall(1, play), None);
        clue(&mut views, 0, 1, play);
        assert!(
            !views[1].interpretations.is_empty(),
            "cluing the newest focus retains its ordinary play interpretation"
        );
    }

    #[test]
    fn eight_clue_save_can_lock_the_receiver() {
        let mut views = deal(hands());
        lock_giver(&mut views);
        for pos in 0..3 {
            know(
                &mut views,
                1,
                pos,
                Clue::Rank(hands()[1][pos as usize].rank),
            );
        }
        for view in &mut views {
            view.context.clues = 8;
            view.context.actions_taken = 2;
        }
        assert_eq!(views[0].stall_severity(0), StallSeverity::EightClues);
        assert_eq!(
            views[0].clue_stall(1, Clue::Rank(3)),
            Some(StallKind::EightClueSave)
        );
        clue(&mut views, 0, 1, Clue::Rank(3));
        for (observer, view) in views.iter().enumerate() {
            let receiver = ((5 - observer) % 4) as u8;
            assert!(view.hands.iter_hand(receiver).all(|(_, slot)| slot.clued));
            assert!(!view.hands.slot(receiver, 3).play);
        }
    }

    #[test]
    fn normal_rank_saves_and_literal_plays_precede_general_saves() {
        for eight in [false, true] {
            for rank in [1, 2, 5] {
                let mut cards = hands();
                cards[1][3] = c(Suit::Red(), rank);
                let mut views = deal(cards);
                lock_giver(&mut views);
                if eight {
                    for view in &mut views {
                        view.context.clues = 8;
                        view.context.actions_taken = 2;
                    }
                }
                assert_eq!(views[0].clue_stall(1, Clue::Rank(rank)), None);
                let Move::Clue(receiver, cl) = choose(&views[0]) else {
                    panic!("a normal play/save clue is available");
                };
                assert_eq!(receiver, 1);
                assert_eq!(views[0].clue_stall(receiver, cl), None);
                clue(&mut views, 0, receiver as usize, cl);
                for (observer, view) in views.iter().enumerate() {
                    let saved = view.hands.slot(((5 - observer) % 4) as u8, 3);
                    assert!(saved.clued && saved.quantum.contains(&c(Suit::Red(), rank)));
                    assert_eq!(saved.play, rank == 1);
                }
            }
        }
    }

    #[test]
    fn saving_a_critical_chop_precedes_an_off_chop_five_stall() {
        for eight in [false, true] {
            let mut cards = hands();
            cards[3][3] = c(Suit::Yellow(), 2);
            let mut views = deal(cards);
            lock_giver(&mut views);
            for view in &mut views {
                view.card_states.discarded(&c(Suit::Blue(), 3));
                if eight {
                    view.context.clues = 8;
                    view.context.actions_taken = 2;
                }
            }
            let Move::Clue(receiver, cl) = choose(&views[0]) else {
                panic!("the last blue three must be saved");
            };
            assert_eq!(receiver, 1);
            assert!(views[0].saves_critical_chop(receiver, cl));
            clue(&mut views, 0, receiver as usize, cl);
            for (observer, view) in views.iter().enumerate() {
                let saved = view.hands.slot(((5 - observer) % 4) as u8, 3);
                assert!(saved.clued && saved.quantum.contains(&c(Suit::Blue(), 3)));
                assert!(!saved.play);
            }
        }
    }

    #[test]
    fn eight_token_replay_prefers_a_normal_five_save_to_a_color_stall() {
        use crate::game::{Game, PlayerStrategy};
        use crate::hyphenated::HyphenatedPlayer;
        let mut bots: Vec<_> = (0..4).map(|_| HyphenatedPlayer::new(false)).collect();
        let mut players: Vec<&mut dyn PlayerStrategy> = bots
            .iter_mut()
            .map(|bot| bot as &mut dyn PlayerStrategy)
            .collect();
        let game = Game::from_replay(
            6,
            "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
            "05obaeoabmadbf6bbnDdagbi6c7dbhawuabbia0abraziaiabua11d1bb2udav0ba4uda5uda6asbq0da8wd",
            "0",
            &mut players,
        );
        assert_eq!(game.status.clues, 8);
        assert_eq!(players[2].act(&game.status), Move::Clue(3, Clue::Rank(5)));
    }

    #[test]
    fn tempo_stalls_precede_general_saves() {
        for eight in [false, true] {
            let mut cards = hands();
            cards[1][2] = c(Suit::Yellow(), 2);
            let mut views = deal(cards);
            lock_giver(&mut views);
            know(&mut views, 1, 1, Clue::Rank(5));
            know(&mut views, 2, 0, Clue::Rank(5));
            know(&mut views, 1, 0, Clue::Rank(3));
            for view in &mut views {
                view.card_states.played(&c(Suit::Green(), 1));
                view.card_states.played(&c(Suit::Green(), 2));
                if eight {
                    view.context.clues = 8;
                    view.context.actions_taken = 2;
                }
            }
            let Move::Clue(receiver, cl) = choose(&views[0]) else {
                panic!("a tempo clue is available");
            };
            assert_eq!(views[0].clue_stall(receiver, cl), Some(StallKind::Tempo));
        }
    }

    #[test]
    fn misplays_can_create_double_discard_danger_without_ending_early_game() {
        let mut cards = hands();
        cards[3][3] = c(Suit::Red(), 4);
        let mut views = deal(cards);
        for (observer, view) in views.iter_mut().enumerate() {
            view.context.clues = 2;
            view.played((7 - observer) % 4, 3, c(Suit::Red(), 4), false);
            assert!(view.context.early_game);
            assert_eq!(view.context.clues, 2);
            assert_eq!(
                view.stall_severity(((4 - observer) % 4) as u8),
                StallSeverity::DoubleDiscard
            );
        }
    }

    #[test]
    fn zero_tokens_never_generate_a_stall() {
        let mut views = deal(hands());
        views[0].context.clues = 0;
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), None);
        assert!(matches!(choose(&views[0]), Move::Discard(_)));
    }

    #[test]
    fn burning_a_pending_focus_preserves_its_existing_connection() {
        use super::super::interpretation::{ClueRecord, Hypothesis, Step, StepKind, Tier};
        let mut views = deal(hands());
        lock_giver(&mut views);
        know(&mut views, 1, 1, Clue::Rank(5));
        know(&mut views, 2, 1, Clue::Rank(4));
        for (observer, view) in views.iter_mut().enumerate() {
            for rank in 1..=3 {
                view.card_states.played(&c(Suit::Blue(), rank));
            }
            let receiver = ((5 - observer) % 4) as u8;
            let connector = ((6 - observer) % 4) as u8;
            let focus = view.hands.slot(receiver, 1).id;
            view.interpretations.add(ClueRecord {
                turn: 0,
                giver: ((7 - observer) % 4) as u8,
                receiver,
                focus,
                hypotheses: vec![Hypothesis {
                    focus_card: c(Suit::Blue(), 5),
                    tier: Tier::Direct,
                    steps: smallvec::smallvec![
                        Step {
                            fallback: None,
                            card: c(Suit::Blue(), 4),
                            holder: connector,
                            kind: StepKind::Prompt,
                            slot: view.hands.slot(connector, 1).id,
                            layers: 0,
                            done: false,
                        },
                        Step {
                            fallback: None,
                            card: c(Suit::Blue(), 5),
                            holder: receiver,
                            kind: StepKind::Focus,
                            slot: focus,
                            layers: 0,
                            done: false,
                        },
                    ],
                }],
            });
        }
        assert_eq!(views[0].clue_stall(1, Clue::Rank(5)), Some(StallKind::Burn));
        clue(&mut views, 0, 1, Clue::Rank(5));
        for (observer, view) in views.iter().enumerate() {
            assert_eq!(view.interpretations.records.len(), 1);
            assert_eq!(
                view.interpretations.records[0].hypotheses[0]
                    .pending_steps()
                    .count(),
                2
            );
            assert!(!view.hands.slot(((5 - observer) % 4) as u8, 1).play);
        }
    }

    #[test]
    fn public_context_stays_synchronized_in_complete_games() {
        use crate::{
            game::{Game, GameState, PlayerStrategy},
            hyphenated::HyphenatedPlayer,
        };
        for count in 2..=4 {
            for seed in 0..3 {
                let mut bots: Vec<_> = (0..count).map(|_| HyphenatedPlayer::new(false)).collect();
                let mut players: Vec<&mut dyn PlayerStrategy> = bots
                    .iter_mut()
                    .map(|p| p as &mut dyn PlayerStrategy)
                    .collect();
                let mut game = Game::new(&mut players, false, seed);
                game.run(&mut players);
                assert_ne!(game.state, GameState::Invalid());
                for bot in &bots {
                    assert_eq!(
                        bot.line().context.clues,
                        game.status.clues,
                        "{count} players, seed {seed}"
                    );
                    assert_eq!(
                        bot.line().context.early_game,
                        bots[0].line().context.early_game
                    );
                    assert_eq!(
                        bot.line().context.early_five_stalled,
                        bots[0].line().context.early_five_stalled
                    );
                    assert_eq!(
                        bot.line().context.actions_taken,
                        bots[0].line().context.actions_taken
                    );
                }
            }
        }
    }
}
