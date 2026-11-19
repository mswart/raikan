//! Shared level-1 rules, evaluated using the information before a clue.

use super::Line;
use crate::{
    PositionSet,
    game::{Card, CardPlayState, Clue},
};

impl Line {
    /// Finessed cards carry an implicit clue and are no longer on chop.
    pub(super) fn chop(&self, player: u8) -> Option<u8> {
        self.hands
            .iter_hand(player)
            .rev()
            .find(|(_, slot)| !slot.clued && slot.promised.is_none())
            .map(|(pos, _)| pos)
    }

    pub(super) fn newly_touched(&self, receiver: u8, touched: PositionSet) -> PositionSet {
        let mut new = touched;
        for pos in touched.iter() {
            let slot = self.hands.slot(receiver, pos);
            if slot.clued || slot.promised.is_some() {
                new.remove(pos);
            }
        }
        new
    }

    pub(super) fn clue_focus(&self, receiver: u8, touched: PositionSet) -> Option<u8> {
        let new = self.newly_touched(receiver, touched);
        if new.is_empty() {
            touched.first()
        } else {
            self.chop(receiver)
                .filter(|pos| new.contains(*pos))
                .or_else(|| new.first())
        }
    }

    /// Only copies visible to the giver and receiver can rule out a 2 save.
    /// A duplicate on another chop is the explicit double-chop exception.
    pub(super) fn can_save(&self, giver: u8, receiver: u8, clue: Clue, card: Card) -> bool {
        match self.card_states[&card].play {
            CardPlayState::Critical() => card.rank != 5 || clue == Clue::Rank(5),
            CardPlayState::Normal() if card.rank == 2 && clue == Clue::Rank(2) => {
                (0..self.hands.num_players)
                    .filter(|player| *player != giver && *player != receiver)
                    .all(|player| {
                        self.hands
                            .iter_hand(player)
                            .all(|(pos, slot)| slot.card != card || self.chop(player) == Some(pos))
                    })
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{interpretation::StepKind, search};
    use super::*;
    use crate::game::{ClueColor, GameStatus, Move, Suit};

    struct Table {
        hands: [Vec<Card>; 4],
        views: [Line; 4],
    }

    fn card(text: &str) -> Card {
        let bytes = text.as_bytes();
        Card {
            suit: match bytes[0] {
                b'r' => Suit::Red(),
                b'y' => Suit::Yellow(),
                b'g' => Suit::Green(),
                b'b' => Suit::Blue(),
                b'p' => Suit::Purple(),
                _ => panic!("invalid suit"),
            },
            rank: bytes[1] - b'0',
        }
    }

    impl Table {
        fn new(hands: [&str; 4]) -> Self {
            let hands = hands.map(|hand| hand.split_whitespace().map(card).collect::<Vec<_>>());
            let mut views = std::array::from_fn(|observer| Line::new(4, observer as u8));
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
            Self { hands, views }
        }

        fn clue(&mut self, giver: usize, receiver: usize, clue: Clue) {
            let mut touched = PositionSet::new(self.hands[receiver].len() as u8);
            for (pos, card) in self.hands[receiver].iter().enumerate() {
                if card.affected(clue) {
                    touched.add(pos as u8);
                }
            }
            for (observer, view) in self.views.iter_mut().enumerate() {
                view.clued(
                    (4 + giver - observer) % 4,
                    (4 + receiver - observer) % 4,
                    clue,
                    touched,
                );
            }
        }

        fn play(&mut self, player: usize, pos: usize) {
            let card = self.hands[player].remove(pos);
            for (observer, view) in self.views.iter_mut().enumerate() {
                view.played((4 + player - observer) % 4, pos, card, true);
            }
        }

        fn assert_cards(&self, player: usize, pos: u8, check: impl Fn(&super::super::Slot)) {
            for (observer, view) in self.views.iter().enumerate() {
                check(view.hands.slot(((4 + player - observer) % 4) as u8, pos));
            }
        }
    }

    fn status(clues: u8) -> GameStatus {
        GameStatus {
            clues,
            turn: 0,
            score: 0,
            max_score: 25,
            num_strikes: 0,
            blind_plays: 0,
        }
    }

    #[test]
    fn known_trash_does_not_change_chop_and_finessed_cards_are_protected() {
        let mut table = Table::new(["r3 r4 b3 b4", "y3 y4 g3 g4", "p3 p4 b2 g2", "y2 r2 g1 p2"]);
        let line = &mut table.views[0];
        line.hands.slot_mut(0, 0).clued = true;
        line.hands.slot_mut(0, 0).trash = true;
        assert_eq!(line.chop(0), Some(3));
        assert_eq!(line.discard(), Move::Discard(0));
        line.hands.slot_mut(0, 0).trash = false;
        line.hands.slot_mut(0, 3).promised = Some(0);
        assert_eq!(line.chop(0), Some(2));
        assert_eq!(line.discard(), Move::Discard(2));
    }

    #[test]
    fn five_saves_require_rank_and_non_chop_clues_require_a_play() {
        let table = Table::new(["y3 y4 g3 g4", "r5 r3 b3 b4", "p3 p4 b2 g2", "y2 r4 g1 p2"]);
        assert!(table.views[0].can_save(0, 1, Clue::Rank(5), card("r5")));
        assert!(!table.views[0].can_save(0, 1, Clue::Color(ClueColor::Red()), card("r5")));
        assert!(
            table.views[0]
                .clone()
                .clue(1, Clue::Color(ClueColor::Red()))
                .unwrap()
                .has_errors()
        );
        let table = Table::new(["y3 y4 g3 g4", "r2 r3 b3 b4", "p3 p4 b2 g2", "y2 r4 g1 p2"]);
        assert!(
            table.views[0]
                .clone()
                .clue(1, Clue::Rank(2))
                .unwrap()
                .has_errors(),
            "an off-chop 2 is a play clue, with no r1 connection available"
        );
    }

    #[test]
    fn newly_unplayable_suits_are_eliminated_after_a_discard() {
        let mut table = Table::new(["y3 y4 g3 r3", "r1 r2 b3 b5", "p3 p4 g2 b2", "y2 b4 g1 p2"]);
        for view in &mut table.views {
            view.card_states.discarded(&card("r3"));
        }
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        for (observer, view) in table.views.iter_mut().enumerate() {
            view.discarded((4 - observer) % 4, 3, card("r3"));
        }
        table.assert_cards(1, 1, |slot| {
            assert_eq!(slot.quantum.to_vec(), vec![card("r2")])
        });
    }

    #[test]
    fn a_known_unclued_card_does_not_reserve_its_identity() {
        let mut table = Table::new(["y3 y4 g3 g4", "r2 r2 b3 b4", "p3 p4 b2 g2", "y2 r4 g1 p2"]);
        for (observer, view) in table.views.iter_mut().enumerate() {
            let receiver = ((5 - observer) % 4) as u8;
            let slot = view.hands.slot_mut(receiver, 1);
            slot.quantum.clear();
            slot.quantum.add_card(&card("r2"), false);
            slot.base_quantum = slot.quantum;
        }
        // A negative clue leaves the known-but-unclued r2 unchanged.
        table.clue(0, 1, Clue::Color(ClueColor::Blue()));
        table.assert_cards(1, 0, |slot| assert!(slot.quantum.contains(&card("r2"))));
    }

    #[test]
    fn minimum_clue_value_prefers_discard_to_an_optional_tempo() {
        let mut table = Table::new(["g3 g4 b3 b4", "r2 y3 y4 r3", "p3 p4 g2 b2", "p5 y2 p2 g3"]);
        for view in &mut table.views {
            view.context.early_game = false;
            view.context.clues = 5;
            view.card_states.played(&card("r1"));
            for player in 1..4 {
                for pos in 0..4 {
                    let slot = view.hands.slot_mut(player, pos);
                    slot.clued = true;
                    slot.quantum.limit_by_rank(slot.card.rank as usize, true);
                    slot.base_quantum = slot.quantum;
                }
            }
        }
        assert_eq!(
            search::choose_action(&table.views[0], &status(5), false),
            Move::Discard(3)
        );
    }

    #[test]
    fn saving_a_critical_card_can_violate_good_touch() {
        let mut table = Table::new(["g3 g4 b3 b4", "r4 r4 y3 r3", "p3 p4 g2 b2", "p5 y2 p2 g3"]);
        for view in &mut table.views {
            view.context.early_game = false;
            view.context.clues = 5;
            view.card_states.played(&card("y1"));
            view.card_states.played(&card("y2"));
            view.card_states.played(&card("y3"));
            view.card_states.discarded(&card("r3"));
        }
        assert!(
            matches!(
                search::choose_action(&table.views[0], &status(5), false),
                Move::Clue(1, Clue::Rank(3) | Clue::Color(ClueColor::Red()))
            ),
            "both saves violate good touch, but the critical r3 must be kept"
        );
    }

    #[test]
    fn focus_skips_old_touches_and_preserves_the_saved_five() {
        let mut table = Table::new(["y3 y4 g3 g4", "r5 r1 b3 b4", "p3 p4 b2 g2", "r3 r4 y2 p2"]);
        table.clue(0, 1, Clue::Rank(5));
        let score = table.views[0]
            .clone()
            .clue(1, Clue::Color(ClueColor::Red()))
            .unwrap();
        assert!(!score.has_errors(), "{score:?}");
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 0, |slot| {
            assert!(!slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![card("r5")]);
        });
        table.assert_cards(1, 1, |slot| {
            assert!(slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![card("r1")]);
        });
    }

    #[test]
    fn chop_takes_focus_before_leftmost_new_card() {
        let mut table = Table::new(["y3 y4 g3 g4", "r3 r4 b3 r1", "p3 p4 b2 g2", "y2 b4 g1 p2"]);
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 3, |slot| {
            assert!(slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![card("r1")]);
        });
        table.assert_cards(1, 0, |slot| assert!(!slot.play));
    }

    #[test]
    fn retouch_focus_is_leftmost_even_when_the_hand_is_locked() {
        let mut table = Table::new(["y3 y4 g3 g4", "r3 r4 b3 r1", "p3 p4 b2 g2", "y2 b4 g1 p2"]);
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        let touched = PositionSet::create(4, 0b1011);
        for (observer, view) in table.views.iter_mut().enumerate() {
            let receiver = ((5 - observer) % 4) as u8;
            assert_eq!(view.clue_focus(receiver, touched), Some(0));
            view.hands.slot_mut(receiver, 2).clued = true;
            assert_eq!(view.chop(receiver), None);
            assert_eq!(view.clue_focus(receiver, touched), Some(0));
        }
    }

    #[test]
    fn finessed_cards_are_old_touches() {
        let mut table = Table::new(["y3 y4 g3 g4", "r1 r3 b3 y2", "r2 p3 p4 b4", "b2 g2 p2 y3"]);
        table.clue(0, 2, Clue::Rank(2));
        table.assert_cards(1, 0, |slot| assert!(slot.promised.is_some()));
        table.clue(3, 1, Clue::Color(ClueColor::Red()));
        for (observer, view) in table.views.iter().enumerate() {
            let receiver = ((5 - observer) % 4) as u8;
            let focus = view.hands.slot(receiver, 1).id;
            assert_eq!(view.interpretations.records.last().unwrap().focus, focus);
        }
        table.assert_cards(1, 0, |slot| {
            assert_eq!(slot.quantum.to_vec(), vec![card("r1")])
        });
        table.assert_cards(1, 1, |slot| {
            assert!(!slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
        });
    }

    #[test]
    fn double_chop_twos_can_be_saved_from_every_view() {
        let mut table = Table::new(["g3 g4 b3 b4", "y3 y4 r3 r2", "p3 p4 g2 r2", "p5 b2 p2 y3"]);
        let score = table.views[0].clone().clue(1, Clue::Rank(2)).unwrap();
        assert!(!score.has_errors(), "{score:?}");
        table.clue(0, 1, Clue::Rank(2));
        table.assert_cards(1, 3, |slot| {
            assert!(slot.quantum.contains(&card("r2")));
            assert!(!slot.play);
        });
        // Once one copy is saved, the exception no longer applies.
        assert!(
            table.views[0]
                .clone()
                .clue(2, Clue::Rank(2))
                .unwrap()
                .has_errors()
        );
        table.clue(0, 2, Clue::Rank(2));
        assert!(
            !table.views[2]
                .hands
                .slot(0, 3)
                .quantum
                .contains(&card("r2"))
        );
    }

    #[test]
    fn off_chop_visible_two_is_excluded_from_a_save() {
        let mut table = Table::new(["g3 g4 b3 b4", "y3 y4 r3 b2", "r2 p3 p4 g2", "p5 y2 p2 y3"]);
        table.clue(0, 1, Clue::Rank(2));
        for observer in [0, 1, 3] {
            assert!(
                !table.views[observer]
                    .hands
                    .slot(((5 - observer) % 4) as u8, 3)
                    .quantum
                    .contains(&card("r2"))
            );
        }
        assert!(
            table.views[2]
                .hands
                .slot(3, 3)
                .quantum
                .contains(&card("r2")),
            "the duplicate is hidden from its holder"
        );
    }

    #[test]
    fn a_duplicate_in_the_givers_hand_does_not_invalidate_a_save() {
        let mut table = Table::new(["r2 g4 b3 b4", "y3 y4 r3 r2", "p3 p4 g2 b2", "p5 y2 p2 y3"]);
        table.clue(0, 1, Clue::Rank(2));
        table.assert_cards(1, 3, |slot| assert!(slot.quantum.contains(&card("r2"))));
    }

    #[test]
    fn early_game_saves_double_chop_twos_before_a_five_stall() {
        let table = Table::new(["g3 g4 b3 b4", "y3 y4 r3 r2", "p3 p4 g2 r2", "p5 b2 p2 y3"]);
        let status = GameStatus {
            clues: 8,
            turn: 0,
            score: 0,
            max_score: 25,
            num_strikes: 0,
            blind_plays: 0,
        };
        assert!(matches!(
            search::choose_action(&table.views[0], &status, false),
            Move::Clue(1 | 2, Clue::Rank(2))
        ));
    }

    #[test]
    fn ambiguous_critical_save_keeps_the_playable_identity() {
        let mut table = Table::new(["g3 g4 b3 b4", "y3 y4 r4 r3", "p3 p4 g2 b2", "p5 y2 p2 y3"]);
        for view in &mut table.views {
            view.card_states.played(&card("r1"));
            view.card_states.discarded(&card("r3"));
        }
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 3, |slot| {
            assert_eq!(slot.quantum.to_vec(), vec![card("r2"), card("r3")]);
            assert!(
                !slot.play,
                "a possible critical save must not be blind-played"
            );
        });
    }

    #[test]
    fn good_touch_applies_to_every_new_card_and_keeps_literal_information() {
        let mut table = Table::new(["y3 y4 g3 g4", "r1 r3 r4 b5", "p3 p4 b2 g2", "y2 b4 g1 p2"]);
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 1, |slot| {
            assert!(!slot.quantum.contains(&card("r1")));
            assert!(slot.quantum.contains_hard(&card("r1")));
        });
        table.assert_cards(1, 3, |slot| {
            assert!(!slot.quantum.contains_hard(&card("r5")))
        });
        table.play(1, 0);
        table.assert_cards(1, 0, |slot| assert!(!slot.quantum.contains(&card("r1"))));
    }

    #[test]
    fn dead_cards_are_removed_from_good_touch_inferences() {
        let mut table = Table::new(["y3 y4 g3 g4", "r1 r2 b3 b5", "p3 p4 g2 b2", "y2 b4 g1 p2"]);
        for view in &mut table.views {
            view.card_states.discarded(&card("r3"));
            view.card_states.discarded(&card("r3"));
        }
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 1, |slot| {
            assert_eq!(slot.quantum.to_vec(), vec![card("r2")]);
            assert!(slot.quantum.contains_hard(&card("r4")));
        });
    }

    #[test]
    fn prompts_use_the_leftmost_matching_clued_card_before_a_finesse() {
        let mut table = Table::new(["y3 y4 g3 g4", "r1 r2 b2 y2", "r3 p3 p4 b4", "b3 g2 p2 y1"]);
        table.clue(0, 1, Clue::Rank(2));
        table
            .views
            .iter_mut()
            .for_each(|view| view.card_states.played(&card("r1")));
        table.clue(0, 2, Clue::Color(ClueColor::Red()));
        table.assert_cards(1, 1, |slot| {
            assert!(slot.play);
            assert_eq!(slot.quantum.to_vec(), vec![card("r2")]);
        });
        assert!(table.views[1].interpretations.records.iter().any(|record| {
            record.hypotheses.iter().any(|h| {
                h.steps.iter().any(|s| {
                    s.kind == StepKind::Prompt && s.slot == table.views[1].hands.slot(0, 1).id
                })
            })
        }));
    }
}
