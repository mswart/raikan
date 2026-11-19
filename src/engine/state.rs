use crate::domain::{CardId, CardSet, ClueId, Identity, MAX_CARDS, MAX_HAND, MAX_SEATS, Seat};
use crate::variants::{IdentityCounts, Stacks, Variant};
use serde::{Deserialize, Serialize};

/// A command, chosen from the actor's information only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Play(CardId),
    Discard(CardId),
    Clue { target: Seat, clue: ClueId },
}

/// Why a game ended (hanab.live end-condition values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EndReason {
    /// Deck ran out and the final round was played, or every stack is complete.
    Normal,
    Strikeout,
    /// An illegal command was submitted (exported like a hanab.live timeout).
    Illegal,
}

/// Whether the game is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Running,
    Over(EndReason),
}

/// The complete public state of a game. Fixed size and `Copy`: no heap, no history.
///
/// Hands hold card ids oldest first; identities live in a [`World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameState {
    pub num_seats: u8,
    pub hand_size: u8,
    hands: [[CardId; MAX_HAND]; MAX_SEATS],
    hand_lens: [u8; MAX_SEATS],
    pub stacks: Stacks,
    pub discarded: IdentityCounts,
    /// Clue tokens in half units.
    pub tokens: u8,
    pub strikes: u8,
    /// Next card id to draw; equals the number of cards dealt so far.
    pub next_card: u8,
    pub deck_total: u8,
    /// Number of actions taken so far.
    pub turn: u16,
    pub current: Seat,
    /// Turns left once the deck is empty (`None` while cards remain).
    pub final_turns: Option<u8>,
    pub status: Status,
    /// Cards touched by at least one clue (public).
    pub clued: CardSet,
}

impl GameState {
    /// The state before the initial deal.
    pub fn new(variant: &Variant, num_seats: u8) -> Self {
        assert!((2..=MAX_SEATS as u8).contains(&num_seats));
        Self {
            num_seats,
            hand_size: hand_size(num_seats),
            hands: [[CardId(0); MAX_HAND]; MAX_SEATS],
            hand_lens: [0; MAX_SEATS],
            stacks: Stacks::default(),
            discarded: [0; crate::domain::MAX_IDENTITIES],
            tokens: variant.tokens.start,
            strikes: 0,
            next_card: 0,
            deck_total: variant.num_cards(),
            turn: 0,
            current: Seat(0),
            final_turns: None,
            status: Status::Running,
            clued: CardSet::EMPTY,
        }
    }

    pub fn seats(&self) -> impl Iterator<Item = Seat> + use<> {
        (0..self.num_seats).map(Seat)
    }

    /// Cards in a hand, oldest first.
    pub fn hand(&self, seat: Seat) -> &[CardId] {
        &self.hands[seat.index()][..self.hand_lens[seat.index()] as usize]
    }

    pub fn hand_set(&self, seat: Seat) -> CardSet {
        self.hand(seat).iter().copied().collect()
    }

    /// The seat holding a card, if it is in a hand.
    pub fn holder(&self, card: CardId) -> Option<Seat> {
        self.seats().find(|&s| self.hand(s).contains(&card))
    }

    pub fn cards_left(&self) -> u8 {
        self.deck_total - self.next_card
    }

    /// Whole clue tokens (rounded down).
    pub fn clue_tokens(&self) -> u8 {
        self.tokens / 2
    }

    pub fn score(&self, variant: &Variant) -> u8 {
        variant.stack_score(&self.stacks)
    }

    pub fn is_over(&self) -> bool {
        matches!(self.status, Status::Over(_))
    }

    /// Cards drawn so far (in hands, on the stacks or in the discard pile).
    pub fn dealt(&self) -> CardSet {
        CardSet::first(self.next_card as usize)
    }

    pub(super) fn push_card(&mut self, seat: Seat, card: CardId) {
        let len = self.hand_lens[seat.index()] as usize;
        self.hands[seat.index()][len] = card;
        self.hand_lens[seat.index()] += 1;
    }

    pub(super) fn remove_card(&mut self, seat: Seat, card: CardId) -> bool {
        let len = self.hand_lens[seat.index()] as usize;
        let hand = &mut self.hands[seat.index()];
        let Some(pos) = hand[..len].iter().position(|&c| c == card) else {
            return false;
        };
        hand.copy_within(pos + 1..len, pos);
        hand[len - 1] = CardId(0);
        self.hand_lens[seat.index()] -= 1;
        true
    }
}

/// Starting hand size by player count (hanab.live defaults).
pub fn hand_size(num_seats: u8) -> u8 {
    match num_seats {
        2 | 3 => 5,
        4 | 5 => 4,
        _ => 3,
    }
}

/// An identity for every card. `GameState` + `World` is a complete, simulatable game.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct World {
    identities: [Identity; MAX_CARDS],
    len: u8,
}

impl World {
    pub fn new(identities: &[Identity]) -> Self {
        assert!(identities.len() <= MAX_CARDS);
        let mut array = [Identity(0); MAX_CARDS];
        array[..identities.len()].copy_from_slice(identities);
        Self {
            identities: array,
            len: identities.len() as u8,
        }
    }

    pub fn identity(&self, card: CardId) -> Identity {
        debug_assert!(card.0 < self.len);
        self.identities[card.index()]
    }

    pub fn set(&mut self, card: CardId, identity: Identity) {
        self.identities[card.index()] = identity;
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[Identity] {
        &self.identities[..self.len as usize]
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.as_slice().iter().map(|i| i.0))
            .finish()
    }
}
