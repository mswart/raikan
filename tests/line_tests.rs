use hanabi::{self, game, hyphenated, PositionSet};

mod tester;

macro_rules! card {
    ($i:ident $player:literal: r $r:literal) => {
        $i.drawn(
            $player,
            game::Card {
                suit: game::Suit::Red(),
                rank: $r,
            },
        );
    };
    ($i:ident $player:literal: y $r:literal) => {
        $i.drawn(
            $player,
            game::Card {
                suit: game::Suit::Yellow(),
                rank: $r,
            },
        );
    };
    ($i:ident $player:literal: b $r:literal) => {
        $i.drawn(
            $player,
            game::Card {
                suit: game::Suit::Blue(),
                rank: $r,
            },
        );
    };
    ($i:ident $player:literal: g $r:literal) => {
        $i.drawn(
            $player,
            game::Card {
                suit: game::Suit::Green(),
                rank: $r,
            },
        );
    };
}

macro_rules! hand {
    ($i:ident $player:literal: [$c:ident $r:literal]) => {
        card! {$i $player: $c $r};
    };
    ($i:ident $player:literal: [$c:ident $r:literal, $($tail:tt)*]) => {
        hand! {$i $player: [$($tail)*]};
        card! {$i $player: $c $r};
    };
}

#[test]
fn safe_5s() {
    let mut line = hyphenated::Line::new(4);
    let game = game::Game::empty(4);
    let suits = game.suits.clone();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [r 3, r 4, r 4, b 5]);
    hand!(line 2: [y 3, y 3, y 4, y 4]);
    hand!(line 3: [g 4, g 4, g 3, g 3]);

    println!("line: {:?}", line);

    let color_safe = line
        .clone()
        .clue(1, game::Clue::Color(suits[3].clue_color()), &game);
    assert_eq!(color_safe, None);
    let rank_safe = line.clone().clue(1, game::Clue::Rank(5), &game);
    assert_ne!(rank_safe, None);
}

#[test]
fn dont_bad_touch_same_card() {
    let mut line = hyphenated::Line::new(4);
    let game = game::Game::empty(4);
    let suits = game.suits.clone();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [r 3, r 4, y 1, b 1]);
    hand!(line 2: [y 3, y 3, y 4, y 4]);
    hand!(line 3: [g 4, g 4, y 1, r 1]);
    line.clued(
        2,
        3,
        game::Clue::Rank(1),
        PositionSet::create(4, 0b1100),
        PositionSet::new(4),
        &game,
    );

    println!("clued: {:?}", PositionSet::create(4, 0b1100));
    println!("line: {:?}", line);

    let rank_clue = line.clone().clue(1, game::Clue::Rank(1), &game);
    assert_eq!(rank_clue, None);
    let color_clue = line
        .clone()
        .clue(1, game::Clue::Color(suits[3].clue_color()), &game);
    assert_ne!(color_clue, None);
}
