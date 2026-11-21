//! Situation classifiers and the level-1/3 rule helpers: chop, focus, saves, 1s order,
//! fixes, stop clues, duplicated plays and sarcastic discards.

use smallvec::SmallVec;

use super::belief::Belief;
use super::profile::Rule;
use super::view::{PlayState, SarcasticTransfer, View};
use crate::domain::{CardId, CardSet, ClueId, Identity, IdentitySet, Seat};
use crate::variants::ClueValue;

/// A fix clue, classified with the receiver's knowledge before the clue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// These cards' inferences contradict the literal clue.
    Literal(CardSet),
    /// Stop the receiver's next queued play.
    Stop(CardId),
}

impl View {
    pub fn is_rank_clue(&self, clue: ClueId, rank: u8) -> bool {
        self.variant.clue_value(clue) == ClueValue::Rank(rank)
    }

    /// Chop: the oldest card that is neither clued, promised nor chop moved. Finessed and
    /// chop-moved cards carry an implicit clue and are not on chop.
    pub fn chop(&self, seat: Seat) -> Option<CardId> {
        self.state
            .hand(seat)
            .iter()
            .copied()
            .find(|&c| !self.note(c).touched())
    }

    pub fn newly_touched(&self, receiver: Seat, touched: CardSet) -> CardSet {
        let _ = receiver;
        touched
            .iter()
            .filter(|&c| !self.note(c).clued && self.note(c).promised.is_none())
            .collect()
    }

    /// Focus: chop if newly touched, otherwise the newest new card; a clue without new
    /// cards focuses the newest re-touched card. Chop-moved cards count as already touched,
    /// unless the clue touches no other new card: then the newest chop-moved card is the
    /// focus (level 4, chop moves and new clues).
    pub fn clue_focus(&self, receiver: Seat, touched: CardSet) -> Option<CardId> {
        let new = self.newly_touched(receiver, touched);
        let fresh: CardSet = new.iter().filter(|&c| !self.note(c).chop_moved).collect();
        if !fresh.is_empty() {
            self.chop(receiver)
                .filter(|&c| fresh.contains(c))
                .or_else(|| fresh.max())
        } else if !new.is_empty() {
            new.max()
        } else if self.rule(Rule::TempoClue) {
            // Focus shifting (level 6): skip touched cards already known to be playable.
            self.newest_first(receiver)
                .map(|(_, c)| c)
                .filter(|&c| touched.contains(c))
                .find(|&c| {
                    let note = self.note(c);
                    !(note.play && note.delayed == 0)
                })
                .or_else(|| touched.max())
        } else {
            touched.max()
        }
    }

    /// Whether a chop card of `identity` may be saved with `clue`, which touches `touched`
    /// in `receiver`'s hand (judged before the clue is applied). Only copies visible to
    /// giver and receiver can rule out a 2 save; a duplicate on another chop is the
    /// double-chop exception.
    pub fn can_save(
        &self,
        giver: Seat,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
        identity: Identity,
    ) -> bool {
        let rank = self.variant.rank_of(identity);
        if !self.save_clue_allowed(receiver, clue, touched, identity) {
            return false;
        }
        match self.play_state(identity) {
            PlayState::Critical if rank == self.variant.num_ranks => self.rule(Rule::Save5),
            PlayState::Critical => self.rule(Rule::SaveCritical),
            PlayState::Normal
                if rank == 2 && self.is_rank_clue(clue, 2) && self.rule(Rule::Save2) =>
            {
                self.state
                    .seats()
                    .filter(|&seat| seat != giver && seat != receiver)
                    .all(|seat| {
                        let chop = self.chop(seat);
                        self.state
                            .hand(seat)
                            .iter()
                            .all(|&c| self.identity(c) != Some(identity) || chop == Some(c))
                    })
            }
            _ => false,
        }
    }

    /// Whether `clue`, touching `touched` in `receiver`'s hand, is the kind of clue that
    /// saves `identity`, whatever its play state (judged before the clue is applied):
    ///
    /// * 5s are saved with 5.
    /// * Dark (one of each) 2s and 5s are saved with their rank; a black clue saves them
    ///   only under the black page's exceptions ([`View::dark_color_save_exception`]),
    ///   other colours (on a dark rainbow card) never.
    /// * Dark 3s and 4s are saved with a colour; a rank clue saves them only as a Florrat
    ///   Save (it newly touches another critical card).
    /// * A dark multi-colour 3 or 4 takes the colour of a plain dark suit if there is one,
    ///   otherwise its rank, or a colour under the dark rainbow page's exceptions
    ///   ([`View::dark_rainbow_color_exception`]).
    /// * Colourless dark cards (gray) can only be saved with their rank anyway.
    pub fn save_clue_allowed(
        &self,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
        identity: Identity,
    ) -> bool {
        let variant = &self.variant;
        let rank = variant.rank_of(identity);
        let top = variant.num_ranks;
        let suit = variant.suit_of(identity);
        if !self.rule(Rule::DarkSave) || !variant.suits[suit].one_of_each {
            return rank != top || self.is_rank_clue(clue, top);
        }
        let color_clue = matches!(variant.clue_value(clue), ClueValue::Color(_));
        if rank == 2 || rank == top {
            return self.is_rank_clue(clue, rank)
                || (self.is_dark_color(clue)
                    && self.dark_color_save_exception(receiver, clue, touched, identity));
        }
        if !(3..top).contains(&rank) {
            return true;
        }
        let colors: SmallVec<[ClueId; 6]> = variant
            .clue_ids()
            .filter(|&c| matches!(variant.clue_value(c), ClueValue::Color(_)))
            .filter(|&c| variant.touch(c).contains(identity))
            .collect();
        match colors.len() {
            0 => true,
            1 => color_clue || self.touches_two_critical(receiver, clue, touched),
            _ if !self.rule(Rule::DarkRainbowSave) => true,
            _ => {
                // Colours of the plain dark suits (black) that touch this card.
                let dark_colors: SmallVec<[ClueId; 6]> = colors
                    .iter()
                    .copied()
                    .filter(|&c| self.is_dark_color(c))
                    .collect();
                if !dark_colors.is_empty() {
                    dark_colors.contains(&clue)
                } else {
                    !color_clue || self.dark_rainbow_color_exception(receiver, clue, touched)
                }
            }
        }
    }

    /// A colour clue of a plain dark suit (black): it touches a one-of-each suit that no
    /// other colour touches.
    fn is_dark_color(&self, clue: ClueId) -> bool {
        let variant = &self.variant;
        matches!(variant.clue_value(clue), ClueValue::Color(_))
            && (0..variant.num_suits()).any(|suit| {
                variant.suits[suit].one_of_each
                    && !variant.is_multi_color_suit(suit)
                    && variant
                        .touch(clue)
                        .intersects(variant.suit_identities(suit))
            })
    }

    /// Whether a touched card of `receiver` may be one of `set`: its identity where this
    /// view sees it, otherwise the receiver's belief once the clue is applied.
    fn touched_may_be(&self, card: CardId, touch: IdentitySet, set: IdentitySet) -> bool {
        match self.identity(card) {
            Some(identity) => set.contains(identity),
            None => {
                let mut belief = self.note(card).belief;
                belief.apply_clue(touch, true);
                belief.intersects(set)
            }
        }
    }

    fn critical_set(&self) -> IdentitySet {
        self.variant
            .all_identities()
            .iter()
            .filter(|&i| self.is_critical(i))
            .collect()
    }

    /// The clue newly touches two or more critical cards (the dark card itself and another
    /// one). Seen cards count by identity; the receiver's own cards count when they may be
    /// critical. This is the first black 2/5 exception ("two or more previously unclued
    /// black cards or critical rainbow cards") and the Florrat Save for black 3s and 4s.
    pub fn touches_two_critical(&self, receiver: Seat, clue: ClueId, touched: CardSet) -> bool {
        let touch = self.variant.touch(clue);
        let critical = self.critical_set();
        self.newly_touched(receiver, touched)
            .iter()
            .filter(|&c| self.touched_may_be(c, touch, critical))
            .count()
            >= 2
    }

    /// The identity of a card as far as this view knows it: seen, or known to its holder.
    fn known_identity(&self, card: CardId) -> Option<Identity> {
        self.identity(card)
            .or_else(|| self.note(card).belief.identity())
    }

    /// Touching this card would break good touch: known trash, or an identity that is
    /// already clued elsewhere.
    fn known_bad_touch(&self, card: CardId) -> bool {
        self.note(card).trash
            || self.known_identity(card).is_some_and(|i| {
                self.trash_set().contains(i) || self.info[i.index()].clued.is_some()
            })
    }

    /// The black page's exceptions that let a colour clue save a dark 2 or 5 (the page
    /// defines none of them further; these are the readings implemented):
    ///
    /// 1. the clue newly touches two or more critical cards;
    /// 2. it is needed for good touch: the rank clue would newly touch a card known to be
    ///    trash or a duplicate, while the colour clue newly touches none;
    /// 3. it fills in an ancillary card: it re-touches an already clued card and narrows
    ///    what that card can be;
    /// 4. it gives important negative information: an untouched clued card's identity,
    ///    playability or trash status becomes known.
    ///
    /// Exception 2 depends on the rank clue's touches, which the receiver cannot see; it
    /// only counts cards whose identity the evaluating view knows.
    pub fn dark_color_save_exception(
        &self,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
        identity: Identity,
    ) -> bool {
        if self.touches_two_critical(receiver, clue, touched) {
            return true;
        }
        let touch = self.variant.touch(clue);
        let hand = self.state.hand(receiver);
        let newly = self.newly_touched(receiver, touched);
        // 2. good touch
        let rank = self.variant.rank_of(identity);
        if let Some(rank_clue) = self.variant.clue_id(ClueValue::Rank(rank)) {
            let rank_touch = self.variant.touch(rank_clue);
            let rank_bad = hand.iter().any(|&c| {
                !self.note(c).touched()
                    && self
                        .known_identity(c)
                        .is_some_and(|i| rank_touch.contains(i))
                    && self.known_bad_touch(c)
            });
            if rank_bad && !newly.iter().any(|c| self.known_bad_touch(c)) {
                return true;
            }
        }
        // 3. fill-in of an already clued card
        let fills_in = hand.iter().any(|&c| {
            let note = self.note(c);
            note.clued && touched.contains(c) && !note.belief.soft.is_subset(touch)
        });
        if fills_in {
            return true;
        }
        // 4. important negative information
        let playable = self.playable_set();
        let trash = self.trash_set();
        hand.iter().any(|&c| {
            let note = self.note(c);
            if !note.clued || touched.contains(c) {
                return false;
            }
            let before = note.belief;
            let mut after = before;
            after.apply_clue(touch, false);
            let settled = |b: &Belief| {
                b.size() > 0
                    && (b.identity().is_some() || b.is_subset_of(playable) || b.is_subset_of(trash))
            };
            after != before && !settled(&before) && settled(&after)
        })
    }

    /// The dark rainbow page's exceptions that let a colour clue save a dark rainbow 3 or
    /// 4 when no black clue exists:
    ///
    /// * the clue also newly touches another critical card that is critical because of
    ///   discards: not a top-rank card and not of a dark suit;
    /// * the colour's own suit is completely played or accounted for: each of its cards is
    ///   played, can no longer be played, or is clued somewhere.
    pub fn dark_rainbow_color_exception(
        &self,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
    ) -> bool {
        let variant = &self.variant;
        let touch = variant.touch(clue);
        let discarded_critical: IdentitySet = self
            .critical_set()
            .iter()
            .filter(|&i| {
                variant.rank_of(i) != variant.num_ranks
                    && !variant.suits[variant.suit_of(i)].one_of_each
            })
            .collect();
        if self
            .newly_touched(receiver, touched)
            .iter()
            .any(|c| self.touched_may_be(c, touch, discarded_critical))
        {
            return true;
        }
        let ClueValue::Color(color) = variant.clue_value(clue) else {
            return false;
        };
        let name = &variant.clue_colors[color as usize];
        (0..variant.num_suits())
            .filter(|&suit| {
                let info = &variant.suits[suit];
                info.clue_colors.len() == 1
                    && &info.clue_colors[0] == name
                    && !info.all_clue_colors
                    && !variant.is_multi_color_suit(suit)
            })
            .any(|suit| {
                variant.suit_identities(suit).iter().all(|i| {
                    matches!(self.play_state(i), PlayState::Trash | PlayState::Dead)
                        || self.info[i.index()].clued.is_some()
                })
            })
    }

    /// The cards of `receiver` this view knows `clue` would touch.
    pub fn known_touches(&self, receiver: Seat, clue: ClueId) -> CardSet {
        let touch = self.variant.touch(clue);
        self.state
            .hand(receiver)
            .iter()
            .copied()
            .filter(|&c| self.identity(c).is_some_and(|i| touch.contains(i)))
            .collect()
    }

    /// Loaded: `seat` has a known play or known trash to do instead of discarding chop.
    pub fn is_loaded(&self, seat: Seat) -> bool {
        self.state.hand(seat).iter().any(|&c| {
            let note = self.note(c);
            (note.play && note.delayed == 0) || note.trash
        })
    }

    /// The Loaded Play Clue Assumption (white): with a colourless suit, a 3 or 4 clue to a
    /// loaded player cannot be a save, since a loaded player does not discard.
    pub fn loaded_play_clue(&self, receiver: Seat, clue: ClueId) -> bool {
        self.rule(Rule::WhiteLoadedSave)
            && (self.is_rank_clue(clue, 3) || self.is_rank_clue(clue, 4))
            && self.is_loaded(receiver)
    }

    // ----- 1s order -----

    fn position(&self, seat: Seat, card: CardId) -> u8 {
        self.newest_first(seat)
            .find(|&(_, c)| c == card)
            .map(|(p, _)| p)
            .unwrap_or(u8::MAX)
    }

    /// Ordering key for playing 1s: a chop-focused 1 first, then fresh 1s newest first, then
    /// starting-hand 1s oldest first.
    pub fn one_order(&self, seat: Seat, card: CardId) -> (u8, u8) {
        let pos = self.position(seat, card);
        if !self.rule(Rule::OnesOrder) {
            return (0, pos);
        }
        let note = self.note(card);
        if note.one_chop_focus {
            (0, pos)
        } else if !note.starting {
            (1, pos)
        } else {
            (2, u8::MAX - pos)
        }
    }

    pub fn first_one(&self, seat: Seat, cards: CardSet) -> Option<CardId> {
        cards.iter().min_by_key(|&c| self.one_order(seat, c))
    }

    pub(super) fn is_rank_belief(&self, belief: &Belief, rank: u8) -> bool {
        belief.soft.is_subset(self.variant.rank_identities(rank))
    }

    /// The card `seat` will play next according to this view.
    pub fn next_play_for(&self, seat: Seat) -> Option<CardId> {
        let playable_set = self.playable_set();
        let trash_set = self.trash_set();
        let mut playable: SmallVec<[CardId; 6]> = SmallVec::new();
        for (_, card) in self.newest_first(seat) {
            let mut note = *self.note(card);
            if note.clued {
                note.update_attributes(playable_set, trash_set);
            }
            if !note.trash && note.play && note.delayed == 0 && note.belief.intersects(playable_set)
            {
                playable.push(card);
            }
        }
        // Finessed (blind) cards come first: they prove the finesse, so teammates waiting on
        // it learn where the connection is, before any clued known play.
        if let Some(&blind) = playable.iter().find(|&&c| {
            let note = self.note(c);
            !note.clued && note.promised.is_some()
        }) {
            return Some(blind);
        }
        let first = *playable.first()?;
        // Clued 1s retain their agreed order even when another clue queues one of them.
        let note = self.note(first);
        if !note.clued || !self.is_rank_belief(&note.belief, 1) {
            return Some(first);
        }
        let ones: CardSet = playable
            .iter()
            .copied()
            .filter(|&c| self.note(c).clued && self.is_rank_belief(&self.note(c).belief, 1))
            .collect();
        self.first_one(seat, ones)
    }

    pub fn next_play(&self) -> Option<CardId> {
        self.next_play_for(self.me)
    }

    // ----- fixes -----

    /// Classify a clue as a fix using the receiver's knowledge before it.
    pub fn fix_kind(&self, receiver: Seat, clue: ClueId, touched: CardSet) -> Option<Fix> {
        if !self.rule(Rule::Fix) {
            return None;
        }
        let touch = self.variant.touch(clue);
        let mut informative = false;
        let mut conflicts = CardSet::EMPTY;
        for &card in self.state.hand(receiver) {
            let note = self.note(card);
            let mut narrowed = note.belief;
            narrowed.apply_clue(touch, touched.contains(card));
            informative |= touched.contains(card) && narrowed.hard_size() < note.belief.hard_size();
            if (note.clued || note.promised.is_some())
                && narrowed.hard_size() > 0
                && narrowed.size() == 0
                && (note.belief.size() > 0
                    || (touched.contains(card) && narrowed.hard_size() < note.belief.hard_size()))
            {
                conflicts.insert(card);
            }
        }
        if !conflicts.is_empty() {
            return Some(Fix::Literal(conflicts));
        }
        if informative
            || !self.newly_touched(receiver, touched).is_empty()
            || !self.rule(Rule::Stop)
        {
            return None;
        }
        let next = self.next_play_for(receiver)?;
        if touched.contains(next)
            && self.stop_options(receiver, next).size() > 0
            && touched.iter().all(|c| {
                let note = self.note(c);
                note.clued && (note.trash || (note.play && note.delayed == 0))
            })
        {
            return Some(Fix::Stop(next));
        }
        None
    }

    /// Another protected card of `identity` exists, so `card` would be a duplicate.
    pub fn protected_duplicate(&self, seat: Seat, card: CardId, identity: Identity) -> bool {
        self.state.seats().any(|holder| {
            self.state.hand(holder).iter().any(|&other| {
                let note = self.note(other);
                other != card
                    && note.clued
                    && !note.stopped
                    && (if holder == self.me || holder == seat {
                        note.belief.size() == 1 && note.belief.contains(identity)
                    } else {
                        self.identity(other) == Some(identity)
                    })
            })
        })
    }

    pub fn stop_options(&self, seat: Seat, card: CardId) -> Belief {
        let mut options = self.note(card).belief;
        options.reset_soft();
        let trash = self.trash_set();
        for identity in options.soft.iter() {
            if !trash.contains(identity) && !self.protected_duplicate(seat, card, identity) {
                options.remove(identity, true);
            }
        }
        options
    }

    /// A mistakenly duplicated direct play still identifies the card, so its receiver can
    /// return the copy with a sarcastic discard.
    pub fn duplicated_play(&self, seat: Seat, card: CardId) -> Option<Identity> {
        if !self.rule(Rule::DuplicatedPlay) {
            return None;
        }
        let mut playable = self.note(card).belief;
        playable.soft_limit(self.playable_set());
        let identity = playable.identity()?;
        self.state
            .seats()
            .filter(|&holder| holder != seat)
            .any(|holder| {
                self.state.hand(holder).iter().any(|&other| {
                    let note = self.note(other);
                    note.clued
                        && !note.stopped
                        && (if holder == self.me {
                            note.belief.size() == 1 && note.belief.contains(identity)
                        } else {
                            self.identity(other) == Some(identity)
                        })
                })
            })
            .then_some(identity)
    }

    /// Cards of `receiver` that `clue` touches, as far as this view can see them.
    pub fn visible_touched(&self, receiver: Seat, clue: ClueId) -> CardSet {
        let touch = self.variant.touch(clue);
        self.state
            .hand(receiver)
            .iter()
            .copied()
            .filter(|&c| self.identity(c).is_some_and(|i| touch.contains(i)))
            .collect()
    }

    pub fn clue_fixes(&self, receiver: Seat, clue: ClueId) -> bool {
        self.fix_kind(receiver, clue, self.visible_touched(receiver, clue))
            .is_some()
    }

    // ----- sarcastic discards -----

    pub(super) fn receive_sarcastic(&mut self, seat: Seat, card: CardId, identity: Identity) {
        if !self.note(card).clued || self.trash_set().contains(identity) {
            return;
        }
        let mut candidates = SmallVec::new();
        for holder in self.state.seats().filter(|&h| h != seat) {
            for &other in self.state.hand(holder) {
                let note = self.note(other);
                if note.clued
                    && !note.stopped
                    && note.belief.contains_hard(identity)
                    && (note.belief.contains(identity) || note.belief.size() != 1)
                {
                    candidates.push(other);
                }
            }
        }
        if !candidates.is_empty() {
            self.sarcastic.push(SarcasticTransfer {
                identity,
                candidates,
            });
        }
    }

    pub(super) fn refresh_sarcastic(&mut self) {
        let trash = self.trash_set();
        let mut transfers = std::mem::take(&mut self.sarcastic);
        transfers.retain_mut(|transfer| {
            if trash.contains(transfer.identity) {
                return false;
            }
            transfer.candidates.retain(|card| {
                self.state.holder(*card).is_some() && {
                    let note = self.note(*card);
                    !note.stopped && note.belief.contains_hard(transfer.identity)
                }
            });
            !transfer.candidates.is_empty()
        });
        for transfer in &transfers {
            if let [card] = transfer.candidates.as_slice() {
                let card = *card;
                let note = self.note_mut(card);
                note.base.reset_soft();
                note.base
                    .soft_limit(crate::domain::IdentitySet::single(transfer.identity));
                // The public discard corrects a duplicate's previous deduction.
                let identity = transfer.identity;
                self.interpretations
                    .retain_consistent(|step| step.card != card || step.identity == identity);
            }
        }
        self.sarcastic = transfers;
    }

    /// A disjunction cannot identify either card: keep literal choices until another public
    /// observation singles out a target.
    pub(super) fn apply_sarcastic_ambiguity(&mut self) {
        let transfers = self.sarcastic.clone();
        for transfer in &transfers {
            if transfer.candidates.len() < 2 {
                continue;
            }
            for &card in &transfer.candidates {
                if self.state.holder(card).is_some() {
                    let note = self.note_mut(card);
                    if note.belief.hard_size() > 1 {
                        note.belief.reset_soft();
                        note.play = false;
                        note.trash = false;
                    }
                }
            }
        }
    }

    /// A clued card of ours that duplicates a clued card elsewhere, to discard sarcastically.
    pub fn sarcastic_discard(&self) -> Option<CardId> {
        if !self.rule(Rule::SarcasticDiscard) {
            return None;
        }
        let trash = self.trash_set();
        self.newest_first(self.me).find_map(|(_, card)| {
            let note = self.note(card);
            if !note.clued || note.promised.is_some() || note.stopped {
                return None;
            }
            let mut belief = note.belief;
            if belief.size() == 0 {
                belief.reset_soft();
            }
            let identity = belief.identity()?;
            if trash.contains(identity) {
                return None;
            }
            // Layered Finesse Dupe Clue: a copy already promised to a blind play elsewhere
            // (a layer, or a finesse) makes ours a duplicate as well.
            let dupe = self.rule(Rule::LayeredFinesseDupe);
            self.others()
                .any(|seat| {
                    self.state.hand(seat).iter().any(|&other| {
                        let n = self.note(other);
                        self.identity(other) == Some(identity)
                            && ((n.clued && !n.stopped && n.belief.contains_hard(identity))
                                || (dupe && !n.clued && n.promised.is_some()))
                    })
                })
                .then_some(card)
        })
    }

    /// The convention discard: sarcastic, known trash, chop, then the card least likely to
    /// matter.
    pub fn discard_choice(&self) -> CardId {
        if let Some(card) = self.sarcastic_discard() {
            return card;
        }
        let mut chop = None;
        for (_, card) in self.newest_first(self.me) {
            let note = self.note(card);
            if note.trash {
                return card;
            }
            if !note.touched() {
                chop = Some(card);
            }
        }
        if let Some(card) = chop {
            return card;
        }
        // Locked hand: drop the card most likely to be a high rank.
        for rank in (1..=self.variant.num_ranks).rev() {
            for (_, card) in self.newest_first(self.me) {
                if self.is_rank_belief(&self.note(card).belief, rank) {
                    return card;
                }
            }
        }
        self.newest_first(self.me)
            .next()
            .map(|(_, c)| c)
            .expect("hand is not empty")
    }
}
