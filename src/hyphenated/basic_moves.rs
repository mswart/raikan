//! Level-2 examples exercised through independent player views.

use super::{Line, Slot, search};
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

    fn discard(&mut self, player: usize) {
        let n = self.views.len();
        let pos = self.hands[player].len() - 1;
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

#[test]
fn double_triple_and_quadruple_prompts_and_finesses() {
    for count in 2..=4 {
        for prompts in 0..=count {
            let connecting = (1..=5)
                .map(|rank| {
                    if rank <= count {
                        format!("r{rank}")
                    } else {
                        "y4".to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let focus = format!("r{} g3 g4 b4 p3", count + 1);
            let mut table = Table::new(&["y3 y4 b3 b4 p4", &connecting, &focus]);
            for pos in 0..prompts {
                table.literal(1, pos, Clue::Color(ClueColor::Red()));
            }
            let score = table.views[0]
                .clone()
                .clue(2, Clue::Color(ClueColor::Red()))
                .unwrap();
            assert!(
                !score.has_errors(),
                "valid {count}-link chain with {prompts} prompts: {score:?}"
            );
            table.clue(0, 2, Clue::Color(ClueColor::Red()));
            for rank in 1..=count {
                let pos = rank - 1;
                assert_eq!(
                    table.views[1].play(),
                    Some(Move::Play(pos)),
                    "{count} links, {prompts} prompts, rank {rank}"
                );
                assert_eq!(
                    table.views[2].play(),
                    None,
                    "{count} links, {prompts} prompts, rank {rank}: {:?}",
                    table.views[2]
                );
                table.play(1, pos as usize, ["y5", "b5", "g5", "p5"][pos as usize]);
            }
            assert_eq!(
                table.views[2].play(),
                Some(Move::Play(0)),
                "{count} links, {prompts} prompts: {:?}",
                table.views[2]
            );
            assert_eq!(
                table.views[2].hands.slot(0, 0).quantum.to_vec(),
                vec![Card {
                    suit: Suit::Red(),
                    rank: count + 1
                }]
            );
        }
    }
}

#[test]
fn double_finesse_across_two_players() {
    let mut table = Table::new(&["y3 y4 b3 b4", "r1 g3 g4 b4", "r2 y3 b3 p4", "r3 g4 b4 p3"]);
    table.clue(0, 3, Clue::Color(ClueColor::Red()));
    assert_eq!(table.views[1].play(), Some(Move::Play(0)));
    assert_eq!(table.views[2].play(), None);
    table.play(1, 0, "y5");
    assert_eq!(
        table.views[2].play(),
        Some(Move::Play(0)),
        "{:?}",
        table.views[2]
    );
    table.play(2, 0, "b5");
    assert_eq!(table.views[3].play(), Some(Move::Play(0)));
}

#[test]
fn reverse_finesse_waits_then_confirms_or_retracts() {
    for connects in [false, true] {
        let mut table = Table::new(&["y3 y4 b3 b4 p4", "r2 g3 g4 b4 p3", "r1 y3 b3 p4 g4"]);
        table.clue(0, 1, Clue::Color(ClueColor::Red()));
        assert_eq!(table.views[1].play(), None);
        table.discard(1);
        if connects {
            table.play(2, 0, "b5");
        } else {
            table.discard(2);
        }
        assert_eq!(table.views[1].play(), Some(Move::Play(0)));
        assert_eq!(
            table.views[1].hands.slot(0, 0).quantum.to_vec(),
            vec![card(if connects { "r2" } else { "r1" })]
        );
    }
}

#[test]
fn self_finesse_waits_for_other_players_then_connects() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "y4 g4 g4 b4 p4", "r2 r3 b4 p4 g4"]);
    table.stacks(1);
    table.clue(0, 2, Clue::Rank(3));
    table.discard(1);
    assert_eq!(table.views[2].play(), Some(Move::Play(0)));
    table.play(2, 0, "b5");
    assert_eq!(table.views[2].play(), Some(Move::Play(1)));
    assert_eq!(
        table.views[2].hands.slot(0, 1).quantum.to_vec(),
        vec![card("r3")]
    );
}

#[test]
fn wrong_prompt_recovers_original_finesse_after_a_draw() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "r2 g4 b4 r4 p4", "r3 y4 b4 p4 g4"]);
    table.stacks(1);
    table.literal(1, 3, Clue::Color(ClueColor::Red()));
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    assert_eq!(table.views[1].play(), Some(Move::Play(3)));
    table.play(1, 3, "b5");
    assert_eq!(table.views[2].play(), None, "{:?}", table.views[2]);
    assert_eq!(
        table.views[1].play(),
        Some(Move::Play(1)),
        "{:?}",
        table.views[1]
    );
    table.play(1, 1, "y5");
    assert_eq!(table.views[2].play(), Some(Move::Play(0)));
}

#[test]
fn wrong_prompt_recovers_a_multi_step_chain() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "r2 r3 b4 r5 p4", "r4 y4 b4 p4 g4"]);
    table.stacks(1);
    table.literal(1, 3, Clue::Color(ClueColor::Red()));
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    assert_eq!(table.views[1].play(), Some(Move::Play(3)));
    table.play(1, 3, "b5");
    assert_eq!(
        table.views[1].play(),
        Some(Move::Play(1)),
        "{:?}",
        table.views[1]
    );
    table.play(1, 1, "y5");
    assert_eq!(table.views[1].play(), Some(Move::Play(2)));
    assert_eq!(table.views[2].play(), None, "{:?}", table.views[2]);
    table.play(1, 2, "g5");
    assert_eq!(table.views[2].play(), Some(Move::Play(0)));
}

#[test]
fn strike_retracts_chain_and_subsequent_notes_but_keeps_literal_clues() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "r4 g4 b4 p4 y4", "r2 y3 b4 p4 g4"]);
    // The receiver cannot see a valid connection; the giver's mistaken clue
    // nevertheless makes the intervening player blind-play r1.
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    table.clue(2, 0, Clue::Color(ClueColor::Yellow()));
    table.play(1, 0, "b5");
    for (player, pos) in [(2, 0), (0, 0)] {
        for slot in table.slots(player, pos) {
            assert!(
                slot.clued && !slot.play && slot.promised.is_none(),
                "player {player}: {slot:?}"
            );
        }
        let slot = table.views[player].hands.slot(0, pos);
        assert!(
            slot.quantum
                .contains_hard(&table.hands[player][pos as usize])
        );
        assert!(slot.quantum.size() > 1);
    }
}

#[test]
fn early_five_stall_does_not_request_a_chain() {
    let mut table = Table::new(&["y3 y4 b3 b4 p4", "r5 g3 g4 b4 p3", "r1 r2 r3 r4 g4"]);
    table.clue(0, 1, Clue::Rank(5));
    assert!(
        table
            .slots(1, 0)
            .all(|s| s.clued && !s.play && s.delayed == 0)
    );
    assert!(table.views.iter().all(|v| v.interpretations.is_empty()));
}

#[test]
fn action_selection_does_not_stomp_an_existing_finesse() {
    let mut table = Table::new(&["y3 y4 b3 b4 p4", "r1 g3 g4 b4 p3", "r2 y3 b3 p4 g4"]);
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    let status = GameStatus {
        clues: 7,
        turn: 1,
        score: 0,
        max_score: 25,
        num_strikes: 0,
        blind_plays: 0,
    };
    let action = search::choose_action(&table.views[0], &status, false);
    assert!(!matches!(
        action,
        Move::Clue(1, Clue::Color(ClueColor::Red()) | Clue::Rank(1))
    ));
}

#[test]
fn reverse_prompt_nonplay_releases_the_direct_interpretation() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "b3 g4 g4 b4 p4", "r2 y4 b4 p4 g4"]);
    table.stacks(1);
    for view in &mut table.views {
        view.card_states.played(&card("b2"));
    }
    table.literal(2, 0, Clue::Color(ClueColor::Red()));
    table.clue(0, 1, Clue::Rank(3));
    assert_eq!(table.views[1].play(), None);
    table.discard(1);
    table.discard(2);
    assert_eq!(table.views[1].play(), Some(Move::Play(0)));
}

#[test]
fn successful_prompt_confirms_its_suit_over_a_direct_play() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "r3 g4 g4 b4 p4", "r2 y4 b4 p4 g4"]);
    table.stacks(1);
    for view in &mut table.views {
        view.card_states.played(&card("b2"));
    }
    table.literal(2, 0, Clue::Color(ClueColor::Red()));
    table.clue(0, 1, Clue::Rank(3));
    assert_eq!(table.views[1].play(), None);
    table.play(2, 0, "y5");
    assert_eq!(
        table.views[1].hands.slot(0, 0).quantum.to_vec(),
        vec![card("r3")]
    );
    assert_eq!(table.views[1].play(), Some(Move::Play(0)));
}

#[test]
fn basic_connections_work_in_every_seat_and_player_count() {
    for players in 2..=5 {
        for giver in 0..players {
            let receiver = (giver + 1) % players;
            let connector = (giver + 2) % players;
            let mut hands = vec!["y4 g4 b4 p4"; players];
            if players == 2 {
                hands[receiver] = "r2 r3 b4 p4";
            } else {
                hands[receiver] = "r3 g4 b4 p4";
                hands[connector] = "r2 g4 b4 p4";
            }
            let mut table = Table::new(&hands);
            table.stacks(1);
            table.clue(giver, receiver, Clue::Rank(3));
            let player = if players == 2 { receiver } else { connector };
            assert_eq!(
                table.views[player].play(),
                Some(Move::Play(0)),
                "{players} players, giver {giver}"
            );
            table.play(player, 0, "y5");
            assert_eq!(
                table.views[receiver].play(),
                Some(Move::Play(if players == 2 { 1 } else { 0 }))
            );
        }
    }
}

#[test]
fn declining_second_finesse_confirms_the_shorter_chain() {
    let mut table = Table::new(&["y3 y4 b3 b4 p4", "r1 r2 g4 b4 p3", "r2 y3 b3 p4 g4"]);
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    table.play(1, 0, "y5");
    assert_eq!(table.views[2].play(), None);
    table.discard(2);
    table.discard(0);
    table.discard(1);
    assert_eq!(table.views[2].play(), Some(Move::Play(0)));
    assert_eq!(
        table.views[2].hands.slot(0, 0).quantum.to_vec(),
        vec![card("r2")]
    );
}

#[test]
fn unrelated_play_declines_a_reverse_finesse() {
    let mut table = Table::new(&["y3 y4 b3 b4 p4", "r1 g3 g4 b4 p3", "r1 y3 b3 p4 g1"]);
    table.literal(2, 4, Clue::Rank(1));
    table.clue(0, 1, Clue::Color(ClueColor::Red()));
    assert_eq!(table.views[1].play(), None);
    table.play(2, 4, "b5");
    assert_eq!(table.views[1].play(), Some(Move::Play(0)));
    assert_eq!(
        table.views[1].hands.slot(0, 0).quantum.to_vec(),
        vec![card("r1")]
    );
}

#[test]
fn ambiguous_reverse_connections_resolve_after_one_player_declines() {
    let mut table = Table::new(&["y3 y4 b3 b4", "r2 g3 g4 b4", "r1 y3 b3 p4", "r1 g4 b4 p3"]);
    table.clue(0, 1, Clue::Color(ClueColor::Red()));
    assert_eq!(table.views[1].play(), None);
    table.discard(1);
    table.discard(2);
    assert_eq!(
        table.views[3].play(),
        Some(Move::Play(0)),
        "{:?}",
        table.views[3]
    );
    table.play(3, 0, "y5");
    assert_eq!(table.views[1].play(), Some(Move::Play(0)));
    assert_eq!(
        table.views[1].hands.slot(0, 0).quantum.to_vec(),
        vec![card("r2")]
    );
}

#[test]
fn wrong_prompt_cannot_use_a_new_draw_or_contradict_a_literal_clue() {
    for removed in [false, true] {
        let mut table = Table::new(&["y4 y4 b4 b4 p4", "r2 g4 b4 r4 p4", "r3 y4 b4 p4 g4"]);
        table.stacks(1);
        table.literal(1, 3, Clue::Color(ClueColor::Red()));
        table.clue(0, 2, Clue::Color(ClueColor::Red()));
        if removed {
            // The original finesse card left the hand before the bad prompt.
            table.play(1, 0, "b5");
        } else {
            // A negative literal clue makes the old finesse position impossible.
            for (observer, view) in table.views.iter_mut().enumerate() {
                view.hands
                    .slot_mut(((4 - observer) % 3) as u8, 0)
                    .apply_clue(Clue::Rank(2), false);
            }
        }
        table.play(1, 3, "y5");
        assert_eq!(table.views[1].play(), None);
        assert!(
            table.views[1]
                .hands
                .iter_hand(0)
                .all(|(_, s)| s.promised.is_none())
        );
    }
}

#[test]
fn strike_preserves_earlier_saves_and_positive_and_negative_clues() {
    let mut table = Table::new(&["y4 y4 b4 b4 p4", "r4 g4 b4 p4 y4", "r2 y3 b4 p4 g5"]);
    table.clue(0, 2, Clue::Rank(5));
    table.clue(0, 2, Clue::Color(ClueColor::Red()));
    table.play(1, 0, "b5");
    assert!(
        table
            .slots(2, 4)
            .all(|s| s.clued && s.quantum.is_rank(5) && !s.play)
    );
    assert!(table.slots(2, 0).all(|s| s.clued
        && !s.quantum.contains_hard(&card("r5"))
        && !s.quantum.contains_hard(&card("b2"))));
}

#[test]
fn one_away_card_without_a_connection_is_not_a_play_clue() {
    let mut table = Table::new(&["y3 y4 b3 b4 p4", "r3 g3 g4 b4 p3", "r2 y3 b3 p4 g4"]);
    table.literal(2, 0, Clue::Rank(2));
    let score = table.views[0]
        .clue(1, Clue::Color(ClueColor::Red()))
        .unwrap();
    assert!(score.has_errors(), "the missing r1 cannot be invented");
}
