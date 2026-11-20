//! Chop moves (level 4): the trash chop move and the 5's chop move read from clues, the
//! order chop move read from plays of 1s. A chop-moved card is protected for the rest of
//! the game (`CardNote::chop_moved`); it is neither on chop nor in finesse position, and
//! it is never prompted.

use smallvec::SmallVec;

use super::profile::Rule;
use super::view::{Clued, PlayState, View};
use crate::domain::{CardId, CardSet, ClueId, IdentitySet, Seat};

/// A clue whose meaning is a chop move instead of a play or save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChopMoveClue {
    pub rule: &'static str,
    /// The cards that are chop moved.
    pub cards: CardSet,
}

impl View {
    /// Chop moves are active (level 4) and not switched off by the end-game (level 8).
    pub fn chop_moves_allowed(&self) -> bool {
        self.rule(Rule::ChopMove) && !self.in_end_game()
    }

    /// Unclued, unpromised, not chop-moved cards of `seat`, oldest (chop) first.
    pub fn unprotected(&self, seat: Seat) -> SmallVec<[CardId; 6]> {
        self.state
            .hand(seat)
            .iter()
            .copied()
            .filter(|&c| !self.note(c).touched())
            .collect()
    }

    /// Identities that count as already accounted for when `receiver` judges trash: played
    /// or dead, or clued in another hand (good touch).
    fn accounted_for(&self, receiver: Seat) -> IdentitySet {
        let mut set = self.trash_set();
        for identity in self.variant.all_identities().iter() {
            let info = self.info[identity.index()];
            if info.clued.is_some() && info.clued != Some(Clued::Holder(receiver)) {
                set.insert(identity);
            }
        }
        set
    }

    /// Classify a clue (before it is applied) as a trash chop move or a 5's chop move.
    /// Fixes and stalls are classified first by the caller.
    pub fn chop_move_clue(
        &self,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
    ) -> Option<ChopMoveClue> {
        if !self.chop_moves_allowed() {
            return None;
        }
        let unprotected = self.unprotected(receiver);
        let chop = *unprotected.first()?;
        let top = self.variant.num_ranks;
        // 5's chop move: the 5 touched is one away from chop (counting only unclued cards);
        // the card to its right (the chop) is chop moved. A 5 on chop is a 5 save.
        if self.rule(Rule::FiveChopMove)
            && self.is_rank_clue(clue, top)
            && !touched.contains(chop)
            && unprotected.get(1).is_some_and(|&c| touched.contains(c))
        {
            return Some(ChopMoveClue {
                rule: "L4.five-chop-move",
                cards: CardSet::single(chop),
            });
        }
        // Trash chop move: every newly touched card is known trash; every unclued card to
        // the right of the rightmost trash card is chop moved.
        if self.rule(Rule::TrashChopMove) {
            let touch = self.variant.touch(clue);
            let fresh: SmallVec<[CardId; 6]> = self
                .state
                .hand(receiver)
                .iter()
                .copied()
                .filter(|&c| touched.contains(c) && !self.note(c).touched())
                .collect();
            let accounted = self.accounted_for(receiver);
            let all_trash = !fresh.is_empty()
                && fresh.iter().all(|&c| {
                    let mut belief = self.note(c).belief;
                    belief.apply_clue(touch, true);
                    !belief.soft.is_empty() && belief.soft.is_subset(accounted)
                });
            if all_trash {
                // hands are stored oldest first: the first fresh card is the rightmost
                let rightmost = fresh[0];
                let cards: CardSet = unprotected
                    .iter()
                    .copied()
                    .take_while(|&c| c != rightmost)
                    .collect();
                if !cards.is_empty() {
                    return Some(ChopMoveClue {
                        rule: "L4.trash-chop-move",
                        cards,
                    });
                }
            }
        }
        None
    }

    /// Clued unknown 1s of `seat` that take part in the 1s order (same number of positive
    /// clues as `like`, identity unknown, not finessed, not known trash).
    fn order_ones(&self, seat: Seat, like: CardId) -> SmallVec<[CardId; 5]> {
        let ones = self.variant.rank_identities(1);
        let positives = self.public.touched_by(like).count_ones();
        self.state
            .hand(seat)
            .iter()
            .copied()
            .filter(|&c| {
                let note = self.note(c);
                note.clued
                    && !note.trash
                    && note.promised.is_none()
                    && !note.belief.soft.is_empty()
                    && note.belief.soft.is_subset(ones)
                    && note.belief.size() > 1
                    && self.public.touched_by(c).count_ones() == positives
            })
            .collect()
    }

    /// The seat whose chop an order chop move by `seat` playing `card` moves: playing an
    /// unknown 1 that skips `k` older 1s in the agreed order moves the chop of the seat `k`
    /// places after the player.
    pub fn order_chop_move_target(&self, seat: Seat, card: CardId) -> Option<Seat> {
        if !self.rule(Rule::OrderChopMove) || !self.chop_moves_allowed() {
            return None;
        }
        let ones = self.order_ones(seat, card);
        if !ones.contains(&card) {
            return None;
        }
        let order = self.one_order(seat, card);
        let skipped = ones
            .iter()
            .filter(|&&c| c != card && self.one_order(seat, c) < order)
            .count() as u8;
        if skipped == 0 || skipped >= self.num_seats() {
            return None;
        }
        Some(seat.after(skipped, self.num_seats()))
    }

    /// Whether a visible card is worth protecting with a chop move: not clued elsewhere and
    /// critical, or a 2 with no other visible copy (the cards a save clue could protect).
    pub fn worth_protecting(&self, card: CardId) -> bool {
        let Some(identity) = self.identity(card) else {
            return false;
        };
        if self.info[identity.index()].clued.is_some() {
            return false;
        }
        match self.play_state(identity) {
            PlayState::Critical | PlayState::CriticalPlayable => true,
            // A playable card deserves a play clue; a protected one-away card clogs the
            // hand for good.
            PlayState::Playable => false,
            PlayState::Trash | PlayState::Dead => false,
            PlayState::Normal => {
                self.variant.rank_of(identity) == 2
                    && !self.state.seats().any(|other| {
                        self.state
                            .hand(other)
                            .iter()
                            .any(|&c| c != card && self.identity(c) == Some(identity))
                    })
            }
        }
    }

    /// Whether this view sees a card on `seat`'s chop worth protecting with a chop move.
    pub fn chop_worth_moving(&self, seat: Seat) -> bool {
        self.chop(seat).is_some_and(|chop| {
            self.identity(chop).is_some_and(|i| {
                matches!(
                    self.play_state(i),
                    PlayState::Critical | PlayState::CriticalPlayable
                ) || self.variant.rank_of(i) == 2
            }) && self.worth_protecting(chop)
        })
    }

    /// One of our unknown 1s whose play is an order chop move saving a valuable chop card,
    /// skipping as few 1s as possible; `None` when the normal order is fine.
    pub fn order_chop_move_play(&self) -> Option<CardId> {
        let first = self.next_play()?;
        let ones = self.order_ones(self.me, first);
        if !ones.contains(&first) {
            return None;
        }
        let mut options: SmallVec<[(u8, CardId); 5]> = ones
            .iter()
            .filter(|&&c| c != first)
            .filter_map(|&c| {
                let target = self.order_chop_move_target(self.me, c)?;
                (target != self.me && self.chop_worth_moving(target)).then(|| {
                    let order = self.one_order(self.me, c);
                    let skipped = ones
                        .iter()
                        .filter(|&&o| self.one_order(self.me, o) < order)
                        .count() as u8;
                    (skipped, c)
                })
            })
            .collect();
        options.sort();
        options.first().map(|&(_, card)| card)
    }
}
