use hanabi::{
    self,
    game::{self, ClueColor, Game},
    hyphenated::{self, HyphenatedPlayer},
    PositionSet,
};

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

fn replay_game(
    turn: u8,
    deck: &str,
    actions: &str,
    options: &str,
) -> (hyphenated::Line, game::Game) {
    let num_players = deck
        .chars()
        .nth(0)
        .expect("deck should be non-empty")
        .to_digit(10)
        .expect("player count must be a number") as u8;
    let mut h1 = HyphenatedPlayer::new(false);
    let mut h2 = HyphenatedPlayer::new(false);
    let mut h3 = HyphenatedPlayer::new(false);
    let mut h4 = HyphenatedPlayer::new(false);
    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut h1);
    players.push(&mut h2);
    players.push(&mut h3);
    players.push(&mut h4);

    let game = Game::from_replay(turn, deck, actions, options, &mut players);

    let target_player = (turn) % num_players;
    println!("target player: {target_player}");
    let line = match target_player {
        0 => h1.line(),
        1 => h2.line(),
        2 => h3.line(),
        _ => h4.line(),
    };
    (line, game)
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

    let mut color_line = line.clone();
    let color_safe = color_line.clue(1, game::Clue::Color(suits[3].clue_color()), &game);
    assert_ne!(color_safe, None);
    let mut rank_line = line.clone();
    let rank_safe = rank_line.clue(1, game::Clue::Rank(5), &game);
    assert_ne!(rank_safe, None);
    println!("Rank\n line: {:?}\n score: {:?}", rank_line, rank_safe);
    println!("color\n line: {:?}\n score: {:?}", color_line, color_safe);
    assert!(rank_safe > color_safe);
}

#[test]
fn dont_bad_touch_same_card1() {
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

    let mut rank_line = line.clone();
    let rank_clue = rank_line.clue(1, game::Clue::Rank(1), &game);
    assert_ne!(rank_clue, None);
    let mut color_line = line.clone();
    let color_clue = color_line.clue(1, game::Clue::Color(suits[3].clue_color()), &game);
    assert_ne!(color_clue, None);
    println!("Rank\n line: {:?}\n score: {:?}", rank_line, rank_clue);
    println!("color\n line: {:?}\n score: {:?}", color_line, color_clue);
    assert!(color_clue > rank_clue);
}

#[test]
fn dont_bad_touch_same_card2() {
    // seed 28176
    let (line, game) = replay_game(
        13,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpocpdbjawbbia0aauaziabkbra1ua1bby0davuba56da60ab7asoaoavaaAbqaiaoaaat6bDaacaGbla0a4icaJ6baBaIDbb90dqb",
        "0"
    );

    println!("line: {:?}", line);
    let mut line_1 = line.clone();
    let clue_1 = line_1.clue(2, game::Clue::Rank(1), &game);
    println!("clue 5s:\n line: {line_1:?}\n => {clue_1:?}");
    let mut line_purple = line.clone();
    let clue_purple = line_purple.clue(2, game::Clue::Color(ClueColor::Purple()), &game);
    println!("clue purple:\n line: {line_purple:?}\n => {clue_purple:?}");
    assert!(clue_1 < clue_purple);
}

#[test]
fn multi_safes() {
    // seed 476604658501943910
    let (line, game) = replay_game(
        42,
        "415fxsufhniwcwaprcmhkaebirlpjaqldkpogbykmxvuqusgndvft",
        "05obae0damicudalarbaiapaboauDdbiucad6caxbvudbg0bapbba10danbcbqobb0bwa71bb2byaf6bobb3aD6auaaEah7boca6aHaz7b6caJakb50db47aaNaC1cajb87cbAaKatqa",
        "0"
    );

    println!("line: {:?}", line);
    let mut safe_5_line = line.clone();
    let safe_5s = safe_5_line.clue(2, game::Clue::Rank(5), &game);
    println!("clue 5s:\n line: {safe_5_line:?}\n => {safe_5s:?}");
    let mut clue_purple_line = line.clone();
    let clue_purple = clue_purple_line.clue(2, game::Clue::Color(ClueColor::Purple()), &game);
    println!("clue purple:\n line: {clue_purple_line:?}\n => {clue_purple:?}");
    assert!(safe_5s > clue_purple);
}

#[test]
fn safe_5s_midgame1() {
    // seed 28176
    let (line, game) = replay_game(
        6,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbf6bbnDdagbi6c7dbhawuabbia0abraziaiabua11d1bb2udav0ba4uda5uda6asbq0da8wd",
        "0",
    );

    println!("line: {:?}", line);
    let mut safe_5_line = line.clone();
    let safe_5s = safe_5_line.clue(3, game::Clue::Rank(5), &game);
    println!("clue 5s:\n line: {safe_5_line:?}\n => {safe_5s:?}");
    let mut clue_purple_line = line.clone();
    let clue_purple = clue_purple_line.clue(3, game::Clue::Color(ClueColor::Purple()), &game);
    println!("clue purple:\n line: {clue_purple_line:?}\n => {clue_purple:?}");
    assert!(safe_5s > clue_purple);
}

#[test]
#[ignore]
fn safe_5s_midgame2() {
    // seed 28176
    let (line, game) = replay_game(
        25,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1ua1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
        "0",
    );

    println!("line: {:?}", line);
    let mut safe_5_line = line.clone();
    let safe_5s = safe_5_line.clue(3, game::Clue::Rank(5), &game);
    println!("clue 5s:\n line: {safe_5_line:?}\n => {safe_5s:?}");
    let mut clue_green_line = line.clone();
    let clue_green = clue_green_line.clue(3, game::Clue::Color(ClueColor::Green()), &game);
    println!("clue purple:\n line: {clue_green_line:?}\n => {clue_green:?}");
    assert!(safe_5s > clue_green);
}

#[test]
fn maximize_knowledge_transfer1() {
    // seed 28176
    let (line, game) = replay_game(
        34,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1ua1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
        "0",
    );

    println!("start line: {:?}\n===", line);
    let mut line_clue_2 = line.clone();
    let clue_2 = line_clue_2.clue(2, game::Clue::Rank(2), &game);
    println!("clue 2s:\n line: {line_clue_2:?}\n => {clue_2:?}");
    let mut clue_blue_line = line.clone();
    let clue_blue = clue_blue_line.clue(2, game::Clue::Color(ClueColor::Blue()), &game);
    println!("clue blue:\n line: {clue_blue_line:?}\n => {clue_blue:?}");
    assert!(clue_2 > clue_blue);
}
