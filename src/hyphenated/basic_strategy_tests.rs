use super::super::{Line, Slot, search};
use crate::{
    PositionSet,
    game::{Card, Clue, ClueColor, GameStatus, Move, Suit},
};

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

struct Table {
    hands: Vec<Vec<Card>>,
    views: Vec<Line>,
}

impl Table {
    fn new(hands: &[&str]) -> Self {
        let hands: Vec<Vec<_>> = hands
            .iter()
            .map(|h| h.split_whitespace().map(card).collect())
            .collect();
        let n = hands.len();
        let mut views: Vec<_> = (0..n).map(|p| Line::new(n as u8, p as u8)).collect();
        for (player, hand) in hands.iter().enumerate() {
            for &card in hand.iter().rev() {
                for (observer, view) in views.iter_mut().enumerate() {
                    if player == observer {
                        view.own_drawn();
                    } else {
                        view.drawn((n + player - observer) % n, card);
                    }
                }
            }
        }
        Self { hands, views }
    }

    fn literal(&mut self, player: usize, pos: u8, clue: Clue) {
        let n = self.views.len();
        for (observer, view) in self.views.iter_mut().enumerate() {
            let slot = view
                .hands
                .slot_mut(((n + player - observer) % n) as u8, pos);
            slot.apply_clue(clue, true);
            slot.clued = true;
            if observer != player {
                view.card_states[&self.hands[player][pos as usize]].clued =
                    Some(((n + player - observer) % n) as u8);
            }
        }
    }

    fn stacks(&mut self, ranks: u8) {
        for view in &mut self.views {
            for suit in [
                Suit::Red(),
                Suit::Yellow(),
                Suit::Green(),
                Suit::Blue(),
                Suit::Purple(),
            ] {
                for rank in 1..=ranks {
                    view.card_states.played(&Card { suit, rank });
                }
            }
        }
    }

    fn clue(&mut self, giver: usize, receiver: usize, clue: Clue) {
        let n = self.views.len();
        let mut touched = PositionSet::new(self.hands[receiver].len() as u8);
        for (pos, card) in self.hands[receiver].iter().enumerate() {
            if card.affected(clue) {
                touched.add(pos as u8);
            }
        }
        for (observer, view) in self.views.iter_mut().enumerate() {
            view.clued(
                (n + giver - observer) % n,
                (n + receiver - observer) % n,
                clue,
                touched,
            );
        }
    }

    fn play(&mut self, player: usize, pos: usize, draw: &str) {
        let n = self.views.len();
        let played = self.hands[player].remove(pos);
        let successful = self.views[player]
            .card_states
            .play_quantum
            .contains(&played);
        for (observer, view) in self.views.iter_mut().enumerate() {
            let relative = (n + player - observer) % n;
            view.played(relative, pos, played, successful);
            if observer == player {
                view.own_drawn();
            } else {
                view.drawn(relative, card(draw));
            }
        }
        self.hands[player].insert(0, card(draw));
    }

    fn discard_at(&mut self, player: usize, pos: usize) {
        let n = self.views.len();
        let discarded = self.hands[player].remove(pos);
        for (observer, view) in self.views.iter_mut().enumerate() {
            view.discarded((n + player - observer) % n, pos, discarded);
        }
    }

    fn slots(&self, player: usize, pos: u8) -> impl Iterator<Item = &Slot> {
        let n = self.views.len();
        self.views
            .iter()
            .enumerate()
            .map(move |(observer, view)| view.hands.slot(((n + player - observer) % n) as u8, pos))
    }
}

fn status(clues: u8) -> GameStatus {
    GameStatus {
        clues,
        turn: 5,
        score: 0,
        max_score: 25,
        num_strikes: 0,
        blind_plays: 0,
    }
}

#[test]
fn starting_ones_order_and_focus_across_player_counts_and_rotations() {
    for players in 2..=5 {
        for receiver in 0..players {
            let filler = if players < 4 {
                "y3 y4 g3 g4 p4"
            } else {
                "y3 y4 g3 g4"
            };
            let mut hands = vec![filler; players];
            hands[receiver] = if players < 4 {
                "r1 b1 g1 p3 p4"
            } else {
                "r1 b1 g1 p3"
            };
            let mut table = Table::new(&hands);
            table.clue((receiver + players - 1) % players, receiver, Clue::Rank(1));
            assert_eq!(table.views[receiver].next_play(), Some(Move::Play(2)));
            for observer in 0..players {
                let view = &table.views[observer];
                let relative = ((players + receiver - observer) % players) as u8;
                let focus = view.hands.slot(relative, 2).id;
                assert!(
                    view.interpretations
                        .records
                        .iter()
                        .any(|r| r.focus == focus)
                );
            }
            table.play(receiver, 2, "y5");
            assert_eq!(table.views[receiver].next_play(), Some(Move::Play(2)));
            table.play(receiver, 2, "g5");
            assert_eq!(table.views[receiver].next_play(), Some(Move::Play(2)));
        }
    }
}

#[test]
fn fresh_ones_precede_starting_ones_and_are_newest_first() {
    let mut table = Table::new(&["y3 y4 g3 g4", "r1 b1 g1 p4", "p3 p4 y2 b2", "r3 r4 b3 b4"]);
    table.clue(0, 1, Clue::Rank(1));
    table.play(1, 2, "y1");
    table.clue(0, 1, Clue::Rank(1));
    assert_eq!(table.views[1].next_play(), Some(Move::Play(0)));
    table.play(1, 0, "p1");
    table.clue(0, 1, Clue::Rank(1));
    assert_eq!(table.views[1].next_play(), Some(Move::Play(0)));
    table.play(1, 0, "y5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(2)));
}

#[test]
fn chop_focus_overrides_fresh_ones_then_resumes_the_order() {
    let mut table = Table::new(&["y3 y4 g3 g4 p4", "g1 b1 y3 p1 r4", "p3 p4 y2 b2 b4"]);
    table.discard_at(1, 4);
    for (observer, view) in table.views.iter_mut().enumerate() {
        if observer == 1 {
            view.own_drawn();
        } else {
            view.drawn((4 - observer) % 3, card("y1"));
        }
    }
    table.hands[1].insert(0, card("y1"));
    table.clue(0, 1, Clue::Rank(1));
    assert_eq!(table.views[1].next_play(), Some(Move::Play(4)));
    table.play(1, 4, "r5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(1)));
    table.play(1, 1, "g5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(3)));
    table.play(1, 3, "b5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(3)));
}

#[test]
fn fix_with_new_touches_does_not_create_a_play_or_finesse() {
    let mut table = Table::new(&["b1 y4 g3 g4", "b4 b1 g3 p3", "r3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(1, 1, Clue::Rank(1));
    table.play(0, 0, "y5");
    // The previous clue incorrectly promised an unplayed 1.
    table.clue(0, 1, Clue::Color(ClueColor::Blue()));
    for slot in table.slots(1, 1) {
        assert!(slot.fixed && slot.trash && !slot.play);
    }
    for slot in table.slots(1, 0) {
        assert!(!slot.play && slot.promised.is_none());
        assert_eq!(
            slot.quantum.to_vec(),
            vec![card("b2"), card("b3"), card("b4"), card("b5")]
        );
        assert!(slot.quantum.contains_hard(&card("b1")));
    }
}

#[test]
fn a_direct_conflict_breaks_a_wrong_identity_lock() {
    let mut table = Table::new(&["y3 y4 g3 g4", "b3 p4 g3 p3", "r3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.clue(0, 1, Clue::Rank(3));
    // Construct an established rank-3 identity (still only rank known literally).
    for observer in 0..4 {
        let slot = table.views[observer]
            .hands
            .slot_mut(((5 - observer) % 4) as u8, 0);
        slot.quantum.soft_clear();
        slot.quantum.add_card(&card("r3"), true);
        slot.base_quantum = slot.quantum;
    }
    table.clue(0, 1, Clue::Color(ClueColor::Blue()));
    for slot in table.slots(1, 0) {
        assert!(slot.fixed);
        assert_eq!(slot.quantum.to_vec(), vec![card("b3")]);
    }
}

#[test]
fn stop_clue_stops_only_the_next_one_and_survives_draws() {
    let mut table = Table::new(&["r1 y4 g3 g4", "g1 r1 b1 p4", "p3 p4 y2 b2", "r3 r4 y3 y4"]);
    table.clue(0, 1, Clue::Rank(1));
    table.play(1, 2, "y5");
    table.play(0, 0, "b5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(2)));
    table.clue(0, 1, Clue::Rank(1));
    for slot in table.slots(1, 2) {
        assert!(slot.stopped && slot.trash && !slot.play);
    }
    for slot in table.slots(1, 1) {
        assert!(!slot.stopped && slot.play);
    }
    assert_eq!(table.views[1].next_play(), Some(Move::Play(1)));
    table.play(1, 1, "p5");
    for slot in table.slots(1, 2) {
        assert!(slot.stopped && slot.trash && !slot.play);
    }
}

#[test]
fn repeat_of_a_delayed_card_is_not_a_stop() {
    let mut table = Table::new(&["y3 y4 g3 g4", "r2 p4 g3 p3", "r1 r4 y2 b2", "p3 p4 y3 y4"]);
    table.clue(0, 1, Clue::Color(ClueColor::Red()));
    table.clue(0, 1, Clue::Color(ClueColor::Red()));
    for slot in table.slots(1, 0) {
        assert!(!slot.stopped);
    }
}

#[test]
fn sarcastic_discard_transfers_a_unique_identity_in_every_view() {
    let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g3 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(0, 0, Clue::Color(ClueColor::Red()));
    table.literal(1, 0, Clue::Rank(3));
    for view in &mut table.views {
        view.card_states.played(&card("r1"));
        view.card_states.played(&card("r2"));
    }
    assert_eq!(table.views[0].sarcastic_discard(), Some(Move::Discard(0)));
    assert_eq!(
        search::choose_action(&table.views[0], &status(3), false),
        Move::Discard(0)
    );
    table.discard_at(0, 0);
    for slot in table.slots(1, 0) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
        assert!(slot.play);
    }
}

#[test]
fn ambiguous_sarcasm_remembers_candidates_without_choosing_the_leftmost() {
    let mut table = Table::new(&["r3 y4 g3 g4", "b3 r3 g4 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(1, 0, Clue::Rank(3));
    table.literal(1, 1, Clue::Rank(3));
    table.discard_at(0, 0);
    for view in &table.views {
        assert_eq!(view.sarcastic[0].candidates.len(), 2);
    }
    for pos in 0..=1 {
        for slot in table.slots(1, pos) {
            assert!(slot.quantum.size() > 1);
            assert!(!slot.play);
        }
    }
    // A negative literal clue rules out the first candidate and identifies the other.
    table.clue(2, 1, Clue::Color(ClueColor::Blue()));
    for slot in table.slots(1, 1) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
        assert!(!slot.play);
    }
}

#[test]
fn ordinary_and_trash_discards_do_not_transfer_information() {
    for clued in [false, true] {
        let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g3 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
        table.literal(1, 0, Clue::Rank(3));
        if clued {
            table.literal(0, 0, Clue::Rank(3));
            table.stacks(3);
        }
        table.discard_at(0, 0);
        for view in &table.views {
            assert!(view.sarcastic.is_empty());
        }
    }
}

#[test]
fn information_locks_do_not_expand_when_stacks_advance() {
    let mut table = Table::new(&["g1 y4 g3 g4", "r2 p4 y3 p3", "y3 r4 y2 b4", "p3 p4 y3 y4"]);
    for view in &mut table.views {
        view.card_states.played(&card("r1"));
        view.card_states.played(&card("b1"));
    }
    table.clue(0, 1, Clue::Rank(2));
    let before = table.views[1].hands.slot(0, 0).quantum;
    assert_eq!(before.to_vec(), vec![card("r2"), card("b2")]);
    table.play(0, 0, "y5");
    for slot in table.slots(1, 0) {
        assert_eq!(slot.quantum.to_vec(), before.to_vec());
    }
}

#[test]
fn duplicate_play_clue_can_be_answered_with_a_sarcastic_discard() {
    let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g4 b4", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    for view in &mut table.views {
        view.card_states.played(&card("r1"));
        view.card_states.played(&card("r2"));
    }
    table.clue(0, 1, Clue::Rank(3));
    assert_eq!(table.views[1].sarcastic_discard(), Some(Move::Discard(0)));
    assert_eq!(
        search::choose_action(&table.views[1], &status(3), false),
        Move::Discard(0)
    );
    table.discard_at(1, 0);
    assert_eq!(
        table.views[0].hands.slot(0, 0).quantum.to_vec(),
        vec![card("r3")]
    );
    assert_eq!(table.views[0].next_play(), Some(Move::Play(0)));
}

#[test]
fn ambiguous_sarcasm_does_not_retarget_a_new_draw() {
    let mut table = Table::new(&["r3 y4 g3 g4", "b3 r3 g4 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(1, 0, Clue::Rank(3));
    table.literal(1, 1, Clue::Rank(3));
    table.discard_at(0, 0);
    table.stacks(2);
    table.play(1, 0, "y3");
    for slot in table.slots(1, 1) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
        assert!(slot.play);
    }
    for slot in table.slots(1, 0) {
        assert!(!slot.clued && slot.quantum.size() > 1);
    }
}

#[test]
fn failed_plays_do_not_send_sarcastic_information() {
    let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g3 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(1, 0, Clue::Rank(3));
    table.play(0, 0, "y5");
    for view in &table.views {
        assert!(view.sarcastic.is_empty());
    }
    for slot in table.slots(1, 0) {
        assert!(slot.quantum.size() > 1);
    }
}

#[test]
fn corrections_preserve_unrelated_locks_and_literal_exclusions() {
    let mut table = Table::new(&["b1 y4 g3 g4", "r1 b1 g3 p3", "r3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.clue(0, 1, Clue::Rank(1));
    table.play(0, 0, "y5");
    let before = table.views[1].hands.slot(0, 0).quantum;
    table.clue(0, 1, Clue::Color(ClueColor::Blue()));
    let after = table.views[1].hands.slot(0, 0);
    assert!(!after.fixed && after.play);
    assert!(before.superset(after.quantum));
    assert!(!after.quantum.contains_hard(&card("b1")));
    for slot in table.slots(1, 2) {
        assert!(!slot.quantum.contains_hard(&card("r1")));
    }
}

#[test]
fn a_partial_literal_fix_cancels_the_old_finesse() {
    let mut table = Table::new(&["y3 y4 g3 g4", "r3 p4 g3 p3", "r2 r4 y2 b2", "p3 p4 y3 y4"]);
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    assert!(table.views[1].hands.slot(0, 0).promised.is_some());
    // Explicitly reveal the wrong finesse's rank; the suit remains unknown.
    table.clue(0, 1, Clue::Rank(3));
    let slot = table.views[1].hands.slot(0, 0);
    assert!(slot.fixed && !slot.play && slot.promised.is_none());
    assert!(slot.quantum.size() > 1);
    assert!(slot.quantum.is_rank(3));
}

#[test]
fn efficiency_rewards_two_useful_touches_over_one() {
    let table = Table::new(&["y3 y4 g3 g4", "r1 b1 p3 p4", "g1 r4 y2 b2", "p3 p4 y3 y4"]);
    let mut two = table.views[0].clone();
    let mut one = table.views[0].clone();
    assert!(two.clue(1, Clue::Rank(1)).unwrap() > one.clue(2, Clue::Rank(1)).unwrap());
}

#[test]
fn safe_play_supplies_tempo_with_an_existing_clue_queue() {
    let mut table = Table::new(&["y1 y4 g3 g4", "r1 b1 p3 p4", "g1 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(1));
    assert_eq!(
        search::choose_action(&table.views[0], &status(3), false),
        Move::Play(0)
    );
}

#[test]
fn fix_precedes_our_play_even_before_the_wrong_card_is_playable() {
    let mut table = Table::new(&["y1 y4 g3 g4", "r4 p4 g3 b3", "y3 g4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(1));
    table.literal(1, 0, Clue::Color(ClueColor::Red()));
    for observer in 0..4 {
        let slot = table.views[observer]
            .hands
            .slot_mut(((5 - observer) % 4) as u8, 0);
        slot.quantum.soft_clear();
        slot.quantum.add_card(&card("r3"), true);
        slot.base_quantum = slot.quantum;
    }
    assert!(table.views[0].next_play().is_some());
    let Move::Clue(1, clue) = search::choose_action(&table.views[0], &status(3), false) else {
        panic!("the false inference must be fixed before our safe play");
    };
    table.clue(0, 1, clue);
    let corrected = table.views[1].hands.slot(0, 0);
    assert!(corrected.fixed && corrected.quantum.contains(&card("r4")));
    assert!(!corrected.play);
}

#[test]
fn sarcasm_never_causes_an_illegal_discard_at_eight_tokens() {
    let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g3 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(0, 0, Clue::Color(ClueColor::Red()));
    table.literal(1, 0, Clue::Rank(3));
    for view in &mut table.views {
        view.card_states.played(&card("r1"));
        view.card_states.played(&card("r2"));
    }
    assert!(!matches!(
        search::choose_action(&table.views[0], &status(8), false),
        Move::Discard(_)
    ));
}

#[test]
fn multiple_fresh_ones_play_newest_before_oldest() {
    let mut table = Table::new(&["y3 y4 g3 g4", "r1 p3 b3 g4", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    for draw in ["b1", "g1"] {
        table.discard_at(1, 3);
        for (observer, view) in table.views.iter_mut().enumerate() {
            if observer == 1 {
                view.own_drawn();
            } else {
                view.drawn((5 - observer) % 4, card(draw));
            }
        }
        table.hands[1].insert(0, card(draw));
    }
    table.clue(0, 1, Clue::Rank(1));
    assert_eq!(table.views[1].next_play(), Some(Move::Play(0)));
    table.play(1, 0, "y5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(1)));
    table.play(1, 1, "b5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(2)));
}

#[test]
fn resolved_prompt_chain_locks_every_in_between_identity() {
    let mut table = Table::new(&["y3 y4 g3 g4", "r2 p4 g3 p3", "r3 b4 y2 b2", "r4 p4 y3 y4"]);
    table.literal(1, 0, Clue::Rank(2));
    table.literal(2, 0, Clue::Rank(3));
    for view in &mut table.views {
        view.card_states.played(&card("r1"));
    }
    table.clue(0, 3, Clue::Color(ClueColor::Red()));
    for (player, identity) in [(1, "r2"), (2, "r3"), (3, "r4")] {
        for slot in table.slots(player, 0) {
            assert_eq!(slot.quantum.to_vec(), vec![card(identity)]);
        }
    }
    table.play(1, 0, "y5");
    for slot in table.slots(2, 0) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
    }
    table.play(2, 0, "b5");
    for slot in table.slots(3, 0) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r4")]);
        assert!(slot.play);
    }
}

#[test]
fn sarcasm_restores_a_good_touch_exclusion_in_an_ambiguous_card() {
    let mut table = Table::new(&["r3 y4 g3 g4", "r3 p4 g4 b4", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    table.literal(1, 0, Clue::Rank(3));
    for observer in 0..4 {
        table.views[observer]
            .hands
            .slot_mut(((5 - observer) % 4) as u8, 0)
            .remove_card(&card("r3"), true);
    }
    table.discard_at(0, 0);
    for slot in table.slots(1, 0) {
        assert_eq!(slot.quantum.to_vec(), vec![card("r3")]);
    }
}

#[test]
fn a_fresh_one_does_not_steal_a_locked_one_from_its_own_hand() {
    let mut table = Table::new(&["y3 y4 g3 g4", "p1 r1 b3 p4", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(1, 1, Clue::Rank(1));
    table.literal(1, 1, Clue::Color(ClueColor::Red()));
    for view in &mut table.views {
        for text in ["y1", "g1", "b1", "p1"] {
            view.card_states.played(&card(text));
        }
    }
    table.clue(0, 1, Clue::Rank(1));
    let receiver = &table.views[1];
    assert!(receiver.hands.slot(0, 0).trash);
    assert!(!receiver.hands.slot(0, 0).play);
    assert_eq!(receiver.next_play(), Some(Move::Play(1)));
}

#[test]
fn ambiguous_sarcasm_protects_candidates_previously_inferred_as_trash() {
    let mut table = Table::new(&["r3 y4 g3 g4", "b3 r3 g4 p3", "y3 r4 y2 b2", "p3 p4 y3 y4"]);
    table.literal(0, 0, Clue::Rank(3));
    for pos in 0..=1 {
        table.literal(1, pos, Clue::Rank(3));
        for observer in 0..4 {
            let slot = table.views[observer]
                .hands
                .slot_mut(((5 - observer) % 4) as u8, pos);
            slot.quantum.soft_clear();
            slot.base_quantum.soft_clear();
        }
    }
    table.discard_at(0, 0);
    for pos in 0..=1 {
        for slot in table.slots(1, pos) {
            assert!(!slot.trash && !slot.play);
            assert!(slot.quantum.contains(&card("r3")) && slot.quantum.size() > 1);
        }
    }
}

#[test]
fn connecting_through_unknown_ones_preserves_their_play_order() {
    let mut table = Table::new(&["y3 y4 g3 g4", "b1 r1 g4 p4", "y3 r4 y2 b3", "b2 p4 y3 y4"]);
    table.clue(0, 1, Clue::Rank(1));
    table.clue(2, 3, Clue::Color(ClueColor::Blue()));
    assert_eq!(table.views[1].next_play(), Some(Move::Play(1)));
    table.play(1, 1, "y5");
    assert_eq!(table.views[1].next_play(), Some(Move::Play(1)));
    assert!(table.views[3].next_play().is_none());
    table.play(1, 1, "g5");
    assert_eq!(table.views[3].next_play(), Some(Move::Play(0)));
}
