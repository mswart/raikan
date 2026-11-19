use raikan::{
    PositionSet,
    game::{Card, Clue, GameStatus, Move, PlayerStrategy, Suit},
    hyphenated::HyphenatedPlayer,
};

fn bot2_after_opening_clues() -> HyphenatedPlayer {
    use Suit::*;
    // The first two visible hands from the logged three-player game, in deal
    // order. Bot2's own cards stay hidden, just as they do in the web client.
    let mut bot = HyphenatedPlayer::new(true);
    bot.init(3, 2);
    for (player, hand) in [
        [
            (Yellow(), 5),
            (Green(), 1),
            (Green(), 3),
            (Green(), 1),
            (Blue(), 1),
        ],
        [
            (Blue(), 2),
            (Purple(), 2),
            (Green(), 3),
            (Blue(), 5),
            (Red(), 3),
        ],
    ]
    .iter()
    .enumerate()
    {
        for &(suit, rank) in hand {
            bot.drawn(player + 1, Card { suit, rank });
        }
    }
    for _ in 0..5 {
        bot.own_drawn();
    }
    let mut touched = PositionSet::new(5);
    touched.add(1);
    bot.clued(1, 2, Clue::Rank(5), touched);
    let mut touched = PositionSet::new(5);
    for position in [0, 2, 3] {
        touched.add(position);
    }
    bot.clued(2, 0, Clue::Rank(2), touched);
    bot
}

#[test]
fn common_opening_play_is_known_without_resolving_the_layered_interpretation() {
    let mut bot = bot2_after_opening_clues();
    let mut line = bot.line();
    let blue_one = line.hands.slot(1, 0);
    assert!(blue_one.play, "b1 is due as either a target or a layer");
    assert!(blue_one.promised.is_some());
    assert_eq!(blue_one.delayed, 0);
    assert_eq!(
        blue_one.quantum.to_vec(),
        vec![Card {
            suit: Suit::Blue(),
            rank: 1
        }]
    );
    assert!(!line.hands.slot(1, 1).play, "g1 is only conditionally due");

    let focus = line.hands.slot(0, 0);
    assert_eq!(
        focus.quantum.to_vec(),
        vec![
            Card {
                suit: Suit::Green(),
                rank: 2
            },
            Card {
                suit: Suit::Blue(),
                rank: 2
            },
        ]
    );
    assert_eq!(line.play(), None, "bot2 must still wait for the connection");
    assert_eq!(line.interpretations.records[0].active().count(), 2);

    // Saving the five and seeing b1 played must not eliminate the valid g2
    // interpretation: g1 can still be the remaining layered connection.
    let mut touched = PositionSet::new(5);
    touched.add(4);
    bot.clued(0, 1, Clue::Rank(5), touched);
    bot.played(
        1,
        0,
        Card {
            suit: Suit::Blue(),
            rank: 1,
        },
        true,
    );
    bot.drawn(
        1,
        Card {
            suit: Suit::Yellow(),
            rank: 3,
        },
    );
    let mut line = bot.line();
    assert_eq!(line.interpretations.records[0].active().count(), 2);
    assert_eq!(line.hands.slot(0, 0).delayed, 1);
    assert_eq!(line.play(), None);
}

#[test]
fn opening_saves_the_five_instead_of_recluing_the_common_play() {
    let mut bot = bot2_after_opening_clues();
    let status = GameStatus {
        turn: 2,
        score: 0,
        max_score: 25,
        num_strikes: 0,
        clues: 6,
        blind_plays: 0,
    };
    assert_eq!(bot.act(&status), Move::Clue(1, Clue::Rank(5)));
}
