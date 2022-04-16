use colored::*;
use hanabi::{
    self,
    game::{self, ClueColor, Game},
    hyphenated::{self, HyphenatedPlayer, LineScore, Slot},
    PositionSet,
};

extern crate slog;
extern crate slog_term;

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

#[derive(Clone)]
struct Replay {
    pub target_player: u8,
    pub line: hyphenated::Line,
    pub lines: [hyphenated::Line; 4],
}
use slog::*;

fn replay_game(turn: u8, deck: &str, actions: &str, options: &str) -> Replay {
    let decorator = slog_term::PlainSyncDecorator::new(slog_term::TestStdoutWriter);
    let drain = slog_term::CompactFormat::new(decorator).build().fuse();
    let drain = std::sync::Mutex::new(drain).fuse();
    let log = slog::Logger::root(drain, o!());

    let num_players = deck
        .chars()
        .nth(0)
        .expect("deck should be non-empty")
        .to_digit(10)
        .expect("player count must be a number") as u8;
    let mut h1 = HyphenatedPlayer::with_logger(log.new(o!("player" => "Alice")));
    let mut h2 = HyphenatedPlayer::with_logger(log.new(o!("player" => "Bob")));
    let mut h3 = HyphenatedPlayer::with_logger(log.new(o!("player" => "Cathy")));
    let mut h4 = HyphenatedPlayer::with_logger(log.new(o!("player" => "Donald")));
    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut h1);
    players.push(&mut h2);
    players.push(&mut h3);
    players.push(&mut h4);

    Game::from_replay(turn, deck, actions, options, &mut players);

    let target_player = (turn) % num_players;
    println!("target player: {target_player}");

    let lines = [h1.line(), h2.line(), h3.line(), h4.line()];
    let line = &lines[target_player as usize];

    let replay = Replay {
        line: line.clone(),
        lines,
        target_player,
    };
    println!("{}", "Start situation".to_string().bold().underline());
    replay.print();
    replay
}

impl Replay {
    fn slot_perspectives(&self, player: u8, pos: u8) -> Vec<Slot> {
        let mut slots = Vec::new();
        for (current_player, line) in self.lines.iter().enumerate() {
            slots.push(
                line.hands
                    .slot((4 + player - current_player as u8) % 4, pos)
                    .clone(),
            );
        }
        slots
    }

    fn print(&self) {
        for player in 0..4 {
            println!("Player {player}");
            for j in 0..4 {
                println!(
                    "- P{j} {:?}",
                    self.lines[j]
                        .hands
                        .iter_hand(((4 + player - j) % 4) as u8)
                        .map(|(_pos, slot)| slot)
                        .collect::<Vec<&Slot>>()
                );
            }
        }
        println!("Clued cards:");
        for line in self.lines.iter() {
            println!(" - {:?}", line.card_states);
        }
        println!("Callbacks:");
        for line in self.lines.iter() {
            line.print_callbacks(" - ");
            // println!(" - {:?}", line.callbacks);
        }
    }

    fn clue_is_bad(&mut self, player: u8, clue: game::Clue) -> bool {
        let score = self.clue(player, clue);
        if let Some(score) = score {
            score.has_errors()
        } else {
            true
        }
    }

    fn clue(&mut self, player: u8, clue: game::Clue) -> Option<LineScore> {
        println!("");
        println!("{}", format!("Clued {player} {clue:?}").bold().underline());
        let num_players = self.lines.len() as u8;
        let clued_player = (num_players + player - self.target_player as u8) % num_players;
        let score = self.line.clue(clued_player as usize, clue);
        let mut touched = PositionSet::new(self.lines[0].hands.hand_sizes[player as usize]);

        for (pos, slot) in self.lines[self.target_player as usize]
            .hands
            .iter_hand(clued_player)
        {
            if slot.card.affected(clue) {
                touched.add(pos);
            }
        }
        for (current_player, line) in self.lines.iter_mut().enumerate() {
            line.clued(
                ((num_players + self.target_player - current_player as u8) % num_players) as usize,
                ((num_players + player - current_player as u8) % num_players) as usize,
                clue,
                touched,
            );
        }
        self.print();
        println!("Score: {score:?}");
        self.target_player = (self.target_player + 1) % self.lines.len() as u8;
        score
    }

    fn play(&mut self, pos: usize, next_card: Option<game::Card>) {
        println!("");
        let num_players = self.lines.len() as u8;
        let slot = self.lines[((self.target_player + 1) % num_players) as usize]
            .hands
            .slot(num_players - 1, pos as u8);
        let card = slot.card;
        println!(
            "{}",
            format!("Player {} plays {card:?}", self.target_player)
                .bold()
                .underline()
        );
        for (current_player, line) in self.lines.iter_mut().enumerate() {
            let rel_player =
                ((num_players + self.target_player - current_player as u8) % num_players) as usize;
            line.played(
                rel_player,
                pos,
                card,
                matches!(
                    self.line.card_states[&card].play,
                    game::CardPlayState::Playable() | game::CardPlayState::CriticalPlayable()
                ),
            );
            if let Some(card) = next_card {
                if rel_player == 0 {
                    line.own_drawn();
                } else {
                    line.drawn(rel_player, card)
                }
            }
        }
        self.target_player += 1;
        self.print();
    }
}

fn clue(line: &hyphenated::Line, player: usize, clue: game::Clue) -> hyphenated::LineScore {
    let mut clue_line = line.clone();
    let clued = clue_line.clue(player, clue);
    println!("clue {clue:?}:\n line: {clue_line:?}\n => {clued:?}");
    clued.expect("clue should succeed")
}

#[test]
fn safe_5s() {
    let mut line = hyphenated::Line::new(4, 0);
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
    let color_safe = color_line.clue(1, game::Clue::Color(suits[3].clue_color()));
    assert_ne!(color_safe, None);
    let mut rank_line = line.clone();
    let rank_safe = rank_line.clue(1, game::Clue::Rank(5));
    assert_ne!(rank_safe, None);
    println!("Rank\n line: {:?}\n score: {:?}", rank_line, rank_safe);
    println!("color\n line: {:?}\n score: {:?}", color_line, color_safe);
    assert!(rank_safe > color_safe);
}

#[test]
fn track_cards() {
    let mut line = hyphenated::Line::new(4, 0);
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [r 3, r 1, g 4, b 5]);
    hand!(line 2: [y 3, y 1, y 4, y 4]);
    hand!(line 3: [g 4, r 1, g 3, g 1]);

    println!("line: {:?}", line);

    // everybody except bob can exclude b5:
    let b5 = game::Card {
        rank: 5,
        suit: game::Suit::Blue(),
    };
    for player in 0..line.hands.num_players {
        for (_pos, slot) in line.hands.iter_hand(player) {
            if player == 1 {
                assert!(
                    slot.quantum.contains(&b5),
                    "Player 1 does not see b5 and may have it: {:?} / {}",
                    slot.card,
                    slot.quantum
                );
            } else {
                assert!(
                    !slot.quantum.contains(&b5),
                    "Player {} sees b5 elsewhere and does not have it: {:?} / {}",
                    player,
                    slot.card,
                    slot.quantum
                );
            }
        }
    }

    // everybody except bob & donald can exclude g4:
    let g4 = game::Card {
        rank: 4,
        suit: game::Suit::Green(),
    };
    for player in 0..line.hands.num_players {
        for (_pos, slot) in line.hands.iter_hand(player) {
            if player == 1 || player == 3 {
                assert!(
                    slot.quantum.contains(&g4),
                    "Player 1 does not see g4 and may have it: {:?} / {}",
                    slot.card,
                    slot.quantum
                );
            } else {
                assert!(
                    !slot.quantum.contains(&g4),
                    "Player {} sees both g4 elsewhere and does not have it: {:?} / {}",
                    player,
                    slot.card,
                    slot.quantum
                );
            }
        }
    }

    // let bob discard the g4
    line.discarded(1, 2, g4);
    println!("line (after discard g4): {:?}", line);

    for player in 0..line.hands.num_players {
        for (_pos, slot) in line.hands.iter_hand(player) {
            if player == 3 {
                assert!(
                    slot.quantum.contains(&g4),
                    "Player 3 does not see g4 and may have it: {:?} / {}",
                    slot.card,
                    slot.quantum
                );
            } else {
                assert!(
                    !slot.quantum.contains(&g4),
                    "Player {} sees both g4 elsewhere and does not have it: {:?} / {}",
                    player,
                    slot.card,
                    slot.quantum
                );
            }
        }
    }
}

#[test]
fn dont_bad_touch_same_card1() {
    let mut line = hyphenated::Line::new(4, 0);
    let game = game::Game::empty(4);
    let suits = game.suits.clone();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [r 3, r 4, y 1, b 1]);
    hand!(line 2: [y 3, y 3, y 4, y 4]);
    hand!(line 3: [g 4, g 4, y 1, r 1]);
    line.clued(2, 3, game::Clue::Rank(1), PositionSet::create(4, 0b1100));

    println!("clued: {:?}", PositionSet::create(4, 0b1100));
    println!("line: {:?}", line);

    let mut rank_line = line.clone();
    let rank_clue = rank_line.clue(1, game::Clue::Rank(1));
    assert_ne!(rank_clue, None);
    let mut color_line = line.clone();
    let color_clue = color_line.clue(1, game::Clue::Color(suits[3].clue_color()));
    assert_ne!(color_clue, None);
    println!("Rank\n line: {:?}\n score: {:?}", rank_line, rank_clue);
    println!("color\n line: {:?}\n score: {:?}", color_line, color_clue);
    assert!(color_clue > rank_clue);
}

#[test]
fn dont_bad_touch_same_card2() {
    // seed 28176
    let line = replay_game(
        13,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpocpdbjawbbia0aauaziabkbra1ua1bby0davuba56da60ab7asoaoavaaAbqaiaoaaat6bDaacaGbla0a4icaJ6baBaIDbb90dqb",
        "0"
    ).line;

    assert!(
        clue(&line, 2, game::Clue::Rank(1))
            < clue(&line, 2, game::Clue::Color(ClueColor::Purple()))
    );
}

#[test]
fn multi_safes() {
    // seed 476604658501943910
    let line = replay_game(
        42,
        "415fxsufhniwcwaprcmhkaebirlpjaqldkpogbykmxvuqusgndvft",
        "05obae0damicudalarbaiapaboauDdbiucad6caxbvudbg0bapbba10danbcbqobb0bwa71bb2byaf6bobb3aD6auaaEah7boca6aHaz7b6caJakb50db47aaNaC1cajb87cbAaKatqa",
        "0"
    ).line;

    assert!(
        clue(&line, 2, game::Clue::Rank(5))
            > clue(&line, 2, game::Clue::Color(ClueColor::Purple()))
    );
}

#[test]
fn safe_5s_midgame1() {
    // seed 28176
    let line = replay_game(
        6,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbf6bbnDdagbi6c7dbhawuabbia0abraziaiabua11d1bb2udav0ba4uda5uda6asbq0da8wd",
        "0",
    )
    .line;

    assert!(
        clue(&line, 3, game::Clue::Rank(5))
            > clue(&line, 3, game::Clue::Color(ClueColor::Purple()))
    );
}

#[test]
fn safe_5s_midgame2() {
    // seed 28176
    let line = replay_game(
        25,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1ua1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
        "0",
    ).line;

    assert!(
        clue(&line, 3, game::Clue::Rank(5)) > clue(&line, 3, game::Clue::Color(ClueColor::Green()))
    );
}

#[test]
fn maximize_knowledge_transfer1() {
    // seed 28176
    let line = replay_game(
        21,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1ua1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
        "0",
    ).line;

    assert!(
        clue(&line, 3, game::Clue::Rank(2)) > clue(&line, 3, game::Clue::Color(ClueColor::Red()))
    );
}

#[test]
fn maximize_knowledge_transfer2() {
    // seed 28176
    let line = replay_game(
        34,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1Da1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
        "0",
    ).line;

    assert!(
        clue(&line, 2, game::Clue::Rank(2)) > clue(&line, 2, game::Clue::Color(ClueColor::Blue()))
    );
}

#[test]
fn clue_multiple_ones1() {
    // seed 7690
    let line = replay_game(
        1,
        "415xubpclkdpbfpfiisghlmxdsruwgnfoawrncaqutkahjvmkqyev",
        "05oc0aakubadagpaocabvcaqoaarbeajbmbaiabibn7dodblao7cbhibbyDda4vab5aciabxb7a80dvdaADd7ab00bava6b2apvdbw0daHbt1ab9aCazocaG7cidqb",
        "0",
    ).line;
    assert!(
        clue(&line, 3, game::Clue::Rank(1)) > clue(&line, 3, game::Clue::Color(ClueColor::Blue()))
    );
}

#[test]
#[ignore]
fn clue_multiple_ones2() {
    // seed 278
    let replay = replay_game(
        1,
        "415isapgyqxplkqmbsktuwhivnexfncwdgvrajfpahfurdlmcboku",
        "05uc0cakiaacidaianvc6aalDcar6aajbmav6casbo6bahbuibbaa1iabpa3af1aoca6beazbtDdDdbwaxudbgvbb5bba4b21cocb7aIa0udb9aCaLbdbEbqqd",
        "0",
    );

    assert!(
        replay.clone().clue(0, game::Clue::Rank(1))
            > replay.clone().clue(2, game::Clue::Color(ClueColor::Blue()))
    );
}

#[test]
/// With r 1..4 played; clue red instead of 5 to [r1, p2, g4, r5]
fn clue_with_trash() {
    // seed 278
    let line = replay_game(
        36,
        "415isapgyqxplkqmbsktuwhivnexfncwdgvrajfpahfurdlmcboku",
        "05uc0cakiaacidaianvc6aalDcar6aajbmav6casbo6bahbuibbaa1iabpa3af1aoca6beazbtDdDdbwaxudbgvbb5bba4b21cocb7aIa0udb9aCaLbdbEbqqd",
        "0",
    ).line;

    assert!(
        clue(&line, 3, game::Clue::Color(ClueColor::Red())) > clue(&line, 3, game::Clue::Rank(5))
    );
}

#[test]
/// With g 1..3 played, and g5 visible on other hand; clue green to b5, g4, g1, g4 hand
fn clue_with_trash2() {
    // seed 62369
    let mut line = replay_game(
        11,
        "415vkglnkntsbxmjwvedqdiombhuuylsfupafpfarwcpxqhcgraki",
        "05Ddpabibnabuabjuaaducalbobavabk6cbc6caybridvdudaw0dbeob7bbsa3Da0bbxa5bqb1bzaguabpaubfodbCpdb7odb9vbbGDbb2odod1daHb4bhbt0bb6Ddbvqd",
        "0",
    ).line;

    let score = line
        .clue(2, game::Clue::Color(game::ClueColor::Green()))
        .expect("Clue should be valid");
    println!("=> {:?}", line);
    println!("Score: {score:?}");
    assert!(!score.has_errors());
}

#[test]
/// with [b3, b1, b4, b5[5]]
/// clue blue with full hand, identifies b4 on chop, b5 from previously + two chop cards
fn clue_with_trash_based_on_previous_clues() {
    // seed 278
    let line = replay_game(
        37,
        "415humvpfntxydvaucswfikulpqnschpwgkxrirbafdljambkoegq",
        "05pbafpdanvcaealam6baqbiucDcbgatvbDbaxubbobaavuabpacoabk7ca47aay1ca6bsa70bbbaz1abra1ahoaica8b9aADbbdaFajubbBaKbwbu1d7dqc",
        "0",
    ).line;
    assert!(
        line.hands.slot(0, 0).trash,
        "Slot 0 should be trash (quantum: {})",
        line.hands.slot(0, 0).quantum
    );
    assert!(
        line.hands.slot(0, 1).trash,
        "Slot 1 should be trash (quantum: {})",
        line.hands.slot(0, 1).quantum
    );
    assert!(
        line.hands.slot(0, 2).play,
        "Slot 2 (previously chop), should be playable b4 (quantum: {})",
        line.hands.slot(0, 2).quantum
    );
}

#[test]
/// with [p1, b2, g2, b5'] and all 2 except g2 played
/// clueing 2 marks b2 as trash
fn clue_with_trash_in_safe_clue() {
    // seed 14124
    let line = replay_game(
        44,
        "415tmcfndirjgqahxplurubyfomvhegsbdlkwqpfxauiwsvkcknap",
        "05icoaal6cadDaaqDcvc0dajaoodbebsamicucatbnvcbfayobbbagak1aarbvai0aa26ca7bpacvab16aaBbhubbuaaa61dDba5b3udaxb8ica4bFDba07abzqa",
        "0",
    ).line;
    assert!(
        line.hands.slot(0, 1).trash,
        "Slot 0 should be trash (quantum: {})",
        line.hands.slot(0, 1).quantum
    );
    assert!(
        !line.hands.slot(0, 2).trash,
        "Slot 2 (previously chop), should be playable b4 (quantum: {})",
        line.hands.slot(0, 2).quantum
    );
}

#[test]
fn priority_saves() {
    // seed 52
    let line = replay_game(
        39,
        "415xtilakwjkoagpcyefqcsnumvfdghpramqpbfihkunvblrsdxuw",
        "05pbaf0damobaq0darocaealuaad6cavuaawbgubbnDdaubiobbaa11abta3ucaj7aacahiab0a8Dabkvcbybsaxb5b6bzb27cbBb4aJ0aab1ab7bAaNqb",
        "0",
    ).line;
    assert!(clue(&line, 2, game::Clue::Rank(3)) > clue(&line, 3, game::Clue::Rank(2)));
}

#[test]
fn no_double_cluing1() {
    // seed 52
    let line = replay_game(
        23,
        "415wmjgiqdkvcfvadsshwpmloqxlbgciaenufbkaunpxtpkruhrfy",
        "05idocakamubahoa0badasub1c0bauaqbo7dafbivcba6dbj7bbbaeuabxatidb1Dbac7ab6b4a5bgpbDbb2aBazbnb0avaw0cb7b3aIap0dbyalaF1dbDqc",
        "0",
    ).line;
    assert!(
        clue(&line, 2, game::Clue::Color(game::ClueColor::Yellow()))
            > clue(&line, 2, game::Clue::Rank(4))
    );
}

#[test]
/// don't clue [y1 y4 y1 xx] with yellow if there are good alternatives
fn no_double_cluing2() {
    // id 2
    let line = &replay_game(
        0,
        "415jqgbuywktfifdpraklukmhrpsaxnvlgvehdcncfpmxauoqsiwb",
        "05ocDaalpc6baeaqoaacpdvbapudarbjauDcbh1banbbaviabmad6dbsa2uba1bw6abtagibboa0a97abxa8a6Db0cb3afbiuc7cbyaI0bbBaJakbDaabMbNqd",
        "0",
    ).line;
    println!("line: {:?}", line);
    assert!(
        clue(&line, 1, game::Clue::Rank(1))
            > clue(&line, 2, game::Clue::Color(game::ClueColor::Yellow()))
    );
}

#[test]
/// giving clue to [b5, x, x, x] (with b1..b4 played) must be a play clue on the b5
/// It must not interpreted as a trash card
fn clue_clear_cards() {
    // seed 17
    let line = replay_game(
        41,
        "415xvpskfqgqixmgddhawafusoplmnwfchrcprbltubjeiayunkvk",
        "05pbafodamibaqpaubacaepb0bodaubiap7cagajbn7dbsubbrbaayva6aabbh1abxa57ablicadbva90ba3bBobaobtaEid1cb8Dcbz1bbAa1ak6abIb6bwbFbDqb",
        "0",
    ).line;
    assert!(
        !line.hands.slot(0, 0).trash,
        "Slot 0 should not be trash (quantum: {})",
        line.hands.slot(0, 0).quantum
    );
    assert!(
        line.hands.slot(0, 0).play,
        "Slot 0 should be playable b5 (quantum: {})",
        line.hands.slot(0, 0).quantum
    );
}

#[test]
fn immediatly_update_play_flags() {
    // seed 205
    let line = replay_game(
        21,
        "415nkifgwinraekpdfqvhlyumwsudcaacxftjoqmpuxbshbrgkvlp",
        "05pdpcalaoobaeajamubasodarubav0dap0cbfai7bbaaxubbnicahakbu0dagbqa6ucpda8ay6cb2atobbba71diaaEb4a0b3b1azb5vbbcaL6daw6cqb",
        "0",
    ).line;
    assert!(
        line.hands.slot(0, 2).play,
        "Slot 2 is playable (card is clearly g4): quantum: {}",
        line.hands.slot(0, 2).quantum
    );
    assert!(
        line.hands.slot(0, 3).play,
        "Slot 3 is playable (card is clearly y4): quantum: {}",
        line.hands.slot(0, 3).quantum
    );
}

#[test]
fn immediatly_update_play_flags2() {
    // shared replay 739462
    let line = replay_game(
        1,
        "415fmblraiypxfkrwscdakedvjsuniuaxmvkclhwnhufggqobqtpp",
        "33cc",
        "0",
    )
    .line;
    assert!(
        line.hands.slot(1, 0).play,
        "Slot 1 is playable (card is a one): quantum: {}",
        line.hands.slot(1, 0).quantum
    );
    assert!(
        line.hands.slot(1, 1).play,
        "Slot 2 is playable (card is a one): quantum: {}",
        line.hands.slot(1, 1).quantum
    );
    assert!(
        line.hands.slot(1, 3).play,
        "Slot 4 is playable (card is a one): quantum: {}",
        line.hands.slot(1, 3).quantum
    );
}

#[test]
fn dont_reclue_uselessly() {
    // shared replay 739462
    let mut line = replay_game(
        1,
        "415fmblraiypxfkrwscdakedvjsuniuaxmvkclhwnhufggqobqtpp",
        "33cc",
        "0",
    )
    .line;
    let score = line.clue(1, game::Clue::Color(game::ClueColor::Yellow()));
    println!("score: {:?}", score);
    println!("=> {:?}", line);
    assert!(score.expect("Clue is valid").has_errors());
}

#[test]
fn fix_clue_revealing_real_identity() {
    // shared replay 37630
    let mut line = replay_game(
        11,
        "415qiwbhdnvrygexohsdxwpfmsfnbuurkpfajquicmgctaklvapkl",
        "05ibafbiDcbavcbrpc7dDdatocbbbeauvcodbgbkbn6dbhiba00dbwpbubbca3bxbpDdb1bziabs6db66cbv6ca9bm6cb51dbE6cb80dicb4vaoauaaIuabBb2bCbyblibbJ6cqc",
        "0",
    ).line;
    assert_eq!(line.hands.slot(3, 2).quantum.size(), 1);
    assert_eq!(
        line.hands
            .slot(3, 2)
            .quantum
            .iter()
            .nth(0)
            .expect("size is 1"),
        game::Card {
            rank: 2,
            suit: game::Suit::Blue()
        }
    );
    line.clue(3, game::Clue::Color(game::ClueColor::Yellow()));
    println!("=> {:?}", line);
    assert_eq!(line.hands.slot(3, 2).quantum.size(), 1);
    assert!(line.hands.slot(3, 2).fixed);
}

#[test]
/// Clues should only mark cards as clued if that is actually the case (even if the foreign player
/// will assume it is a y2 we must not makr y2 as clued).
fn only_mark_card_as_clued_if_actually_the_case() {
    // id 66540
    let replay = replay_game(
        8,
        "415gdjsfnaqvarhgwaifbfwnboxlvrpkhtdklcqpcyxiukuupsmem",
        "05pbagbivcbaaearoaodbfbjpabsvdbk0bbbvdblucbd0ab0uca1bhvdbmodbqububbcb6odbppc0da2vc0dbtayb96dbvbApcDcb4aFbo7db8ibbIbxaB1aa5aLvaauoaqa",
        "0",
    );
    for line in replay.lines.iter() {
        assert!(line.card_states[&game::Card {
            rank: 2,
            suit: game::Suit::Yellow(),
        }]
            .clued
            .is_none());
    }
}

#[test]
/// If a safed card could have multiple identities, do not consider this card is clearly clued for that player.
fn dont_consider_ambiguous_safes_as_clued() {
    // id 88355
    let replay = replay_game(
        29,
        "415soerqcrcvfnatpxkhjwiywpxgfmasidqkfdglvuubklahubmpn",
        "05pdpcalapDdbeajanbaDabiboDd6dbkvcbdbfay1bocb0aq0bbvbsat0b7dodbwarbz6db1vcb4idaApcb9vaaCicaBbhaEiabcbGb7ucbD1a1ab8bLucbKoabNqb",
        "0",
    );
    for line in replay.lines.iter() {
        assert_ne!(
            line.card_states[&game::Card {
                rank: 2,
                suit: game::Suit::Blue(),
            }]
                .clued
                .unwrap_or(0),
            255
        );
        assert_ne!(
            line.card_states[&game::Card {
                rank: 4,
                suit: game::Suit::Blue(),
            }]
                .clued
                .unwrap_or(0),
            255
        );
    }
}

#[test]
/// Don't keep wrong state from a bad intermediate clue
/// In a 8 clue situtation, a purple clue was given (touching p2 on chop).
/// Upon misplaying them as p1, it should be clear that p1 was not yet clued.
fn correct_card_assumption_upon_misplay() {
    // id 4441
    let line = replay_game(
        18,
        "415tijdgavpvlsxrcsgrxampmyuucibbkoeffudhkqnkwwnqphlaf",
        "05pbahDabmbbaf7abn6cbeai6b1bbvbj6cbsiaby7cocbxa0oaiba1pbiaaza3DbDaada5oapaa6b7odapbcodbkaA0cb9aC0bb8aq0dbubBbgbwucbGbEaLbobJbHqc",
        "0",
    ).line;
    assert!(line.hands.slot(0, 0).play, "p1 must be marked as playable");
}

#[test]
/// Only play cards if there is a change they are playable.
/// Fix clue that clear the identity should potentially remove the play annotation
fn dont_play_cards_not_playable() {
    // id 6631
    let line = replay_game(
        17,
        "415xapsgahtqlohngmwnedbudqvfirupuxiswfpkfkcjkmvbclary",
        "05ibafbivbba7dbjicDcbgat6b1cauDbobbbae0a6aax1dobapacaybvamvbbwubbn6dbh7aa4bzb2pbb0bdaA1b6bb1aDiab6asb7arb8vdb5blaKud6d1daNqa",
        "0",
    ).line;
    let slot = &line.hands.slot(0, 3);
    assert!(
        !slot.play,
        "y2 must be marked as playable (it can't be) {:?}",
        slot
    );
}

#[test]
/// Should a player be forced to discard a clued card, remove them from the clued state
fn clear_clued_cards_on_discard() {
    // id 34077
    let replay = replay_game(
        19,
        "415suknlcxuhfpimviqsejnpqbvfxywdcghbwtdfmuaakkraplgro",
        "05pcpaakbmac0dajapab6dvbanbaaebibo7cDdbz6cbdDaa1buodbf6ba4icbhbtDcbvbg6db0udbyb8aBpcb5aDoabs0abGocaHbFaq7cbx7cbl1cb2iabNqd",
        "0",
    );
    for line in replay.lines.iter() {
        assert!(line.card_states[&game::Card {
            rank: 4,
            suit: game::Suit::Purple(),
        }]
            .clued
            .is_none());
    }
}

#[test]
/// Should a clue fixed the identity of a card, clear clued cards from old reasoning
fn clear_clued_cards_on_fix_clue() {
    // id 51155
    let replay = replay_game(
        7,
        "415mftyndbwgicxorhvqdsrflpuckjuhqkaeppifalvunwskgxmab",
        "05Ddoabibnab1dodbpbaDabj7bubbg0bbr6daw0daxpd0caqazuc1davbtbs0abkicbcbha5ibbub7blDcb4Daoab16d7ab2aDbC6a1ab3aGbAobvbbIaJ6aaoaKiaub6aaNqb",
        "0",
    );
    for line in replay.lines.iter() {
        assert!(line.card_states[&game::Card {
            rank: 3,
            suit: game::Suit::Blue(),
        }]
            .clued
            .is_none());
    }
}

#[test]
#[ignore]
/// Should a player have two copies of the same card, on is to be discarded
fn discard_double_card() {
    // id 50241
    let line = replay_game(
        13,
        "415pbedsnnyrwhjgaucxllpaiafudfxqtmuqfkmkkgicpsowhrvbv",
        "05pd0abiaoaabepdanbbDabj7bubbhobbmbdax1dbriduabkau0cDca2bybviabqb1pdDdbwa8b6vab4vcasucaCb5bB7aa9bAubagobucbcaJaH0aaK6a6bvcaNqb",
        "0",
    ).line;
    println!("line: {:?}", line);
    assert_ne!(
        line.hands.slot(0, 2).trash,
        line.hands.slot(0, 3).trash,
        "Only one g4 is needed, discard the other one"
    );
}

// delayed play clues:
#[test]
fn unambiguous_delayed_play_clue_by_color() {
    let mut line = hyphenated::Line::new(4, 0);
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [y 1, g 1, r 1, b 1]);
    hand!(line 2: [y 3, y 3, y 4, y 4]);
    hand!(line 3: [g 4, g 4, g 3, g 3]);

    println!("line: {:?}", line);

    line.clue(1, game::Clue::Rank(1));

    println!("line: {:?}", line);

    line.played(
        1,
        3,
        game::Card {
            rank: 1,
            suit: game::Suit::Blue(),
        },
        true,
    );

    println!("line: {:?}", line);

    line.clued(
        2,
        0,
        game::Clue::Color(game::ClueColor::Red()),
        PositionSet::create(4, 0100),
    );

    println!("line: {:?}", line);

    assert_eq!(
        line.hands.slot(0, 2).quantum.size(),
        1,
        "clued card can only be r2"
    );
    assert_eq!(
        line.hands
            .slot(0, 2)
            .quantum
            .iter()
            .nth(0)
            .expect("check previously"),
        game::Card {
            rank: 2,
            suit: game::Suit::Red()
        },
        "r2 as delayed play clue on r1"
    );
}

#[test]
fn ambiguous_delayed_play_clue_by_rank() {
    let mut line = hyphenated::Line::new(4, 0);
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    line.own_drawn();
    hand!(line 1: [y 1, g 1, r 1, b 1]);
    hand!(line 2: [y 3, y 3, y 4, y 4]);
    hand!(line 3: [g 4, g 4, g 3, g 3]);
    line.clue(1, game::Clue::Rank(1));
    line.played(
        1,
        3,
        game::Card {
            rank: 1,
            suit: game::Suit::Blue(),
        },
        true,
    );

    println!("line: {:?}", line);

    line.clued(2, 0, game::Clue::Rank(2), PositionSet::create(4, 0100));

    println!("line: {:?}", line);

    assert_eq!(
        line.hands.slot(0, 2).quantum.size(),
        4,
        "clued card can only be y2, g2, r2, b2"
    );
    assert_eq!(
        line.hands
            .slot(0, 2)
            .quantum
            .iter()
            .collect::<Vec<game::Card>>(),
        vec![
            game::Card {
                rank: 2,
                suit: game::Suit::Red()
            },
            game::Card {
                rank: 2,
                suit: game::Suit::Yellow()
            },
            game::Card {
                rank: 2,
                suit: game::Suit::Green()
            },
            game::Card {
                rank: 2,
                suit: game::Suit::Blue()
            },
        ],
        "the 2 could be any building of any of the already played/clued 1"
    );
}

#[test]
/// delayed play clues on player themselves
fn extend_delayed_play_after_first_plays() {
    // id 24141
    let mut replay = replay_game(
        5,
        "415eptdwhpvmcirgmivulubndghkxykucpjlfaaxsnqwsqbffokar",
        "050bagDapbvbaqbibm1d6dbjbpbbbhia7abwuaubucayarbkan7cbeaubobzbfpbbtpda7bla86dbxiab20db0iaaDb6iabsibicb9aHibbGa3uabvaK0abJiaaNqb",
        "0",
    );
    replay.play(0, None);
    assert_eq!(
        replay.lines[1]
            .hands
            .slot(0, 0)
            .quantum
            .iter()
            .collect::<Vec<game::Card>>(),
        vec![game::Card {
            rank: 2,
            suit: game::Suit::Purple()
        },],
        "When slot 1 been played as p1, this must be p2"
    );
}
#[test]
/// clue p5 on chop, with purple if p3 is already played and p4 already clued
fn clue_color_instead_5_for_delayed_playable_5s() {
    // id 13851371563455722778
    let line = &replay_game(
        33,
        "415xfaqfgptnpfgrascsviywuachvkwdkuerxnbmdpqibmhljkuol",
        "05pbagia0aadae0damacvcalaoDbahbi6c1cbfayobucasa0bnbabqavvaar1a6abpauDdibbwa7vabjiaa9bz6daticaxa2b3vda5obaIbBaJ1daAudbEuba8qa",
        "0",
    ).line;
    println!("line: {:?}", line);
    assert!(
        clue(&line, 2, game::Clue::Color(game::ClueColor::Purple()))
            > clue(&line, 2, game::Clue::Rank(5))
    );
}

#[test]
/// prefer color clues if it clear card clued twice on one hand (makes on playable and one trash)
fn clear_up_doubled_card_in_one_card() {
    // id 58695
    let line = &replay_game(
        10,
        "415lxtvebqbsihwflnoppqrnifjmpdsvuwhxakgrkfakduymucgac",
        "05Dbodbiam0cbfaqvb1cbevbbnvbvabjboDdbuudvbbbDavbbpvbby7bbr7db1pbvb6da36ab2ad6abwicbcuaa7b5a8asat0baaahb91cbz6abDucbBbgaIocbEb4aLbAbJbCqc",
        "0",
    ).line;
    println!("line: {:?}", line);
    assert!(
        clue(&line, 3, game::Clue::Color(game::ClueColor::Blue()))
            > clue(&line, 3, game::Clue::Rank(2))
    );
}

#[test]
/// a nice 5-for-1 finess starting with a self-finess (all in order)
fn good_touch_principal() {
    // id 31554
    let replay = &replay_game(
        23,
        "415tlxnrlfdxhwyrjckpmpkhifvvgckgmaunaisweausbpfoduqbq",
        "05obagDapbudaquaapabbe1dbnbc6cbjbsbtvaobbvazau1bby6carib7aada46ab0a57abxb3a8bfblb7bwiabkiaaFidbiaouc7daIbJ0db2b9aL0db67bqd",
        "0",
    );
    let g4_slots = replay.slot_perspectives(0, 2);
    println!("g4_slots: {:?}", g4_slots);
    for (player, slot) in g4_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 4,
                suit: game::Suit::Green()
            }],
            "Player {player} missed a clue on green; only g4 is left"
        );
    }
}

#[test]
#[ignore]
fn good_touch_principal2() {
    // id 56145
    let mut replay = replay_game(
        0,
        "415ckxnvsrmujuaqcognqupdpdaistegbmffkbvhayfwhkpilwlxr",
        "05pcpaal6babDcak6cpcaepb0abaatbsbmar1dDdbpbcbf0bpbbdagib7c0da3biazpcbva5an7cbhawoaa2a11bbx1db70aaEa0bD1b1bbuaFuab6aJaIajubbya4qc",
        "0",
    );
    assert!(
        replay.clue_is_bad(2, game::Clue::Rank(1)),
        "p1 is double clued"
    );
}

// prompts
#[test]
fn dont_wait_for_potential_self_prompts_of_the_clue_giver() {
    // id 36
    let replay = &replay_game(
        13,
        "415asbffugnxpehrufwdxiqktyvhnakgrmpmsvlpcaklbqcudwioj",
        "05pbafia0caaaeajob0cagatpb1dau7aamabbh1abnayDcavobacasbibovdbq6baxDbbzvbb0uba9blapibaBb2b6ara4b57cbdawaJb8odbLbCaNqa",
        "0",
    );
    let slots = replay.slot_perspectives(3, 3);
    println!("3er_slots: {:?}", slots);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![
                game::Card {
                    rank: 3,
                    suit: game::Suit::Yellow()
                },
                game::Card {
                    rank: 3,
                    suit: game::Suit::Blue()
                }
            ],
            "Player {player} missed the prompt; p2 is promised"
        );
    }
}

#[test]
fn prompt_via_color_5_safe() {
    // id 36
    let replay = &replay_game(
        31,
        "415asbffugnxpehrufwdxiqktyvhnakgrmpmsvlpcaklbqcudwioj",
        "05pbafia0caaaeajob0cagatpb1dau7aamabbh1abnayDcavobacasbibovdbq6baxDbbzvbb0uba9blapibaBb2b6ara4b57cbdawaJb8odbLbCaNqa",
        "0",
    );
    let p2_slots = replay.slot_perspectives(3, 2);
    println!("p2_slots: {:?}", p2_slots);
    for (player, slot) in p2_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 2,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p2 is promised"
        );
    }
    let p3_slots = replay.slot_perspectives(3, 3);
    println!("p3_slots: {:?}", p3_slots);
    for (player, slot) in p3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p3 is promised"
        );
    }
    let p4_slots = replay.slot_perspectives(0, 2);
    println!("p4_slots: {:?}", p4_slots);
    for (player, slot) in p4_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 4,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p4 is promised"
        );
    }
    let p5_slots = replay.slot_perspectives(1, 3);
    println!("p5_slots: {:?}", p5_slots);
    for (player, slot) in p5_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 5,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p5 is promised"
        );
    }
}

// todo:
// multi cards prompts
// clear card identiy based on prompt cards being played
//   - like r4 critical, red on chop clued;
//   - other player plays previously clued r2
//   => must be r3 on chop not r4)

#[test]
fn dont_prompt_own_cards_if_they_are_ambigious() {
    // id 56145
    let mut line = replay_game(
        19,
        "415ckxnvsrmujuaqcognqupdpdaistegbmffkbvhayfwhkpilwlxr",
        "05pcpaal6babDcak6cpcaepb0abaatbsbmar1dDd7abcbf1banbzagiaibbua3obbpb2a5oaocbybvbjbxb4b01bobb7aEbib1DdbAubpbadaJbq6cudahaMbIbKbNqc",
        "0",
    ).line;
    assert!(
        line.clue(1, game::Clue::Rank(4)).expect("").has_errors(),
        "the 3 could be different cards"
    );
}

#[test]
fn self_prompt() {
    let replay = replay_game(
            11,
            "415uwcsgfqvfcbpdbikgxevorthxuylkhmrjqpnwismaaupkdlnfa",
            "05pc6aal0baaagaivcud7aatapar1d0aavadbe7abmayuabjoca1bfaqDcaw7ab2aoa3oab5uba6a4uabna9paau1caEbhaAwc",
            "0",
    );
    let p3_slots = replay.slot_perspectives(0, 3);
    println!("p3_slots: {:?}", p3_slots);
    for (player, slot) in p3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p3 is promised"
        );
    }
    let p4_slots = replay.slot_perspectives(0, 0);
    println!("p4_slots: {:?}", p4_slots);
    for (player, slot) in p4_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 4,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p4 is promised"
        );
        // if player == 0 {
        // } else {
        //     assert_eq!(
        //         slot.delayed, 1,
        //         "Player {player} missed the prompt; p4 is promised"
        //     );
        // }
    }
}

#[test]
#[ignore]
// tests needs to be adapted.
fn self_prompt_after_potential_finess() {
    let replay = replay_game(
            11,
            "415uwcsgfqvfwbpdbikgxevorthxuylkhmrjqpncismaaupkdlnfa",
            "05pc6aal0baaagaivcud7aatapar1d0aavadbe7abmayuabjoca1bfaqDcaw7ab2aoa3oab5uba6a4uabna9paau1caEbhaAwc",
            "0",
    );
    let p3_slots = replay.slot_perspectives(0, 3);
    println!("p3_slots: {:?}", p3_slots);
    for (player, slot) in p3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p3 is promised"
        );
    }
    let p4_slots = replay.slot_perspectives(0, 0);
    println!("p4_slots: {:?}", p4_slots);
    for (player, slot) in p4_slots.iter().enumerate() {
        if player == 0 {
            assert_eq!(
                slot.quantum.iter().collect::<Vec<game::Card>>(),
                vec![game::Card {
                    rank: 4,
                    suit: game::Suit::Purple()
                }],
                "Player {player} missed the prompt; p4 is promised"
            );
        } else {
            assert_eq!(
                slot.delayed, 1,
                "Player {player} missed the prompt; p4 is promised"
            );
        }
    }
}

#[test]
#[ignore]
// tests needs to be adapted.
fn self_finess_instead_of_prompt() {
    let replay = replay_game(
            11,
            "415uycsgfqvfwbpdbikgxevorthxuwlkhmrjqpncismaaupkdlnfa",
            "05pc6aal0baaagaivcud7aatapar1d0aavadbe7abmayuabjoca1bfaqDcaw7ab2aoa3oab5uba6a4uabna9paau1caEbhaAwc",
            "0",
    );
    let p3_slots = replay.slot_perspectives(2, 3);
    println!("p3_slots: {:?}", p3_slots);
    for (player, slot) in p3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p3 is promised"
        );
    }
    let p4_slots = replay.slot_perspectives(0, 0);
    println!("p4_slots: {:?}", p4_slots);
    for (player, slot) in p4_slots.iter().enumerate() {
        if player == 0 {
            assert_eq!(
                slot.quantum.iter().collect::<Vec<game::Card>>(),
                vec![game::Card {
                    rank: 4,
                    suit: game::Suit::Purple()
                }],
                "Player {player} missed the prompt; p4 is promised"
            );
        } else {
            assert_eq!(
                slot.delayed, 1,
                "Player {player} missed the prompt; p4 is promised"
            );
        }
    }
}

#[test]
/// If if cards are still in the process of being played to resolve (self)-prompts
/// they are still clued
/// Don't see finesses if a play-clue is added upton
/// in this case p4 was self-prompted upon p3; while they are still to-be-played
/// p5 is clued (while p4 would lay on finess on a different player)
fn no_finess_if_cards_are_already_clued() {
    let replay = replay_game(
        20,
        "415uwcsgfqvfwbpdbhkgxevorthxuylkimrjqpncismaaupkdlnfa",
        "03lcwaalsbaaagaipcodxaatapbcsabjavadkcaqwcabbeaxkcarbfa3ta",
        "0",
    );
    let slots = replay.slot_perspectives(3, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert!(
            !slot.play,
            "Player {player} saw a finess of already clued cards: {slot:?}"
        );
    }
}

#[test]
#[ignore]
fn self_prompt2() {
    let line = replay_game(
            52,
            "415uwcsgfqvfwbpdbhkgxevorthxuylkimrjqpncismaaupkdlnfa",
            "05pc6aal0baaagaivcud7aatapab1d0aavarbebj6caduaa0oca1bfaqDcaw1db2axodbh1bbouba4b5obbcaBoabma6a9auDbpd1db7aFvdbHbkbKbGbEbCb8qa",
            "0",
    ).line;
    assert!(
        clue(&line, 2, game::Clue::Color(game::ClueColor::Red()))
            > clue(&line, 3, game::Clue::Rank(2))
    );
}

#[test]
fn no_self_prompt_if_easier_alternative() {
    // seed 205
    let replay = replay_game(
        13,
        "415nkifgwinraekpdfqvhlyumwsudcaacxftjoqmpuxbshbrgkvlp",
        "05pdpcalaoobaeajamubasodarubav0dap0cbfai7bbaaxubbnicahakbu0dagbqa6ucpda8ay6cb2atobbba71diaaEb4a0b3b1azb5vbbcaL6daw6cqb",
        "0",
    );
    let g3_slots = replay.slot_perspectives(1, 0);
    println!("g3_slots: {:?}", g3_slots);
    for (player, slot) in g3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Green()
            }],
            "Player {player} must assume new card is simply g3"
        );
    }
    let not_g3_slots = replay.slot_perspectives(1, 1);
    println!("not_g3_slots: {:?}", not_g3_slots);
    for (player, slot) in not_g3_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![
                game::Card {
                    rank: 3,
                    suit: game::Suit::Green()
                },
                game::Card {
                    rank: 4,
                    suit: game::Suit::Green()
                },
                game::Card {
                    rank: 5,
                    suit: game::Suit::Green()
                }
            ],
            "Player {player} reasoned too much about non-focused card"
        );
    }
}

#[test]
fn no_self_prompt_if_easier_alternative2() {
    // Seed 2
    let replay = replay_game(
            23,
            "415jqgbuywktfifdpraklukmhrpsaxnvlgvehdcncfpmxauoqsiwb",
            "05pcDaal6bpdaeajapucoaaqanacva1davadbhubbmabar6baobwocakbtaaucau7aaybz1abxa9ica8b00cbAaivca1Dda5a6ucbFaI1bb7agbLbJbMbNqc",
            "0",
    );
    let p2_slots = replay.slot_perspectives(1, 0);
    println!("p2_slots: {:?}", p2_slots);
    for (player, slot) in p2_slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 2,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed the prompt; p3 is promised"
        );
        assert_eq!(slot.delayed, 0);
    }
}

#[test]
fn always_consider_foreign_prompts() {
    // seed 9756824915852424369
    let replay = replay_game(
        28,
        "415tdcbfjvfmhuixnglskwuqigaahmnlsyapqoxudrekvpfwkprcb",
        "05pbahDa6codaeakaoubar6b6cudagasapbb1caibmDbbqidaxbcvabl7bada1bwpbbya6uabna80a6bbt0ba9bz6cb5aua47aDbbGajobbFav0daCa3afidaMaBqb",
        "0",
    );

    let focus_slots = replay.slot_perspectives(1, 0);
    assert_eq!(
        focus_slots[1].delayed, 1,
        "Player 1 must wait a round to give 2 a change to initiate a prompt (if y4 focused)"
    );
}
#[test]
fn always_consider_foreign_prompts2() {
    // seed 9756824915852424369
    let replay = replay_game(
            27,
            "415tdcbfjvfmhuixnglskwuqigaahmnlsyapqoxudrekvpfwkprcb",
            "05pbahDa6codaeakaoubar6b6cudagasapbb1caibmDbbqidaxbcvabl7bada1bwpbbya6uabna80a6bbt0ba9bz6cb5aua47aDbbGajobbFav0daCa3afidaMaBqb",
            "0",
        );
    assert!(
        replay
            .clone()
            .clue(1, game::Clue::Color(game::ClueColor::Yellow()))
            > replay.clone().clue(1, game::Clue::Rank(4))
    );
}

#[test]
fn dont_self_prompt_yourself() {
    // seed 1204237994774607731
    let mut replay = replay_game(
        10,
        "415fjpkhpkaaiyuuwbbsnsincduoxmaepqvrqtkdcwvhrxgllffmg",
        "05pbahidap6cagaloaaaaf1dibauavakarwd",
        "0",
    );
    assert!(replay.clue_is_bad(3, game::Clue::Rank(3)));
}

#[test]
// waitplay callbacks must be removed if that card is chopped
fn dont_self_prompt_yourself2() {
    // id 184
    let mut replay = replay_game(
        12,
        "415rdumafqudgvpbaufxglhhynqsmfiwkivcsarbekxnotlkpjcpw",
        "05pbahvdvc0bafal6cocagak1a6baqauapwd",
        "0",
    );
    assert!(replay.clue_is_bad(1, game::Clue::Color(game::ClueColor::Purple())));
}

#[test]
fn no_wait_if_not_possible() {
    // seed 0
    let replay = replay_game(
            11,
            "415uxxrhphtwqkdbgvksgqfneysiufmwjomkilfaalvcnbpcrpadu",
            "05uc6aakpbaaaf0b0a7cas6d6cadbeaqobbrahpb6cbuatDbbmodbybjanDcagb3bpodb0uba7bxa8iaubaAa5b6b17dudbEaFvcb2aGa41baIalvbbzaDbJbKbCqb",
            "0",
        );
    let slots = replay.slot_perspectives(3, 1);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 2,
                suit: game::Suit::Purple()
            }],
            "Player {player} misinterpreted the simply play clue"
        );
        assert_eq!(
            slot.delayed, 0,
            "Player {player} incorrectly saw a potential prompt"
        );
    }
}

#[test]
fn unambiguous_prompt_clue_by_rank() {
    // seed 0
    let replay = replay_game(
            8,
            "415fkhmpbsvcdaxxjyinlawuuqwlfqapihorknrsevfkbgtcupgmd",
            "050baepapcabuaak7barbfpbvbadau0bbmaaaw6d7cbcDdbibpbtbzobb2udbq0bb5bva67ab7a8ag6bbAbybCibbDvdaFDbaGidaHbsaIoba4ajiaaBb9b1qd",
            "0",
        );
    let slots = replay.slot_perspectives(0, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 2,
                suit: game::Suit::Green()
            }],
            "Player {player} misinterpreted the direct play clue"
        );
    }
    let slots = replay.slot_perspectives(0, 1);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Green()
            }],
            "Player {player} misinterpreted the prompt"
        );
    }
    let slots = replay.slot_perspectives(1, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 4,
                suit: game::Suit::Green()
            }],
            "Player {player} misinterpreted the prompt"
        );
    }
}

#[test]
fn ambigious_delayed_play_clue() {
    // seed 0
    let replay = replay_game(
            12,
            "415fkhmpbsvcdaxxjyinlawuuqwlfqapihorknrsevfkbgtcupgmd",
            "050baepapcabuaak7barbfpbvbadau0bbmaaaw6d7cbcDdbibpbtbzobb2udbq0bb5bva67ab7a8ag6bbAbybCibbDvdaFDbaGidaHbsaIoba4ajiaaBb9b1qd",
            "0",
        );
    let slots = replay.slot_perspectives(1, 2);
    assert_eq!(
        slots[0].delayed, 1,
        "Alice knows the play is not delayed by her one (to be fixed later)"
    );
    assert_eq!(
        slots[1].delayed, 2,
        "Bob must wait for the too already clued ones to be played"
    );
}

#[test]
/// you can't clue purple touching [p3, p1, ...] (with p2 on finess elsewhere)
fn self_prompts_cant_include_new_cards() {
    // seed 0
    let mut replay = replay_game(
        0,
        "415gtxvqcxvnduwadqfpkomfsislnwphrukkilyjfamercbhubagp",
        "056cpdalapvb6aaqvaibafudarbbvd0daobv7abi0bbxucaybmubatbjuaazbg7dbu0cb2a36cb46aa0wc",
        "0",
    );
    assert!(replay.clue_is_bad(2, game::Clue::Color(game::ClueColor::Purple())))
}

// try to act like in https://hanab.live/shared-replay-json/415gtxvqcxvnduwadqfp-komfsislnwphrukkilyj-famercbhubagp,03tcka-akapsbahaq1aldaealam-aaxabiarocodaybnackd-atsaa2bfa3azaxxcbvaw-ab1ca91bbdas1daAb4bg-1bbob8aHtab1aGbuajb7-gbaE,0

// todo https://hanab.live/shared-replay-json/415uxxrhphtwqkdbgvks-gqfneysiufmwjomkilfa-alvcnbpcrpadu,03ocwa-aklbaaafsbsaxcaswdwc-adbeaqaokbahpdbpxbat-aianacagbjwcbbayaw,0

#[test]
fn wait_on_potential_prompts() {
    // id 3
    let replay = replay_game(
        3,
        "415pfcuklchpfmlvabnegnwfdrgaspqkswkxrtibaudvixohjuqym",
        "05pcvdajamadidaianubaeibao1cag7dicbaavaqvbobaxbl7abbahuabrvba1b00cbcafa77bbyazakap1bbC6daEbsa4DabuobaFbt1bbBa6odaJa86aqc",
        "0",
    );
    assert_ne!(
        replay.slot_perspectives(3, 3)[3].delayed,
        0,
        "Donald must wait on more round (to allow p1 or b1 to play)"
    );
}

// finess clues:

#[test]
#[ignore]
/// a nice 5-for-1 finess starting with a self-finess (all in order)
fn prefer_finess_over_delayed_play_clue() {
    // id 31554
    let replay = replay_game(
        6,
        "415tlxnrlfdxhwyrjckpmpkhifvvgckgmaunaisweausbpfoduqbq",
        "05obagDapbudaquaapabbe1dbnbc6cbjbsbtvaobbvazau1bby6carib7aada46ab0a57abxb3a8bfblb7bwiabkiaaFidbiaouc7daIbJ0db2b9aL0db67bqd",
        "0",
    );
    assert!(
        replay.clone().clue(0, game::Clue::Rank(4))
            > replay
                .clone()
                .clue(0, game::Clue::Color(game::ClueColor::Green()))
    );
}

#[test]
#[ignore]
fn layed_finess() {
    // https://hanab.live/replay-json/415gbbkuaxamlmfipdgchyaejwukhfvqlrpvnpkwsrfitcdqnusxo,05pbah0danocafalvavdae1dapaaoduaaradbgibamacaqbiavabwa,0
    // e.g. on r3 to get g1, r2 to play
    assert!(false);
}

#[test]
#[ignore]
fn layered_self_finess() {
    let mut replay = replay_game(
        0,
        "415cuiquxlflokyvgmkjardfprpwmdqnpabwvcbaxuftkhsehnsig",
        "05uc1dpbapvdahakibDcaraiao0daeajav7bbg0banadawbsam6dbziaaybba5Ddb0aaafubb46ca2alb6bcoabuDcaGataEwc",
        "0",
    );
    let score = replay
        .clue(3, game::Clue::Rank(3))
        .expect("is a valid clue");
    assert!(!score.has_errors(), "Valid layered finess clue");
    // bob does not plays finess (donald must reason is might not have y3, and must start with self-finess)
    replay.clue(0, game::Clue::Color(game::ClueColor::Purple()));
}

#[test]
#[ignore]
/// a nice 5-for-1 finess starting with a self-finess (all in order)
fn dont_force_players_into_chopping_critical_cards() {
    // id 31554
    let replay = replay_game(
        20,
        "415tdcbfjvfmhuixnglskwuqigaahmnlsyapqoxudrekvpfwkprcb",
        "05pbahDa6codaeakaoubar6b6cudagasapbb1caibm7bbfajubbcocbzpaaybu7bbt0ba6bwan6cb8a9vaadbBa4bxvd0c7aa7b5bqaC0aa3bEibbHaaaMb2bNqa",
        "0",
    );
    assert!(
        replay.clone().clue_is_bad(1, game::Clue::Rank(4)),
        "y5 is sacrifised"
    );
}

#[test]
#[ignore]
fn fix_pending_misplay() {
    // https://hanab.live/replay-json/415gbbkuaxamlmfipdgchyaejwukhfvqlrpvnpkwsrfitcdqnusxo,05pbah0danocafalvavdae1dapaaoduaaradbgibamacaqbiavabwa,0
    // turn
    assert!(false);
}

#[test]
fn reclue_for_finess() {
    let replay = replay_game(
        41,
        "415bjvhlakfiexaygtmfumnfoxrkakgvpudbcdicshlwpnqpurqws",
        "03lbahkdan1dagkaxcadafai1cabpaoboaaaaeblbpwdasbzaratsdpaa3acbqgbb0gda7b2a5bwlaavb8wc",
        "0",
    );
    let slots = replay.slot_perspectives(3, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Purple()
            }],
            "Player {player} missed a finess"
        );
    }
}

#[test]
#[ignore]
fn prompt_finess() {
    // prompt converts to finess after successful play
    let _replay = replay_game(
        6,
        "415xbpkdphugripkuqvmfsbaqtcvkecldlmfwgunajrwnsfyoaxih",
        "05pdvdalan0cvcodapob0dbkaovcocajauba0datasbbpdbqazicbea1awudvdbva21dbfiba7bca3iaa5a07aby1bbx7ab4vbbdbh1bbrpcDda6bm6bbBobaLbDaIudobqa",
        "0",
    );
}

#[test]
#[ignore]
/// cathy struggles with this clues (assumpts prompt over self-finess)
/// all 3s played except r3, g3;
/// r1 and g2 next cards to play
/// r2 clued as any two in a players hand
/// another player gets 3 clued (aka r3 or g3)
/// player sees the r1 is still missing (if it would be r3)
/// nothing else in finess somewhere => starts with self-finess to figure out which finess it is
fn play_or_prompt_starting_with_self_finess() {
    let mut replay = replay_game(
        28,
        "415uwcsgfqvfwbpdbhkgxevorthxuylkimrjqpncismaaupkdlnfa",
        "03lcwaalsbaaagaipcodxaatapbcsabjavadkcaqwcabbeaxkcarbfa3ta",
        "0",
    );
    let slots = replay.slot_perspectives(0, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert!(slot.play, "Player {player} missed a finess");
    }
    // check intermediate state
    replay.play(1, None);
    replay.print();
    let slots = replay.slot_perspectives(0, 0);
    for (player, slot) in slots.iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Green()
            }],
            "Player {player} missed a finess"
        );
    }
    for (player, slot) in replay.slot_perspectives(0, 0).iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Green()
            }],
            "Player {player} don't update its state after the happened finess"
        );
    }
    for (player, slot) in replay.slot_perspectives(2, 3).iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 2,
                suit: game::Suit::Red()
            }],
            "Player {player} forgot to restore r2"
        );
    }
}

// end game:

// example of pass clues around
// => https://hanab.live/shared-replay-json/415bjvhlakfiexaygtmf-umnfoxrkakgvpudbcdic-shlwpnqpurqws,03lbah-kdan1dagkaxcadafai1c-abpaoboaaaaeblbpwdas-bzaratsdpaa3acbqgbb0-gda7b2a5bwlaavb8wcbu-ajaEpdsaakaHaxbyxdam-sdsdsdaBb4b1bAao,0

// errors
// https://hanab.live/replay-json/415ckkvxmlauqnhrpdngaqfigfpbwdtrviywfebkjxucmlousphsa,056cpaaiubacaguaucibah7d6aadafbjpbbbatvbbp6dayblaz6baebqpaaxavbwiaaaasb1ibbr1db4a27bb9iaaoa86ab7ama50aoabnaJ1abAucaLbEaHoaaBwa,0

// https://hanab.live/replay-json/415kbauckvlorvqugibpamfxghwujrqxcdifmesnkhanytfdpsplw,05Dcpa6biaadag1b0aaq1cajvbacahbkpcabbfat6aaxaeblobaaavbwDcbs6ab27baua4b5iaa8ucbz1cb31aaCbm7bb96daF7dbrbEbn0daIwb,0
// 11 don't give self-finess that must be interpreted as direct play clues
// 13 clued 2 (g2) must wait for g1 to be played
// 31. give 4 instead of purple => identity clear
// 46 => really good self-finess, but not understood
// 49 give 2 blue instead of y4 (both 1-for-1, but allow blue to play
// 54 clue b2 instead of b4 (b2 is no longer on finess and it is clearly missed)

// 1204237994774607731
// https://hanab.live/replay-json/415fjpkhpkaaiyuuwbbsnsincduoxmaepqvrqtkdcwvhrxgllffmg,05pbahidap6cagaloaaaafibbm7davbiawbcbebjDc7cbqa27b0cbxa40c6ba5a60c6dasa8an6cb1bAazbdb3akocod7caHaEucoaaIbobu1aatuca0bGbNqd,0

// turn 3: don't bad clue r2 twice

// turn 29: finess

#[test]
// finess p2, p3 via p4 instead of cluing p2 directly (p1 already played)
fn prefer_finess_over_direct_play_clue() {
    // id 1204237994774607731
    let replay = replay_game(
        28,
        "415fjpkhpkaaiyuuwbbsnsincduoxmaepqvrqtkdcwvhrxgllffmg",
        "05pbahidap6cagaloaaaafibbm7davbiawbcbebjDc7cbqa27b0cbxa40c6ba5a60c6dasa8an6cb1bAazbdb3akocod7caHaEucoaaIbobu1aatuca0bGbNqd",
        "0",
    );
    assert!(
        replay.clone().clue(3, game::Clue::Rank(4))
            > replay
                .clone()
                .clue(1, game::Clue::Color(game::ClueColor::Purple()))
    );
}

#[test]
// finess p2, p3 via p4 instead of cluing p2 directly (p1 already played)
fn assume_good_touch_from_finesses() {
    // id 1204237994774607731
    let replay = replay_game(
        35,
        "415fjpkhpkaaiyuuwbbsnsincduoxmaepqvrqtkdcwvhrxgllffmg",
        "05pbahidap6cagaloaaaafibbm7davbiawbcbebjDc7cbqa27b0cbxa40c6da5a6an0casa8bo6c7dbBazbdb1akocod7caHaEucoaaIb9bu1aatuca0bGbNqd",
        "0",
    );
    for (player, slot) in replay.slot_perspectives(3, 1).iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 4,
                suit: game::Suit::Purple()
            },],
            "Player {player} should assume p4 via good touch principal"
        );
    }
}

#[test]
/// with all ones played and nothing else clued,
/// and a hand of [y2, xx, y3', xx]
/// use self-finess to get two cards to play instead of one with yellow only
fn prefer_self_finess_over_normal_play_clue() {
    // id 685
    let replay = replay_game(
        9,
        "415auulkldpfphycrnxwrfghivmocfqenaqwdambupkjskstbixgv",
        "05pbahpaocacaeai6aaaocatuaadocav6aawbfoabm6baqbj1abb0aa2a0wd",
        "0",
    );
    assert!(
        replay.clone().clue(2, game::Clue::Rank(3))
            > replay
                .clone()
                .clue(2, game::Clue::Color(game::ClueColor::Yellow()))
    );
}

#[test]
/// with all ones played and nothing else clued,
/// and a hand of [y2, xx, y3', xx]
/// use self-finess to get two cards to play instead of one with yellow only
fn dont_assume_self_prompt() {
    // id 685
    let replay = replay_game(
        13,
        "415auulkldpfphycrnxwrfghivmocfqenaqwdambupkjskstbixgv",
        "05pbahpaocacaeai6aaaocatuaadocav6aawbfoabm6baqbj1abb0aa2a0wd",
        "0",
    );
    let clue_4 = replay.clone().clue(2, game::Clue::Rank(4));
    let mut yellow_replay = replay.clone();
    let yellow_clue = yellow_replay.clue(2, game::Clue::Color(game::ClueColor::Yellow()));
    assert!(clue_4 > yellow_clue);
    for (player, slot) in yellow_replay.slot_perspectives(2, 0).iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![game::Card {
                rank: 3,
                suit: game::Suit::Yellow()
            },],
            "Player {player} assumpted something to complex"
        );
    }
}

#[test]
// waitplay callbacks must be removed if that card is chopped
fn flush_delay_waits_upon_discard() {
    // id 17001580704262382651
    let replay = replay_game(
        27,
        "415pqhrfftgbgjokakuablnusrvadxppdclwvimxsmqyekuihnfwc",
        "05pd0apbapaaaevaaoabpdvbaqicas1abnadah0davbuvdbjaxarvcbiDcby1ab20ba6ag7dbma4a9oaa3b16db5aAucbw6ab0aEicaFoaac7caKata8bHalbNqa",
        "0",
    );
    for (player, slot) in replay.slot_perspectives(0, 3).iter().enumerate() {
        assert_eq!(
            slot.delayed, 0,
            "Player {player} forgot to clear a delay note somewhere"
        );
    }
}

#[test]
// waitplay callbacks must be removed if that card is chopped
fn clear_alternative_finess_after_first_play() {
    // id 431
    let mut replay = replay_game(
        1,
        "415jqfgllkfugiwmiapvenxcdsbnfuatpqbwaxmpcushrodykhvkr",
        "05vcahva6cvbpdaiapadaeajaoabia1bicaxauavbmubagarDabcafbk6a7da5wb",
        "0",
    );
    replay.play(0, None);
    let y2 = game::Card {
        rank: 2,
        suit: game::Suit::Yellow(),
    };
    for (player, slot) in replay.slot_perspectives(2, 2).iter().enumerate() {
        assert_eq!(
            slot.quantum.iter().collect::<Vec<game::Card>>(),
            vec![y2],
            "Player {player} missed that the played finess clears the card identity"
        );
        assert_eq!(
            slot.delayed, 0,
            "Player {player} forgot to clear a delay note somewhere"
        );
    }
    for (player, line) in replay.lines.iter().enumerate() {
        assert!(
            line.card_states[&y2].locked.is_some() || line.card_states[&y2].clued.is_some(),
            "Player {player} missed the clearing finess play"
        );
    }
}

#[test]
// waitplay callbacks must be removed if that card is chopped
fn good_touch_principal3() {
    // id 431
    let mut replay = replay_game(
        2,
        "415jqfgllkfugiwmiapvenxcdsbnfuatpqbwaxmpcushrodykhvkr",
        "05vcahva6cvbpdaiapadaeajaoabia1bicaxauavbmubagarDabcafbk6a7da5wb",
        "0",
    );
    assert!(replay.clue_is_bad(0, game::Clue::Rank(2)));
}

#[test]
// play clue promises g1 and g2 (finess). the player has multiple ones it could be any of them
// g2 must only be played if g1 is found
fn move_promised_card_to_next_clued_card() {
    // id 6
    let replay = replay_game(
        4,
        "415tmxfdcqukafnuihnlbduplvafympcewopgrkvbjsihqkrsgxwa",
        "05pcDaak1a6bahaj1b7c7daqucbc0aaibmauucavboab6dbtapvbbrvdawbdaguab2a5afoda71dbe0aa4a8b3aswc",
        "0",
    );
    let slot = replay.slot_perspectives(2, 2)[2];
    assert_eq!(
        slot.quantum.iter().collect::<Vec<game::Card>>(),
        vec![game::Card {
            rank: 1,
            suit: game::Suit::Green(),
        }],
        "Player 2 missed that the finess promised g1 somewhere"
    );
    assert_eq!(
        slot.promised,
        Some(4),
        "Player 2 missed that they must continue to search for g1"
    );

    // play r1 thinking it is g1:
    let replay = replay_game(
        7,
        "415tmxfdcqukafnuihnlbduplvafympcewopgrkvbjsihqkrsgxwa",
        "05pcDaak1a6bahaj1b7c7daqucbc0aaibmauucavboab6dbtapvbbrvdawbdaguab2a5afoda71dbe0aa4a8b3aswc",
        "0",
    );
    let slot = replay.slot_perspectives(2, 3)[2];
    assert_eq!(
        slot.quantum.iter().collect::<Vec<game::Card>>(),
        vec![game::Card {
            rank: 1,
            suit: game::Suit::Green(),
        }],
        "Player 2 missed that the finess promised g1 somewhere"
    );
    assert_eq!(
        slot.promised,
        Some(4),
        "Player 2 missed that they must continue to search for g1"
    );
}

#[test]
// get [p3 xx p3 xx] to play by recluing p4 in other hand to initiate a finess
// otherwise it would be a bad touch clue.
fn finess_over_direct_clue_with_bad_touch() {
    // id 17
    let replay = replay_game(
        21,
        "415xvpskfqgqixmgddhawafusoplmnwfchrcprbltubjeiayunkvk",
        "05vbafpbpaacaq1d0bocahbiappcagauvabaaeaj7cab1ablDba1bsak6aayawbx1cbrida8bma90cb51aa6bt0davb3oab2aBaE6caIaoibaFuaaMaKwa",
        "0",
    );
    let p4_play = replay
        .clone()
        .clue(2, game::Clue::Color(game::ClueColor::Purple()));
    let p3_clue = replay.clone().clue(0, game::Clue::Rank(3));
    assert!(p4_play > p3_clue);
}

#[test]
// prefer finess to get g2, g3, g4, g5 to play instead of safe (that can be done by a later person)
fn long_finess_row_over_safe() {
    // id 17
    let replay = replay_game(
        19,
        "415xvpskfqgqixmgddhawafusoplmnwfchrcprbltubjeiayunkvk",
        "05vbafpbpaacaq1d0bocahbiappcagauvabaaeaj7cab1ablDba1bsak6aayawbx1cbrida8bma90cb51aa6bt0davb3oab2aBaE6caIaoibaFuaaMaKwa",
        "0",
    );
    let p4_safe = replay.clone().clue(2, game::Clue::Rank(4));
    let g_play = replay.clone().clue(1, game::Clue::Rank(5));
    assert!(g_play > p4_safe);
}

#[test]
// if you are about to play finess but the play before you strikes playing their finess position
// remove all finess markers
// assumes staling is not yet implemented
fn stop_finess_plays_upon_first_strike() {
    // id 5
    let mut replay = replay_game(
        46,
        "415vlqgwdnadxyeuravxhfqtnpciabfujgfocpmlbhssupimkkrwk",
        "05pdva6bao7dbfbiamaaaeDaaq1doaasawadvaob1bacarbj6cbzDaakaya3iaDabna0idb2a7b8bgblbpbBbh7bbtbFuaaDaHaIwa",
        "0",
    );
    let g1 = game::Card {
        rank: 1,
        suit: game::Suit::Green(),
    };
    // every player expects a self-finess on g1 or marks the g3 as g1 to play
    assert_eq!(
        replay.lines[0].hands.slot(0, 0).quantum.to_vec(),
        vec![g1],
        "Alice: Assuming no staling, g1 is promosed"
    );
    assert_eq!(
        replay.lines[1].hands.slot(3, 0).quantum.to_vec(),
        vec![g1],
        "Bob: Assuming no staling, g1 is promosed"
    );
    assert_eq!(
        replay.lines[2].hands.slot(0, 0).quantum.to_vec(),
        vec![g1],
        "Cathy: Assuming no staling, g1 is promosed"
    );
    assert!(
        replay.lines[2].hands.slot(0, 0).promised.is_some(),
        "Cathy: assuming no staling, g1 via finess is promised"
    );
    assert_eq!(
        replay.lines[3].hands.slot(0, 0).quantum.to_vec(),
        vec![g1],
        "Donald: Assuming no staling, g1 is promosed"
    );
    assert!(
        replay.lines[3].hands.slot(0, 0).promised.is_some(),
        "Donald: assuming no staling, g1 via finess is promised"
    );
    replay.play(0, None);
    // alice no longer expects a g1:
    assert_ne!(
        replay.lines[0].hands.slot(0, 0).quantum.to_vec(),
        vec![g1],
        "Alice: only g1 could be misplayed => card is not on our hand"
    );
    // // bob no longer expects alice to play g1:
    // assert_ne!(
    //     replay.lines[1].hands.slot(3, 0).quantum.to_vec(),
    //     vec![g1],
    //     "Bob: only g1 could be misplayed => card is not on our hand"
    // );

    // donald clear its finess information:
    assert_ne!(
        replay.lines[3].hands.slot(0, 0).quantum.to_vec(),
        vec![g1],
        "Donald: only g1 could be misplayed => card is not on our hand"
    );
    assert!(
        replay.lines[3].hands.slot(0, 0).promised.is_none(),
        "Donald: only g1 could be misplayed => card is not on our hand"
    );
}

#[test]
fn never_extend_hard_quantum_via_callbacks() {
    let replay = replay_game(
        12,
        "415cuiquxlflokyvgmkjardfprpwmdqnpabwvcbaxuftkhsehnsig",
        "05uc1dpbapvdahakibDcaraiao0daeajav7bbg0banadawbsam6dbziaaybba5Ddb0aaafubb46ca2alb6bcoabuDcaGataEwc",
        "0",
    );
    for (player, slot) in replay.slot_perspectives(2, 3).iter().enumerate() {
        assert_eq!(
            slot.quantum.hard_size(),
            1, // g5
            "Player {player} wrongly extended the hard quantum"
        );
        assert!(!slot.quantum.contains(&game::Card {
            rank: 4,
            suit: game::Suit::Green(),
        }));
        assert!(!slot.quantum.contains_hard(&game::Card {
            rank: 4,
            suit: game::Suit::Green(),
        }));
    }
}
#[test]
fn never_extend_hard_quantum_via_callbacks2() {
    let mut replay = replay_game(
        43,
        "415cuiquxlflokyvgmkjardfprpwmdqnpabwvcbaxuftkhsehnsig",
        "05uc1dpbapvdahakibDcaraiao0daeajav7bbg0banadawbsam6dbziaaybba5Ddb0aaafubb46ca2alb6bcoabuDcaGataEwc",
        "0",
    );
    assert!(
        replay.clue_is_bad(2, game::Clue::Rank(5)),
        "Cathy must expect r5 (and not a self-finess)"
    );
}

#[test]
fn never_extend_hard_quantum_via_callbacks3() {
    let replay = replay_game(
        4,
        "415pfcuklchpfmlvabnegnwfdrgaspqkswkxrtibaudvixohjuqym",
        "05pcvdajubadae0aamid6dobatarah1banbbvcbiao0cagbka0aaasalaxwd",
        "0",
    );
    for (player, slot) in replay.slot_perspectives(0, 0).iter().enumerate() {
        assert_eq!(
            slot.quantum.to_vec(),
            vec![game::Card {
                rank: 1,
                suit: game::Suit::Purple(),
            }],
            "Player {player} should assume finess"
        );
    }
}

#[test]
#[ignore]
fn stop_finess_plays_after_prompt_misplays() {
    let _replay = replay_game(
        19,
        "415jubvhmpdrukqrpgdagafoptaelmbxkfqxcwfinhnucivyswksl",
        "05pcvaakia0bagajicadodas1batvcalaobaae7dbmvbazbivbbba1aqucaya4wb",
        "0",
    );
    // clue on red 4 went side-ways (the clue-giver assumpted goood touch to have r3)
    assert!(false);
}

#[test]
fn update_promise_from_prompt_to_finess() {
    let mut replay = replay_game(
        6,
        "415jubvhmpdrukqrpgdagafoptaelmbxkfqxcwfinhnucivyswksl",
        "05pcvaakia0bagajicadodas1batvcalaobaae7dbmvbazbivbbba1aqucaya4wb",
        "0",
    );
    let r1 = game::Card {
        rank: 1,
        suit: game::Suit::Red(),
    };
    // cathy can assume r1 is prompted (instead of the finess as it is actually is)
    // (later with more context she *might* directly play finess, as p1 is already promised)
    assert_eq!(
        replay.lines[2].hands.slot(0, 2).quantum.to_vec(),
        vec![r1],
        "Cathy should assume finess"
    );
    replay.play(2, Some(r1));
    // clue on red 4 went side-ways (the clue-giver assumpted goood touch to have r3)
    for (player, slot) in replay.slot_perspectives(2, 1).iter().enumerate() {
        assert_eq!(
            slot.quantum.to_vec(),
            vec![r1],
            "Player {player} should assume finess"
        );
    }
}

#[test]
fn considered_common_finess_cards_clued() {
    let mut replay = replay_game(
        8,
        "415jubvhmpdrukqrpgdagafoptaelmbxkfqxcwfinhnucivyswksl",
        "05pcvaakia0bagajicadodas1batvcalaobaae7dbmvbazbivbbba1aqucaya4wb",
        "0",
    );
    assert!(
        replay.clue_is_bad(2, game::Clue::Color(game::ClueColor::Red())),
        "r1 already finessed; bad clue recluing it"
    );
}
