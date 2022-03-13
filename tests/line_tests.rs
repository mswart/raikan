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

struct Replay {
    pub line: hyphenated::Line,
    pub lines: [hyphenated::Line; 4],
}

fn replay_game(turn: u8, deck: &str, actions: &str, options: &str) -> Replay {
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

    Game::from_replay(turn, deck, actions, options, &mut players);

    let target_player = (turn) % num_players;
    println!("target player: {target_player}");
    let lines = [h1.line(), h2.line(), h3.line(), h4.line()];
    let line = &lines[target_player as usize];
    println!("Start lines:\n---");
    for player in 0..4 {
        println!("Player {player}");
        for j in 0..4 {
            println!("- P{j} {:?}", lines[j].hands[(4 + player - j) % 4]);
        }
    }
    println!("Clued cards:");
    for line in lines.iter() {
        println!(" - {:?}", line.card_states);
    }
    Replay {
        line: line.clone(),
        lines,
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
    for (player, hand) in line.hands.iter().enumerate() {
        for slot in hand.iter() {
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
    for (player, hand) in line.hands.iter().enumerate() {
        for slot in hand.iter() {
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

    for (player, hand) in line.hands.iter().enumerate() {
        for slot in hand.iter() {
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
    line.clued(
        2,
        3,
        game::Clue::Rank(1),
        PositionSet::create(4, 0b1100),
        PositionSet::new(4),
    );

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
        34,
        "415lwngfqylimrcnbjifgqmfcusxawbusopkktuhekpahvpxdrvad",
        "05obaeoabmadbfDbbnDdbhuabpoc6dbjawbbia0abraziabkbua1ua1bby0davuba56da60ab7asoaoavaaADaaiaoaaat7ab9ac6dblaGa4id1aaJaKbqDbbDaB0dqc",
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
fn clue_multiple_ones2() {
    // seed 278
    let line = replay_game(
        1,
        "415isapgyqxplkqmbsktuwhivnexfncwdgvrajfpahfurdlmcboku",
        "05uc0cakiaacidaianvc6aalDcar6aajbmav6casbo6bahbuibbaa1iabpa3af1aoca6beazbtDdDdbwaxudbgvbb5bba4b21cocb7aIa0udb9aCaLbdbEbqqd",
        "0",
    ).line;

    assert!(
        clue(&line, 3, game::Clue::Rank(1)) > clue(&line, 1, game::Clue::Color(ClueColor::Blue()))
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
#[ignore]
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
    let own_hand = &line.hands[0];
    assert!(
        own_hand[0].trash,
        "Slot 0 should be trash (quantum: {})",
        own_hand[0].quantum
    );
    assert!(
        own_hand[1].trash,
        "Slot 1 should be trash (quantum: {})",
        own_hand[1].quantum
    );
    assert!(
        own_hand[2].play,
        "Slot 2 (previously chop), should be playable b4 (quantum: {})",
        own_hand[2].quantum
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
    let own_hand = &line.hands[0];
    assert!(
        own_hand[1].trash,
        "Slot 0 should be trash (quantum: {})",
        own_hand[1].quantum
    );
    assert!(
        !own_hand[2].trash,
        "Slot 2 (previously chop), should be playable b4 (quantum: {})",
        own_hand[2].quantum
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
fn no_double_cluing() {
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
    let own_hand = &line.hands[0];
    assert!(
        !own_hand[0].trash,
        "Slot 0 should not be trash (quantum: {})",
        own_hand[0].quantum
    );
    assert!(
        own_hand[0].play,
        "Slot 0 should be playable b5 (quantum: {})",
        own_hand[0].quantum
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
    let own_hand = &line.hands[0];
    assert!(
        own_hand[2].play,
        "Slot 2 is playable (card is clearly g4): quantum: {}",
        own_hand[2].quantum
    );
    assert!(
        own_hand[3].play,
        "Slot 3 is playable (card is clearly y4): quantum: {}",
        own_hand[3].quantum
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
    let target_hand = &line.hands[1];
    assert!(
        target_hand[0].play,
        "Slot 1 is playable (card is a one): quantum: {}",
        target_hand[0].quantum
    );
    assert!(
        target_hand[1].play,
        "Slot 2 is playable (card is a one): quantum: {}",
        target_hand[1].quantum
    );
    assert!(
        target_hand[3].play,
        "Slot 4 is playable (card is a one): quantum: {}",
        target_hand[3].quantum
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
    assert_eq!(line.hands[3][2].quantum.size(), 1);
    assert_eq!(
        line.hands[3][2].quantum.iter().nth(0).expect("size is 1"),
        game::Card {
            rank: 2,
            suit: game::Suit::Blue()
        }
    );
    line.clue(3, game::Clue::Color(game::ClueColor::Yellow()));
    println!("=> {:?}", line);
    assert_eq!(line.hands[3][2].quantum.size(), 1);
    assert!(line.hands[3][2].fixed);
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
    assert!(line.hands[0][0].play, "p1 must be marked as playable");
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
    let slot = &line.hands[0][3];
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
        line.hands[0][2].trash, line.hands[0][3].trash,
        "Only one g4 is needed, discard the other one"
    );
}
