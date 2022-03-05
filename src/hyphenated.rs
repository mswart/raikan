use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::game::{self, CardPlayState};
use crate::{
    card_quantum::{CardQuantum, Variant},
    PositionSet,
};

impl std::fmt::Debug for HyphenatedPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        for (pos, slot) in self.line.hands[0].iter().enumerate() {
            if pos > 0 {
                f.write_str(", ")?;
            }
            std::fmt::Display::fmt(&slot.quantum, f)?;
            if slot.clued {
                f.write_str("'")?;
            }
            if slot.trash {
                f.write_str("kt")?;
            }
            if slot.play {
                f.write_str("!")?;
            }
        }
        f.write_str("]")?;
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone)]
struct Slot {
    card: game::Card,
    clued: bool,
    play: bool,
    trash: bool,
    quantum: CardQuantum,
}

impl Slot {
    fn update_slot_attributes(&mut self, game: &game::Game) {
        let mut all_trash = true;
        let mut all_playable = true;
        for card in self.quantum.iter() {
            match card.play_state(game) {
                game::CardPlayState::Playable() => all_trash = false,
                game::CardPlayState::Critical() => {
                    all_trash = false;
                    all_playable = false;
                    break;
                }
                game::CardPlayState::Normal() => {
                    all_trash = false;
                    all_playable = false;
                    break;
                }
                game::CardPlayState::Trash() => all_playable = false,
                game::CardPlayState::Dead() => all_playable = false,
            }
        }
        if all_playable {
            self.play = true;
        }
        if all_trash {
            self.trash = true;
            self.play = false;
        }
    }
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.card, f)?;
        if self.clued {
            f.write_str("'")?;
        }
        if self.trash {
            f.write_str("kt")?;
        }
        if self.play {
            f.write_str("!")?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Ord, Eq, Clone, Debug)]
pub struct LineScore {
    discard_risks: i8,
    score: u8,
    max_score: u8,
    clued: u8,
    play: u8,
    errors: u8,
    bonus: u8,
}

impl PartialOrd for LineScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.score.cmp(&other.score) {
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Equal => {}
        }
        match self.max_score.cmp(&other.max_score) {
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Equal => {}
        }
        match (self.discard_risks as i32
            + self.play as i32
            + self.clued as i32 * 2
            + self.bonus as i32
            - self.errors as i32 * 10)
            .cmp(
                &(other.discard_risks as i32
                    + other.play as i32
                    + other.clued as i32 * 2
                    + other.bonus as i32
                    - other.errors as i32 * 10),
            ) {
            std::cmp::Ordering::Greater => return Some(std::cmp::Ordering::Greater),
            std::cmp::Ordering::Less => return Some(std::cmp::Ordering::Less),
            std::cmp::Ordering::Equal => {}
        }
        Some(self.play.cmp(&other.play))
    }
}

impl LineScore {
    pub fn zero() -> Self {
        Self {
            score: 0,
            max_score: 0,
            clued: 0,
            play: 0,
            discard_risks: 0,
            errors: 0,
            bonus: 0,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Line {
    hands: Vec<VecDeque<Slot>>,
    clued_cards: BTreeSet<game::Card>,
    tracked_cards: BTreeMap<game::Card, u8>,
    turn: u8,
    variant: Variant,
}

impl Line {
    pub fn new(num_players: u8) -> Self {
        let mut hands = Vec::new();
        for _ in 0..num_players {
            hands.push(VecDeque::new());
        }
        Self {
            clued_cards: BTreeSet::new(),
            hands,
            tracked_cards: BTreeMap::new(),
            turn: 0,
            variant: Variant {},
        }
    }

    fn score(&self, extra_error: u8, game: &game::Game) -> LineScore {
        let mut discard_risks = 0;
        let mut clued = 0;
        let mut play = 0;
        let mut errors = extra_error;
        let mut bonus = 0;
        for hand in self.hands.iter().skip(1) {
            let mut queued_actions = 0;
            let mut chop = true;
            let mut discard_risk = 0;
            for slot in hand.iter().rev() {
                if slot.clued {
                    clued += 1;
                    if slot.play {
                        play += 1;
                        queued_actions += 1;
                    }
                    if slot.trash {
                        match slot.card.play_state(&game) {
                            CardPlayState::Trash() => queued_actions += 1,
                            CardPlayState::Dead() => queued_actions += 1,
                            CardPlayState::Critical() => errors += 3,
                            CardPlayState::Playable() => errors += 2,
                            CardPlayState::Normal() => errors += 1,
                        };
                    } else {
                        match slot.card.play_state(&game) {
                            CardPlayState::Trash() => errors += 2,
                            CardPlayState::Dead() => errors += 2,
                            _ => {}
                        };
                    }
                } else if chop {
                    chop = false;
                    if !self.clued_cards.contains(&slot.card) {
                        match slot.card.play_state(&game) {
                            CardPlayState::Critical() => discard_risk -= 3,
                            CardPlayState::Playable() => discard_risk -= 2,
                            _ => {}
                        }
                    }
                }
                if slot.play {
                    match slot.card.play_state(&game) {
                        CardPlayState::Playable() => {}
                        CardPlayState::Critical() => errors += 3,
                        CardPlayState::Normal() => errors += 2,
                        CardPlayState::Dead() => errors += 1,
                        CardPlayState::Trash() => errors += 1,
                    }
                }
                if !slot.trash && !slot.quantum.contains(&slot.card) {
                    match slot.card.play_state(&game) {
                        CardPlayState::Playable() => errors += 2,
                        CardPlayState::Critical() => errors += 3,
                        CardPlayState::Normal() => errors += 2,
                        CardPlayState::Dead() => errors += 1,
                        CardPlayState::Trash() => errors += 1,
                    }
                }
                if slot.quantum.size() == 1 {
                    bonus += 1;
                }
            }
            if discard_risk != 0 && queued_actions < 1 {
                discard_risks += discard_risk;
            }
        }
        LineScore {
            score: game.score,
            max_score: game.max_score,
            clued,
            play,
            discard_risks,
            errors,
            bonus,
        }
    }

    pub fn drawn(&mut self, player: usize, card: game::Card) {
        self.hands[player].push_front(Slot {
            quantum: CardQuantum::new(self.variant),
            card: card,
            clued: false,
            play: false,
            trash: false,
        });
        self.track_card(card);
    }

    pub fn own_drawn(&mut self) {
        let mut hand = Slot {
            quantum: CardQuantum::new(self.variant),
            play: false,
            trash: false,
            clued: false,
            card: game::Card {
                suit: self.variant.suits()[0],
                rank: 0,
            },
        };
        for (card, count) in self.tracked_cards.iter() {
            if *count == card.suit.card_count(card.rank) {
                // a card is lost -> updated maximal possible score based on remaining cards
                hand.quantum.remove_card(card);
            }
        }
        self.hands[0].push_front(hand);
    }

    fn played(
        &mut self,
        player: usize,
        pos: usize,
        card: game::Card,
        successful: bool,
        _blind: bool,
    ) {
        self.turn += 1;
        let removed = self.hands[player].remove(pos).expect("Game ensures this");
        if player == 0 {
            self.track_card(card);
            if removed.clued && successful {
                self.clued_cards.insert(card);
                for slot in self.hands[0].iter_mut() {
                    slot.quantum.remove_card(&card);
                }
            }
        }
        if !successful {
            self.clued_cards.remove(&card);
        }
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.turn += 1;
        self.hands[player].remove(pos);
        if player == 0 {
            self.track_card(card);
        }
    }

    fn foreign_chop(&self, player: usize) -> i8 {
        for (pos, slot) in self.hands[player].iter().enumerate().rev() {
            if !slot.clued {
                return pos as i8;
            }
        }
        return -1;
    }

    fn track_card(&mut self, card: game::Card) {
        let count = self
            .tracked_cards
            .entry(card)
            .and_modify(|e| *e += 1)
            .or_insert(1);
        if *count == card.suit.card_count(card.rank) {
            // all instances of card are tracked (elsewhere!), card cannot be in our hand
            for slot in self.hands[0].iter_mut() {
                slot.quantum.remove_card(&card);
            }
        }
    }

    pub fn clued(
        &mut self,
        who: usize,
        whom: usize,
        clue: game::Clue,
        touched: game::PositionSet,
        previously_clued: game::PositionSet,
        game: &game::Game,
    ) -> u8 {
        self.turn += 1;
        let mut error = 0;
        let newly_clued = touched - previously_clued;
        for pos in 0..self.hands[whom].len() {
            match clue {
                game::Clue::Rank(rank) => self.hands[whom][pos as usize]
                    .quantum
                    .limit_by_rank(rank as usize, touched.contains(pos as u8)),
                game::Clue::Color(color) => self.hands[whom][pos as usize]
                    .quantum
                    .limit_by_suit(&color.suit(), touched.contains(pos as u8)),
            }
        }
        if newly_clued.is_empty() {
            let focus = touched.first().expect("empty clues are not implemented");
            self.hands[whom][focus as usize].play = true;
            return 0;
        }

        let old_chop = self.foreign_chop(whom);

        let mut potential_safe = false;
        let focus = if old_chop >= 0 && touched.contains(old_chop as u8) {
            let chop_slot = &mut self.hands[whom][old_chop as usize];
            // check whether it can be a safe clue.
            for potential_card in chop_slot.quantum.clone().iter() {
                if potential_card.rank == 5 && clue != game::Clue::Rank(5) {
                    // 5 will only be safed via rank
                    continue;
                }
                match potential_card.play_state(game) {
                    game::CardPlayState::Critical() => potential_safe = true,
                    game::CardPlayState::Dead() => {
                        chop_slot.quantum.remove_card(&potential_card);
                    }
                    game::CardPlayState::Trash() => {
                        chop_slot.quantum.remove_card(&potential_card);
                    }
                    game::CardPlayState::Normal() => {
                        chop_slot.quantum.remove_card(&potential_card);
                    }
                    _ => {}
                }
            }
            old_chop as u8
        } else {
            touched
                .first()
                .expect("We have check previously that touched must contain something")
        };

        // somebody else was clued -> remember which cards are clued
        for pos in (touched - previously_clued).iter_first(focus) {
            let slot = self.hands[whom]
                .get_mut(pos as usize)
                .expect("own and game state out of sync");
            slot.clued = true;
            for card in self.clued_cards.iter() {
                slot.quantum.remove_card(card);
            }
            if pos == focus && !potential_safe {
                slot.play = true;
                for potential_card in slot.quantum.clone().iter() {
                    match potential_card.play_state(game) {
                        game::CardPlayState::Playable() => {}
                        _ => slot.quantum.remove_card(&potential_card),
                    }
                }
                if slot.quantum.size() == 1 {
                    // for self mode
                    self.clued_cards.insert(
                        slot.quantum
                            .clone()
                            .iter()
                            .nth(0)
                            .expect("We checked the size"),
                    );
                }
            }
            slot.update_slot_attributes(&game);
            if pos == focus && slot.trash {
                error += 5;
            }
            if who > 0 && whom != 0 {
                let card = slot.card.clone();
                for own_hand in self.hands[0].iter_mut() {
                    if own_hand.clued {
                        own_hand.quantum.remove_card(&card);
                    }
                }
            }
            if whom != 0 {
                self.clued_cards.insert(self.hands[whom][pos as usize].card);
            }
        }
        error
    }

    pub fn clue(&mut self, whom: usize, clue: game::Clue, game: &game::Game) -> Option<LineScore> {
        let mut touched = PositionSet::new(self.hands[whom].len() as u8);
        let mut previously_clued = PositionSet::new(self.hands[whom].len() as u8);
        for (pos, slot) in self.hands[whom].iter().enumerate() {
            if slot.card.affected(clue) {
                touched.add(pos as u8);
            }
            if slot.clued {
                previously_clued.add(pos as u8);
            }
        }
        if touched.is_empty() {
            return None;
        }
        let error = self.clued(0, whom, clue, touched, previously_clued, game);
        Some(self.score(error, game))
    }

    fn discard(&mut self) -> game::Move {
        // look for trash
        let mut chop = -1;
        for (pos, slot) in self.hands[0].iter().enumerate() {
            if slot.trash {
                return game::Move::Discard(pos as u8);
            }
            if !slot.clued {
                chop = pos as i8;
            }
        }

        if chop >= 0 {
            return game::Move::Discard(chop as u8);
        }
        // all positions occupied, search for the best worst scenario to drop:
        // lock for highest possible card (least damage):
        for rank in [5, 4, 3, 2, 1].iter() {
            for (pos, slot) in self.hands[0].iter().enumerate() {
                if slot.quantum.is_rank(*rank) {
                    return game::Move::Discard(pos as u8);
                }
            }
        }
        // nothing clear found; drop newest card
        game::Move::Discard(0)
    }

    fn play(&mut self, game: &game::Game) -> Option<game::Move> {
        for (pos, slot) in self.hands[0].iter_mut().enumerate() {
            if slot.trash {
                continue;
            }
            if slot.clued {
                slot.update_slot_attributes(&game);
            }
            if slot.trash {
                slot.play = false;
            }
            if slot.play {
                return Some(game::Move::Play(pos as u8));
            }
        }
        None
    }
}

pub struct HyphenatedPlayer {
    debug: bool,
    variant: Variant,
    turn: u8,
    line: Line,
}

impl HyphenatedPlayer {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            variant: Variant {},
            turn: 0,
            line: Line::new(0),
        }
    }

    pub fn line(&self) -> Line {
        self.line.clone()
    }
}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn init(&mut self, game: &game::Game) {
        self.variant = Variant {};
        self.turn = 0;
        self.line = Line::new(game.num_players());
    }

    fn drawn(&mut self, player: usize, card: game::Card) {
        self.line.drawn(player, card);
    }

    fn own_drawn(&mut self) {
        self.line.own_drawn();
    }

    fn played(
        &mut self,
        player: usize,
        pos: usize,
        card: game::Card,
        successful: bool,
        blind: bool,
    ) {
        self.line.played(player, pos, card, successful, blind);
        self.turn += 1;
    }

    fn discarded(&mut self, player: usize, pos: usize, card: game::Card) {
        self.line.discarded(player, pos, card);
        self.turn += 1;
    }

    fn clued(
        &mut self,
        who: usize,
        whom: usize,
        clue: game::Clue,
        touched: game::PositionSet,
        previously_clued: game::PositionSet,
        game: &game::Game,
    ) {
        self.line
            .clued(who, whom, clue, touched, previously_clued, game);
        self.turn += 1;
    }

    fn act(&mut self, game: &game::Game) -> game::Move {
        if let Some(play_move) = self.line.play(game) {
            return play_move;
        }
        if game.clues == 0 {
            return self.line.discard();
        }
        // compare clues:
        let mut best_score = self.line.score(0, game); // LineScore::zero();
        let mut best_move = self.line.discard();
        if self.debug {
            println!("discarding score: {:?}", best_score);
        }
        for player in 1..game.num_players() {
            for suit in self.variant.suits().iter() {
                let clue = game::Clue::Color(suit.clue_color());
                if let Some(score) = self.line.clone().clue(player as usize, clue, game) {
                    if self.debug {
                        println!("considered cluing {:?} to {player} with {:?}", clue, score);
                    }
                    if score > best_score {
                        best_move = game::Move::Clue(player, clue);
                        best_score = score;
                    }
                }
            }
            for rank in 1..=5 {
                let clue = game::Clue::Rank(rank);
                if let Some(score) = self.line.clone().clue(player as usize, clue, game) {
                    if self.debug {
                        println!("considered cluingg {:?} to {player} with {:?}", clue, score);
                    }
                    if score > best_score {
                        best_move = game::Move::Clue(player, clue);
                        best_score = score;
                    }
                }
            }
        }
        return best_move;
    }
}
