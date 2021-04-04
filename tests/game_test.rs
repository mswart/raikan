use hanabi;

mod tester;

#[test]
fn initial_game() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let game = hanabi::game::Game::new(&mut players, false);
    assert_eq!(game.state, hanabi::game::GameState::Early());
    assert_eq!(game.score, 0);
    assert_eq!(game.max_score, 25);
}

#[test]
fn discard_game() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let mut game = hanabi::game::Game::new(&mut players, false);
    assert_eq!(game.run(&mut players), 0);
    assert_ne!(game.max_score, 25); // most cards should have been discarded
    assert_eq!(game.state, hanabi::game::GameState::Finished());
}

#[test]
fn striked_game() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Play(0));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Play(0));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let mut game = hanabi::game::Game::new(&mut players, false);
    game.run(&mut players);
    assert_ne!(game.score, 25); // most cards should have been discarded
    assert_eq!(game.state, hanabi::game::GameState::Lost());
}

#[test]
fn too_many_clues() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Clue(
        1,
        hanabi::game::Clue::Rank(1),
    ));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Clue(
        1,
        hanabi::game::Clue::Rank(1),
    ));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let mut game = hanabi::game::Game::new(&mut players, false);
    assert_eq!(game.run(&mut players), 0);
    assert_eq!(game.score, 0);
    assert_eq!(game.max_score, 25);
    assert_eq!(game.state, hanabi::game::GameState::Invalid());
}

#[test]
fn unknown_clued_player() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));
    tester1.add(hanabi::game::Move::Clue(2, hanabi::game::Clue::Rank(1)));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let mut game = hanabi::game::Game::new(&mut players, false);
    assert_eq!(game.run(&mut players), 0);
    assert_eq!(game.score, 0);
    assert_eq!(game.max_score, 25);
    assert_eq!(game.state, hanabi::game::GameState::Invalid());
}

#[test]
fn clued_self() {
    let mut tester1 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));
    tester1.add(hanabi::game::Move::Clue(0, hanabi::game::Clue::Rank(1)));
    let mut tester2 = tester::InstructedPlayer::with_default(hanabi::game::Move::Discard(0));

    let mut players: Vec<&mut dyn hanabi::game::PlayerStrategy> = Vec::new();
    players.push(&mut tester1);
    players.push(&mut tester2);

    let mut game = hanabi::game::Game::new(&mut players, false);
    assert_eq!(game.run(&mut players), 0);
    assert_eq!(game.score, 0);
    assert_eq!(game.max_score, 25);
    assert_eq!(game.state, hanabi::game::GameState::Invalid());
}
