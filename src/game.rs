use std::collections::BTreeMap;
use std::collections::VecDeque;

// mod position_set;

// use crate::game::{self, CardPlayState};

// use crate::position_set;
pub use crate::position_set::PositionSet;

use colored::*;
use rand::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Suite {
    Red(),
    Green(),
    Yellow(),
    Blue(),
    Purple(),
}

impl Suite {
    pub fn color(&self) -> Color {
        match self {
            Self::Red() => Color::Red,
            Self::Green() => Color::Green,
            Self::Yellow() => Color::Yellow,
            Self::Blue() => Color::Cyan,
            Self::Purple() => Color::Magenta,
        }
    }

    pub fn char(&self) -> char {
        match self {
            Self::Red() => 'r',
            Self::Green() => 'g',
            Self::Yellow() => 'y',
            Self::Blue() => 'b',
            Self::Purple() => 'p',
        }
    }

    pub fn card_count(&self, rank: u8) -> u8 {
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
    clued: bool,
}

impl CardState {
    fn from_card(card: Card) -> Self {
        Self { card, clued: false }
    }

    fn clue(&mut self, clue: Clue) -> bool {
        let clued = self.card.affected(clue);
        self.clued |= clued;
        clued
    }
}

impl std::fmt::Debug for CardState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {}", self.card, if self.clued { '*' } else { ' ' })
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
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
    pub score_integral: u16,
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

impl ClueColor {
    pub fn suite(&self) -> Suite {
        match self {
            ClueColor::Red() => Suite::Red(),
            ClueColor::Yellow() => Suite::Yellow(),
            ClueColor::Blue() => Suite::Blue(),
            ClueColor::Green() => Suite::Green(),
            ClueColor::Purple() => Suite::Purple(),
        }
    }
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

pub trait PlayerStrategy: std::fmt::Debug {
    fn init(&mut self, game: &Game);
    fn act(&mut self, game: &Game) -> Move;

    fn drawn(&mut self, player: usize, card: Card);
    fn own_drawn(&mut self);

    fn played(&mut self, player: usize, pos: usize, card: Card, successful: bool, blind: bool);

    fn discarded(&mut self, player: usize, pos: usize, card: Card);
    fn clued(
        &mut self,
        who: usize,
        whom: usize,
        clue: Clue,
        touched: PositionSet,
        previously_clued: PositionSet,
        game: &Game,
    );
}

impl Game {
    pub fn new(players: &mut Vec<&mut dyn PlayerStrategy>, debug: bool, seed: u64) -> Self {
        let suites = vec![
            Suite::Red(),
            Suite::Green(),
            Suite::Yellow(),
            Suite::Blue(),
            Suite::Purple(),
        ];

        let mut rng = rand_pcg::Pcg64::seed_from_u64(seed);
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
            hands.push(Hand::with_capacity(num_cards));
        }

        let mut game = Self {
            score: 0,
            score_integral: 0,
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

        for player in 0..players.len() {
            for _ in 0..num_cards {
                game.draw_card(player, players);
            }
        }

        game
    }

    pub fn num_players(&self) -> u8 {
        self.hands.len() as u8
    }

    pub fn num_hand_cards(&self, player: usize) -> u8 {
        self.hands[(self.active_player + player) % self.hands.len()].len() as u8
    }

    pub fn dump(&self, strategies: &mut Vec<&mut dyn PlayerStrategy>) {
        println!("Game:");
        println!(
            "  suites={:?} turn={} score={}/{} (sum: {}) strikes={} clues={} state={:?}",
            self.suites,
            self.turn,
            self.score,
            self.max_score,
            self.score_integral,
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

        for (pos, hand) in self.hands.iter().enumerate() {
            println!("  hand {:?} {:?}", hand, strategies[pos]);
        }

        println!("  deck: {:?}", self.deck);
    }

    pub fn run(&mut self, strategies: &mut Vec<&mut dyn PlayerStrategy>) -> u8 {
        if self.debug {
            self.dump(strategies);
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
                self.dump(strategies);
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

    fn relative_player_index(&self, player: usize, receiver: usize) -> usize {
        (self.hands.len() + player - receiver) % self.hands.len()
    }

    fn draw_card(&mut self, player: usize, strategies: &mut Vec<&mut dyn PlayerStrategy>) {
        if let Some(card) = self.deck.pop_front() {
            self.hands[player].push_front(CardState::from_card(card));
            if self.deck.len() == 0 {
                self.state = GameState::Final(self.hands.len() as u8);
            }
            for (notify_player, strategy) in strategies.iter_mut().enumerate() {
                if notify_player == player {
                    strategy.own_drawn();
                } else {
                    strategy.drawn(self.relative_player_index(player, notify_player), card);
                }
            }
        }
    }

    fn update_max_score(&mut self) {
        self.max_score = 0;
        for suite in self.suites.iter() {
            self.max_score += self.max_rank_for_suite(*suite);
        }
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
                for (notify_player, strategy) in strategies.iter_mut().enumerate() {
                    strategy.discarded(
                        self.relative_player_index(self.active_player, notify_player),
                        pos as usize,
                        card.card,
                    );
                }
                self.draw_card(self.active_player, strategies);
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
                let success = if self.played_rank(&card.card.suite) + 1 == card.card.rank {
                    if self.debug {
                        println!(
                            "Player {} played successfully {:?} from pos {}",
                            self.active_player, card, pos
                        );
                    }
                    for (suite_pos, current_suite) in self.suites.iter().enumerate() {
                        if *current_suite == card.card.suite {
                            self.played[suite_pos] += 1;
                        }
                    }
                    self.score += 1;
                    if card.card.rank == 5 && self.clues < 8 {
                        self.clues += 1;
                    }
                    if self.score as usize == self.suites.len() * 5 {
                        self.state = GameState::Won();
                    }
                    true
                } else {
                    if self.debug {
                        println!(
                            "Player {} failed to played {:?} from pos {}",
                            self.active_player, card, pos
                        );
                    }
                    self.discard(card.card);
                    self.num_strikes += 1;
                    if self.num_strikes == 3 {
                        self.state = GameState::Lost();
                        if self.debug {
                            println!("Game lost due to three strikes");
                        }
                    }
                    false
                };
                for (notify_player, strategy) in strategies.iter_mut().enumerate() {
                    strategy.played(
                        self.relative_player_index(self.active_player, notify_player),
                        pos as usize,
                        card.card,
                        success,
                        !card.clued,
                    );
                }
                self.draw_card(self.active_player, strategies);
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
                let mut previously_clued = PositionSet::new(self.hands[player_index].len() as u8);
                let mut touched = PositionSet::new(self.hands[player_index].len() as u8);

                for (pos, card_state) in self.hands[player_index].iter_mut().enumerate() {
                    if card_state.clued {
                        previously_clued.add(pos as u8);
                    }
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
                for (notify_player, strategy) in strategies.iter_mut().enumerate() {
                    strategy.clued(
                        self.relative_player_index(self.active_player, notify_player),
                        self.relative_player_index(player_index, notify_player),
                        clue,
                        touched,
                        previously_clued,
                        &self,
                    );
                }
                self.clues -= 1;
            }
        }
        self.active_player = (self.active_player + 1) % self.hands.len();
        self.score_integral += self.score as u16;
    }
}
