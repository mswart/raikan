use std::collections::BTreeMap;
use std::collections::VecDeque;

use colored::*;
use rand::seq::SliceRandom;
use rand::thread_rng;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suite {
    Red(),
    Green(),
    Yellow(),
    Blue(),
    Purple(),
}

impl Suite {
    fn color(&self) -> Color {
        match self {
            Self::Red() => Color::Red,
            Self::Green() => Color::Green,
            Self::Yellow() => Color::Yellow,
            Self::Blue() => Color::BrightBlue,
            Self::Purple() => Color::Magenta,
        }
    }

    fn char(&self) -> char {
        match self {
            Self::Red() => 'r',
            Self::Green() => 'g',
            Self::Yellow() => 'y',
            Self::Blue() => 'b',
            Self::Purple() => 'p',
        }
    }

    fn card_count(&self, rank: u8) -> u8 {
        match rank {
            1 => 3,
            5 => 1,
            _ => 2,
        }
    }

    fn affected(&self, rank: u8, clue: Clue) -> bool {
        match clue {
            Clue::Rank(clue_rank) => rank == clue_rank,
            Clue::Color(clue_color) => match (self, clue_color) {
                (Self::Red(), ClueColor::Red()) => true,
                (Self::Blue(), ClueColor::Blue()) => true,
                (Self::Yellow(), ClueColor::Yellow()) => true,
                (Self::Green(), ClueColor::Green()) => true,
                (Self::Purple(), ClueColor::Purple()) => true,
                _ => false,
            },
        }
    }
}

impl std::fmt::Display for Suite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.char().to_string().color(self.color()).fmt(f)
    }
}

impl std::fmt::Debug for Suite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Card {
    pub suite: Suite,
    pub rank: u8,
}

impl Card {
    fn affected(&self, clue: Clue) -> bool {
        self.suite.affected(self.rank, clue)
    }
    pub fn play_state(&self, game: &Game) -> CardPlayState {
        match self.rank as i8 - game.played_rank(&self.suite) as i8 {
            diff if diff < 0 => CardPlayState::Trash(),
            0 => CardPlayState::Playable(),
            _ => match self.suite.card_count(self.rank) - game.discarded.get(self).unwrap_or(&0) {
                0 => CardPlayState::Dead(),
                1 => CardPlayState::Critical(),
                _ => CardPlayState::Normal(),
            },
        }
    }
}

#[derive(Debug)]
pub struct CardState {
    card: Card,
    clues: Vec<Clue>,
    excluded: Vec<Clue>,
}

pub enum CardPlayState {
    Dead(),
    Playable(),
    Critical(),
    Normal(),
    Trash(),
}

impl std::fmt::Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            format!("{}{}", self.suite.char(), self.rank).color(self.suite.color())
        )
    }
}

struct Player<'a> {
    cards: VecDeque<CardState>,
    strategy: &'a dyn PlayerStrategy,
}

#[derive(Debug)]
pub enum GameState {
    Early(),
    Mid(),
    Final(u8),
    Lost(),
    Won(),
    Finished(),
    Invalid(),
}

pub struct Game<'a> {
    pub suites: Vec<Suite>,
    pub score: u8,
    pub max_score: u8,
    pub turn: u8,
    pub discarded: BTreeMap<Card, u8>,
    pub played: Vec<u8>,
    pub num_strikes: u8,
    pub clues: u8,
    deck: VecDeque<Card>,
    players: Vec<Player<'a>>,
    active_player: usize,
    state: GameState,
}

#[derive(Clone, Copy, Debug)]
pub enum ClueColor {
    Red(),
    Green(),
    Yellow(),
    Blue(),
    Purple(),
}

#[derive(Debug, Clone, Copy)]
pub enum Clue {
    Color(ClueColor),
    Rank(u8),
}

#[derive(Debug)]
pub enum Move {
    Discard(u8),
    Play(u8),
    Clue(u8, Clue),
}

pub trait PlayerStrategy {
    fn act(&self, game: &Game) -> Move;
}

impl<'a> Game<'a> {
    pub fn new(players: Vec<&'a dyn PlayerStrategy>) -> Self {
        let suites = vec![
            Suite::Red(),
            Suite::Green(),
            Suite::Yellow(),
            Suite::Blue(),
            Suite::Purple(),
        ];

        let mut rng = thread_rng();
        let mut deck = Vec::with_capacity(10 * suites.len());
        for suite in suites.iter() {
            for rank in 1..=5 {
                for _count in 0..suite.card_count(rank) {
                    deck.push(Card {
                        suite: suite.clone(),
                        rank: rank,
                    });
                }
            }
        }
        deck.shuffle(&mut rng);

        let mut player_states = Vec::new();
        let num_cards = match players.len() {
            2 => 5,
            3 => 5,
            4 => 4,
            5 => 5,
            6 => 3,
            _ => unimplemented!(),
        };
        for strategy in players.iter() {
            let mut player = Player {
                cards: VecDeque::with_capacity(num_cards),
                strategy: *strategy,
            };
            for _ in 0..num_cards {
                player.cards.push_back(CardState {
                    card: deck.pop().expect("Deck is full"),
                    clues: Vec::new(),
                    excluded: Vec::new(),
                });
            }
            player_states.push(player);
        }

        Self {
            score: 0,
            max_score: 5 * suites.len() as u8,
            turn: 0,
            deck: deck.into(),
            discarded: BTreeMap::new(),
            played: vec![0; suites.len()],
            players: player_states,
            num_strikes: 0,
            suites,
            active_player: 0,
            clues: 8,
            state: GameState::Early(),
        }
    }

    pub fn num_players(&self) -> u8 {
        self.players.len() as u8
    }

    pub fn num_hand_cards(&self, player: usize) -> u8 {
        self.players[(self.active_player + player) % self.players.len()]
            .cards
            .len() as u8
    }

    pub fn dump(&self) {
        println!("Game:");
        println!(
            "  suites={:?} turn={} score={}/{} strikes={} clues={} state={:?}",
            self.suites,
            self.turn,
            self.score,
            self.max_score,
            self.num_strikes,
            self.clues,
            self.state,
        );
        print!("  played:");
        for (pos, suite) in self.suites.iter().enumerate() {
            print!(" {}={}", suite, self.played[pos]);
        }
        println!("");
        println!("  discarded: {:?}", self.discarded);

        // println!("  players:");
        for player in self.players.iter() {
            println!("  player {:?}", player.cards);
        }

        println!("  deck: {:?}", self.deck);
    }

    pub fn run(&mut self, debug: bool) -> u8 {
        if debug {
            self.dump();
        }
        loop {
            match self.state {
                GameState::Early() => {
                    self.play();
                }
                GameState::Mid() => {
                    self.play();
                }
                GameState::Final(0) => self.state = GameState::Finished(),
                GameState::Final(remaining) => {
                    self.play();
                    self.state = GameState::Final(remaining - 1)
                }
                _ => break,
            }
            if debug {
                self.dump();
            }
        }
        self.score
    }

    fn played_rank(&self, suite: &Suite) -> u8 {
        for (pos, current_suite) in self.suites.iter().enumerate() {
            if current_suite == suite {
                return self.played[pos];
            }
        }
        return 0;
    }

    fn discard(&mut self, card: Card) {
        let count = *self
            .discarded
            .entry(card)
            .and_modify(|e| *e += 1)
            .or_insert(1);
        if count == card.suite.card_count(card.rank) {
            // a card is lost -> updated maximal possible score based on remaining cards
            self.update_max_score();
        }
    }

    fn draw_card(&mut self) {
        if let Some(card) = self.deck.pop_front() {
            self.players[self.active_player]
                .cards
                .push_front(CardState {
                    card,
                    clues: Vec::new(),
                    excluded: Vec::new(),
                });
            if self.deck.len() == 0 {
                self.state = GameState::Final(self.players.len() as u8);
            }
        }
    }

    pub fn card_cluded(&self, pos: u8, player: usize) -> bool {
        self.players[(self.active_player + player) % self.players.len()].cards[pos as usize]
            .clues
            .len()
            > 0
    }

    pub fn player_card(&self, pos: u8, player: usize) -> Card {
        assert!(player > 0, "Own cards are unknown");
        self.players[(self.active_player + player) % self.players.len()].cards[pos as usize].card
    }

    fn update_max_score(&mut self) {
        self.max_score = 0;
        for suite in self.suites.iter() {
            for rank in 1..=5 {
                let card = Card {
                    suite: *suite,
                    rank,
                };
                match card.play_state(self) {
                    CardPlayState::Dead() => break,
                    _ => self.max_score += 1,
                }
            }
        }
    }

    fn play(&mut self) {
        let action = self.players[self.active_player].strategy.act(&self);
        self.turn += 1;
        match action {
            Move::Discard(pos) => {
                let card = self.players[self.active_player].cards.remove(pos as usize);
                if card.is_none() {
                    println!(
                        "Invalid move: player {} tried to discard card {} (hand only has {} cards)",
                        self.active_player,
                        pos,
                        self.players[self.active_player].cards.len()
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                let card = card.unwrap();
                println!(
                    "Player {} discarded {:?} from pos {}",
                    self.active_player, card, pos
                );
                self.discard(card.card);
                if let GameState::Early() = self.state {
                    self.state = GameState::Mid();
                }
                if self.clues < 8 {
                    self.clues += 1;
                }
                self.draw_card();
            }
            Move::Play(pos) => {
                let card = self.players[self.active_player].cards.remove(pos as usize);
                if card.is_none() {
                    println!(
                        "Invalid move: player {} tried to play card {} (hand only has {} cards)",
                        self.active_player,
                        pos,
                        self.players[self.active_player].cards.len()
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                let card = card.unwrap();
                if self.played_rank(&card.card.suite) + 1 == card.card.rank {
                    println!(
                        "Player {} played successfully {:?} from pos {}",
                        self.active_player, card, pos
                    );
                    for (pos, current_suite) in self.suites.iter().enumerate() {
                        if *current_suite == card.card.suite {
                            self.played[pos] += 1;
                        }
                    }
                    self.score += 1;
                    if card.card.rank == 5 && self.clues < 8 {
                        self.clues += 1;
                    }
                    if self.score as usize == self.suites.len() * 5 {
                        self.state = GameState::Won();
                    }
                } else {
                    println!(
                        "Player {} failed to played {:?} from pos {}",
                        self.active_player, card, pos
                    );
                    self.discard(card.card);
                    self.num_strikes += 1;
                    if self.num_strikes == 3 {
                        self.state = GameState::Lost();
                        println!("Game lost due to three strikes");
                    }
                }
                self.draw_card();
            }
            Move::Clue(player, clue) => {
                if player >= self.players.len() as u8 || player == 0 {
                    println!(
                        "Invalid move: player {} tried to clue to invalid player number {}",
                        self.active_player, player,
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                if self.clues == 0 {
                    println!(
                        "Invalid move: player {} tried to clue but no clue tokens are left",
                        self.active_player,
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                let len = self.players.len();
                let mut affected_cards = 0;
                for card_state in self.players[(self.active_player + player as usize) % len]
                    .cards
                    .iter_mut()
                {
                    let affected = card_state.card.affected(clue);
                    if affected {
                        card_state.clues.push(clue.clone());
                        affected_cards += 1;
                    } else {
                        card_state.excluded.push(clue.clone());
                    }
                    println!("{:?}", card_state);
                }
                println!(
                    "Player {} clue played {} about {} {:?} cards",
                    self.active_player,
                    (self.active_player + player as usize) % len,
                    affected_cards,
                    clue
                );
                self.clues -= 1;
            }
        }
        self.active_player = (self.active_player + 1) % self.players.len();
    }
}
