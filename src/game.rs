use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};

// mod position_set;

// use crate::game::{self, CardPlayState};

// use crate::position_set;
pub use crate::position_set::PositionSet;

use colored::*;
use rand::prelude::*;
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

    pub fn clue_color(&self) -> ClueColor {
        match self {
            Self::Red() => ClueColor::Red(),
            Self::Blue() => ClueColor::Blue(),
            Self::Yellow() => ClueColor::Yellow(),
            Self::Green() => ClueColor::Green(),
            Self::Purple() => ClueColor::Purple(),
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

#[cfg(test)]
mod suite_tests {
    use super::*;

    #[test]
    fn card_counts() {
        for suite in [Suite::Blue(), Suite::Green()].iter() {
            assert_eq!(suite.card_count(1), 3);
            assert_eq!(suite.card_count(2), 2);
            assert_eq!(suite.card_count(3), 2);
            assert_eq!(suite.card_count(4), 2);
            assert_eq!(suite.card_count(5), 1);
        }
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
        if self.rank > game.max_rank_for_suite(self.suite) {
            return CardPlayState::Dead();
        }
        match self.rank as i8 - game.played_rank(&self.suite) as i8 {
            diff if diff <= 0 => CardPlayState::Trash(),
            1 => CardPlayState::Playable(),
            _ => match self.suite.card_count(self.rank) - game.discarded.get(self).unwrap_or(&0) {
                0 => CardPlayState::Dead(),
                1 => CardPlayState::Critical(),
                _ => CardPlayState::Normal(),
            },
        }
    }
}

pub struct CardState {
    card: Card,
    clues: BTreeSet<Clue>,
    excluded: BTreeSet<Clue>,
    potential_ranks: u8,
    potential_suites: u8,
}

impl CardState {
    fn from_card(card: Card) -> Self {
        Self {
            card,
            clues: BTreeSet::new(),
            excluded: BTreeSet::new(),
            potential_ranks: 0b11111,
            potential_suites: 0b11111,
        }
    }

    fn clue(&mut self, clue: Clue) -> bool {
        let affected = self.card.affected(clue);
        match clue {
            Clue::Rank(rank) => {
                if affected {
                    self.potential_ranks &= 1 << (rank - 1);
                } else {
                    self.potential_ranks ^= (self.potential_ranks) & (1 << rank - 1);
                }
            }
            Clue::Color(color) => {
                let suite_bit = match color {
                    ClueColor::Red() => 1,
                    ClueColor::Yellow() => 2,
                    ClueColor::Green() => 4,
                    ClueColor::Blue() => 8,
                    ClueColor::Purple() => 16,
                };
                if affected {
                    self.potential_suites = suite_bit
                } else {
                    self.potential_suites ^= (self.potential_suites) & suite_bit;
                }
            }
        }
        if affected {
            self.clues.insert(clue.clone());
            true
        } else {
            self.excluded.insert(clue.clone());
            false
        }
    }
}

impl std::fmt::Debug for CardState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} {}({}{}{}{}{} {}{}{}{}{})",
            self.card,
            if self.clues.len() > 0 { "*" } else { " " },
            if self.potential_ranks & 1 > 0 {
                "1".bold()
            } else {
                "1".strikethrough()
            },
            if self.potential_ranks & 2 > 0 {
                "2".bold()
            } else {
                "2".strikethrough()
            },
            if self.potential_ranks & 4 > 0 {
                "3".bold()
            } else {
                "3".strikethrough()
            },
            if self.potential_ranks & 8 > 0 {
                "4".bold()
            } else {
                "4".strikethrough()
            },
            if self.potential_ranks & 16 > 0 {
                "5".bold()
            } else {
                "5".strikethrough()
            },
            if self.potential_suites & 1 > 0 {
                Suite::Red()
                    .char()
                    .to_string()
                    .color(Suite::Red().color())
                    .bold()
            } else {
                Suite::Red()
                    .char()
                    .to_string()
                    .color(Suite::Red().color())
                    .strikethrough()
            },
            if self.potential_suites & 2 > 0 {
                Suite::Yellow()
                    .char()
                    .to_string()
                    .color(Suite::Yellow().color())
                    .bold()
            } else {
                Suite::Yellow()
                    .char()
                    .to_string()
                    .color(Suite::Yellow().color())
                    .strikethrough()
            },
            if self.potential_suites & 4 > 0 {
                Suite::Green()
                    .char()
                    .to_string()
                    .color(Suite::Green().color())
                    .bold()
            } else {
                Suite::Green()
                    .char()
                    .to_string()
                    .color(Suite::Green().color())
                    .strikethrough()
            },
            if self.potential_suites & 8 > 0 {
                Suite::Blue()
                    .char()
                    .to_string()
                    .color(Suite::Blue().color())
                    .bold()
            } else {
                Suite::Blue()
                    .char()
                    .to_string()
                    .color(Suite::Blue().color())
                    .strikethrough()
            },
            if self.potential_suites & 16 > 0 {
                Suite::Purple()
                    .char()
                    .to_string()
                    .color(Suite::Purple().color())
                    .bold()
            } else {
                Suite::Purple()
                    .char()
                    .to_string()
                    .color(Suite::Purple().color())
                    .strikethrough()
            },
        )
    }
}

#[derive(Debug)]
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

#[derive(Debug, PartialEq, Eq)]
pub enum GameState {
    Early(),
    Mid(),
    Final(u8),
    Lost(),
    Won(),
    Finished(),
    Invalid(),
}

type Hand = VecDeque<CardState>;

pub struct Game {
    pub suites: Vec<Suite>,
    pub score: u8,
    pub max_score: u8,
    pub turn: u8,
    pub discarded: BTreeMap<Card, u8>,
    pub played: Vec<u8>,
    pub num_strikes: u8,
    pub clues: u8,
    deck: VecDeque<Card>,
    hands: Vec<Hand>,
    active_player: usize,
    pub state: GameState,
    debug: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClueColor {
    Red(),
    Green(),
    Yellow(),
    Blue(),
    Purple(),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Clue {
    Color(ClueColor),
    Rank(u8),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Move {
    Discard(u8),
    Play(u8),
    Clue(u8, Clue),
}

pub trait PlayerStrategy {
    fn init(&mut self, game: &Game);
    fn clued(
        &mut self,
        clue: Clue,
        touched: PositionSet,
        previously_clued: PositionSet,
        game: &Game,
    );
    fn act(&mut self, game: &Game) -> Move;
}

impl Game {
    pub fn new(players: &mut Vec<&mut dyn PlayerStrategy>, debug: bool) -> Self {
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

        let mut hands = Vec::new();
        let num_cards = match players.len() {
            2 => 5,
            3 => 5,
            4 => 4,
            5 => 5,
            6 => 3,
            _ => unimplemented!(),
        };
        for _ in players.iter() {
            let mut hand = Hand::with_capacity(num_cards);
            for _ in 0..num_cards {
                hand.push_back(CardState::from_card(deck.pop().expect("Deck is full")));
            }
            hands.push(hand);
        }

        let game = Self {
            score: 0,
            max_score: 5 * suites.len() as u8,
            turn: 0,
            deck: deck.into(),
            discarded: BTreeMap::new(),
            played: vec![0; suites.len()],
            hands,
            num_strikes: 0,
            suites,
            active_player: 0,
            clues: 8,
            state: GameState::Early(),
            debug,
        };
        for strategy in players.iter_mut() {
            strategy.init(&game);
        }
        game
    }

    pub fn num_players(&self) -> u8 {
        self.hands.len() as u8
    }

    pub fn num_hand_cards(&self, player: usize) -> u8 {
        self.hands[(self.active_player + player) % self.hands.len()].len() as u8
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

        for hand in self.hands.iter() {
            println!("  hand {:?}", hand);
        }

        println!("  deck: {:?}", self.deck);
    }

    pub fn run(&mut self, strategies: &mut Vec<&mut dyn PlayerStrategy>) -> u8 {
        if self.debug {
            self.dump();
        }
        loop {
            match self.state {
                GameState::Early() => {
                    self.play(strategies);
                }
                GameState::Mid() => {
                    self.play(strategies);
                }
                GameState::Final(0) => self.state = GameState::Finished(),
                GameState::Final(remaining) => {
                    self.play(strategies);
                    self.state = GameState::Final(remaining - 1)
                }
                _ => break,
            }
            if self.debug {
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
            self.hands[self.active_player].push_front(CardState::from_card(card));
            if self.deck.len() == 0 {
                self.state = GameState::Final(self.hands.len() as u8);
            }
        }
    }

    pub fn card_cluded(&self, pos: u8, player: usize) -> bool {
        self.hands[(self.active_player + player) % self.hands.len()][pos as usize]
            .clues
            .len()
            > 0
    }

    fn cards_clued(&mut self, player: usize) -> PositionSet {
        let index = (self.active_player + player) % self.hands.len();
        let mut clued = PositionSet::new(self.hands[index].len() as u8);
        for pos in 0..self.hands[index].len() {
            if self.hands[index][pos].clues.len() > 0 {
                clued.add(pos as u8);
            }
        }
        clued
    }

    pub fn player_card(&self, pos: u8, player: usize) -> Card {
        assert!(player > 0, "Own cards are unknown");
        self.hands[(self.active_player + player) % self.hands.len()][pos as usize].card
    }

    pub fn min_played_rank(&self) -> u8 {
        *self.played.iter().min().unwrap()
    }

    fn update_max_score(&mut self) {
        self.max_score = 0;
        for suite in self.suites.iter() {
            self.max_score += self.max_rank_for_suite(*suite);
        }
    }

    pub fn card_must_rank(&self, player: usize, pos: u8, rank: u8) -> bool {
        self.hands[(self.active_player + player) % self.hands.len()][pos as usize].potential_ranks
            == 1 << (rank - 1)
    }

    pub fn max_rank_for_suite(&self, suite: Suite) -> u8 {
        let mut max = 0;
        while max < 5 {
            let card = Card {
                suite,
                rank: max + 1,
            };
            if *self.discarded.get(&card).unwrap_or(&0) == suite.card_count(max + 1) {
                return max;
            }
            max += 1;
        }
        max
    }

    fn play(&mut self, strategies: &mut Vec<&mut dyn PlayerStrategy>) {
        let action = strategies[self.active_player].act(&self);
        self.turn += 1;
        match action {
            Move::Discard(pos) => {
                let card = self.hands[self.active_player].remove(pos as usize);
                if card.is_none() {
                    println!(
                        "Invalid move: player {} tried to discard card {} (hand only has {} cards)",
                        self.active_player,
                        pos,
                        self.hands[self.active_player].len()
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                let card = card.unwrap();
                if self.debug {
                    println!(
                        "Player {} discarded {:?} from pos {}",
                        self.active_player, card, pos
                    );
                }
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
                let card = self.hands[self.active_player].remove(pos as usize);
                if card.is_none() {
                    println!(
                        "Invalid move: player {} tried to play card {} (hand only has {} cards)",
                        self.active_player,
                        pos,
                        self.hands[self.active_player].len()
                    );
                    self.state = GameState::Invalid();
                    return;
                }
                let card = card.unwrap();
                if self.played_rank(&card.card.suite) + 1 == card.card.rank {
                    if self.debug {
                        println!(
                            "Player {} played successfully {:?} from pos {}",
                            self.active_player, card, pos
                        );
                    }
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
                if player >= self.hands.len() as u8 || player == 0 {
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
                let player_index = (self.active_player + player as usize) % self.hands.len();
                let mut affected_cards = 0;
                let previously_clued = self.cards_clued(player as usize);
                let mut touched = PositionSet::new(self.hands[player_index].len() as u8);

                for (pos, card_state) in self.hands[player_index].iter_mut().enumerate() {
                    if card_state.clue(clue.clone()) {
                        affected_cards += 1;
                        touched.add(pos as u8);
                    }
                }
                if self.debug {
                    println!(
                        "Player {} clue played {} about {} {:?} cards",
                        self.active_player, player_index, affected_cards, clue
                    );
                }
                strategies[player_index].clued(clue, touched, previously_clued, &self);
                self.clues -= 1;
            }
        }
        self.active_player = (self.active_player + 1) % self.hands.len();
    }
}
