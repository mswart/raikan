//! A viewpoint's interpretation of the game: notes per card, live clue records and the
//! turn context, updated by folding observed events.
//!
//! Addressing is absolute: seats are table seats and cards are deck orders, so a view of
//! another seat is the same data with a different `me` and sight; nothing is rotated.

use std::sync::Arc;

use smallvec::SmallVec;

use super::belief::Belief;
use super::profile::{Profile, Rule};
use super::records::{ClueRecord, HandEntry, Hypothesis, Interpretations, Step, StepKind};
use crate::domain::{CardId, CardSet, Identity, IdentitySet, MAX_CARDS, MAX_IDENTITIES, Seat};
use crate::engine::{self, Event, GameState};
use crate::knowledge::{ConstraintStore, ConventionConstraint, KnownCards, PublicCards, Sight};
use crate::variants::Variant;

/// What a viewpoint concludes about one card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CardNote {
    /// Drawn before the first action.
    pub starting: bool,
    /// A rank-1 clue focused this card on chop (it plays first among 1s).
    pub one_chop_focus: bool,
    /// A stop clue told the holder not to play it.
    pub stopped: bool,
    pub clued: bool,
    pub play: bool,
    pub trash: bool,
    /// Current belief including live interpretations.
    pub belief: Belief,
    /// Clue information and good-touch deductions, before live interpretations.
    pub base: Belief,
    pub locked: bool,
    pub fixed: bool,
    /// Number of actions completed when the card was drawn.
    pub drawn_at: i16,
    /// Plays that must happen before this card is played.
    pub delayed: u8,
    /// Turn of the record that promised a blind play of this card.
    pub promised: Option<i16>,
    /// A chop move (level 4 and later) protects this unclued card permanently.
    pub chop_moved: bool,
}

impl CardNote {
    fn new(all: IdentitySet) -> Self {
        Self {
            starting: false,
            one_chop_focus: false,
            stopped: false,
            clued: false,
            play: false,
            trash: false,
            belief: Belief::new(all),
            base: Belief::new(all),
            locked: false,
            fixed: false,
            drawn_at: 0,
            delayed: 0,
            promised: None,
            chop_moved: false,
        }
    }

    /// Clued, promised or chop moved: neither on chop nor in finesse position.
    pub fn touched(&self) -> bool {
        self.clued || self.promised.is_some() || self.chop_moved
    }

    pub(super) fn remove(&mut self, identity: Identity, soft_only: bool) {
        self.belief.remove(identity, soft_only);
        self.base.remove(identity, soft_only);
    }

    pub(super) fn apply_clue(&mut self, touch: IdentitySet, touched: bool) {
        self.belief.apply_clue(touch, touched);
        self.base.apply_clue(touch, touched);
    }

    /// Recompute `play` and `trash` from the belief.
    pub fn update_attributes(&mut self, playable: IdentitySet, trash: IdentitySet) {
        if self.stopped {
            self.trash = true;
            self.play = false;
            self.delayed = 0;
            self.promised = None;
            return;
        }
        if self.delayed > 0 {
            return;
        }
        if self.belief.size() == 0 {
            self.trash = true;
            self.play = false;
            return;
        }
        if !self.belief.intersects(playable) {
            self.play = false;
        }
        if self.belief.is_subset_of(playable) {
            self.play = true;
            self.trash = false;
        }
        if self.belief.is_subset_of(trash) {
            self.trash = true;
        }
        if self.trash {
            self.play = false;
        }
    }
}

/// Where a viewpoint knows an identity to be clued.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clued {
    /// Clued in this seat's hand (from the viewpoint's sight).
    Holder(Seat),
    /// Everybody knows where it is (played, or a clued card everybody can identify).
    Everyone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct IdentityInfo {
    pub clued: Option<Clued>,
    /// The card everybody knows to be this identity.
    pub locked: Option<(Seat, CardId)>,
}

/// Play state of an identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayState {
    Normal,
    Playable,
    Critical,
    CriticalPlayable,
    /// Already played.
    Trash,
    /// Can never be played (all copies of it or of a lower rank are gone).
    Dead,
}

/// Rule-owned public facts used by stalling and the early game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TurnContext {
    pub early_game: bool,
    pub early_five_stalled: bool,
    /// The previous action discarded or misplayed this identity.
    pub last_loss: Option<(Seat, Identity)>,
}

impl Default for TurnContext {
    fn default() -> Self {
        Self {
            early_game: true,
            early_five_stalled: false,
            last_loss: None,
        }
    }
}

/// A transfer of identity by a sarcastic discard, possibly still ambiguous.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SarcasticTransfer {
    pub identity: Identity,
    pub candidates: SmallVec<[CardId; 5]>,
}

/// Inferences before a clue, kept to undo a mistaken clue after a strike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub turn: i16,
    pub notes: Box<[CardNote; MAX_CARDS]>,
    pub previous: Option<Arc<Snapshot>>,
}

#[derive(Clone)]
pub struct View {
    pub variant: Arc<Variant>,
    pub profile: Arc<Profile>,
    /// The seat whose view this is.
    pub me: Seat,
    /// Public state of the game.
    pub state: GameState,
    pub public: PublicCards,
    /// Identities this view can see.
    pub known: KnownCards,
    pub notes: Box<[CardNote; MAX_CARDS]>,
    pub interpretations: Interpretations,
    pub sarcastic: SmallVec<[SarcasticTransfer; 2]>,
    pub context: TurnContext,
    pub info: [IdentityInfo; MAX_IDENTITIES],
    /// Number of turn actions processed (records are stamped with it).
    pub turn: i16,
    pub(super) history: Option<Arc<Snapshot>>,
    sets: PlaySets,
    /// The rule that classified the last clue (for traces).
    pub(super) last_rule: &'static str,
    /// Interpretation trace, collected when enabled with [`View::enable_trace`].
    pub trace: TraceLog,
    /// One view per seat, fed only what that seat observes (their own cards hidden, our
    /// own cards unknown). Only the agent's root view keeps them; simulation clones drop
    /// them (see [`View::detached`]).
    seats: Option<Arc<Vec<View>>>,
    /// The receiver's view from before our own clue, to check how the receiver reads it.
    pub(super) pending_receiver: Option<Box<View>>,
    /// In a kept seat view: possible identities of cards this seat sees but our model cannot
    /// identify (the root seat's own cards). Empty for every other card.
    hints: Box<[IdentitySet; MAX_CARDS]>,
}

/// Trace entries of a view. Clones (simulations) never inherit or extend a trace.
#[derive(Default)]
pub struct TraceLog(Option<Vec<crate::trace::InterpretationTrace>>);

impl Clone for TraceLog {
    fn clone(&self) -> Self {
        Self(None)
    }
}

impl TraceLog {
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn push(&mut self, entry: crate::trace::InterpretationTrace) {
        if let Some(log) = &mut self.0 {
            log.push(entry);
        }
    }

    /// Remove and return the collected entries.
    pub fn take(&mut self) -> Vec<crate::trace::InterpretationTrace> {
        self.0.as_mut().map(std::mem::take).unwrap_or_default()
    }
}

/// Play-state sets of the current stacks and discards, cached per card movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlaySets {
    playable: IdentitySet,
    played: IdentitySet,
    trash: IdentitySet,
    critical: IdentitySet,
}

impl PlaySets {
    fn new(variant: &Variant, state: &GameState) -> Self {
        let lost: IdentitySet = variant
            .all_identities()
            .iter()
            .filter(|&i| state.discarded[i.index()] >= variant.copies(i))
            .collect();
        let played: IdentitySet = variant.played(&state.stacks);
        let trash = variant.trash(&state.stacks, &state.discarded) | lost;
        let critical = variant
            .all_identities()
            .iter()
            .filter(|&i| variant.copies(i).saturating_sub(state.discarded[i.index()]) == 1)
            .collect();
        Self {
            playable: variant.playable(&state.stacks),
            played,
            trash,
            critical,
        }
    }
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "View of seat {} (turn {})", self.me.0, self.turn)?;
        for seat in self.state.seats() {
            write!(f, "  {}: [", seat.0)?;
            for (pos, card) in self.newest_first(seat) {
                if pos > 0 {
                    f.write_str(", ")?;
                }
                let note = &self.notes[card.index()];
                match self.identity(card) {
                    Some(i) => write!(f, "{}", self.variant.identity_name(i))?,
                    None => f.write_str("??")?,
                }
                let names: Vec<String> = note
                    .belief
                    .soft
                    .iter()
                    .map(|i| self.variant.identity_name(i))
                    .collect();
                write!(f, " #{} {{{}}}", card.0, names.join(","))?;
                if note.clued {
                    f.write_str("'")?;
                }
                if note.trash {
                    f.write_str(" kt")?;
                } else if note.delayed > 0 {
                    write!(f, " +{}", note.delayed)?;
                } else if note.play {
                    f.write_str(" !")?;
                }
                if note.promised.is_some() {
                    f.write_str(" f")?;
                }
            }
            writeln!(f, "]")?;
        }
        for record in &self.interpretations.records {
            writeln!(f, "  {record:?}")?;
        }
        Ok(())
    }
}

impl View {
    pub fn new(variant: Arc<Variant>, profile: Arc<Profile>, num_seats: u8, me: Seat) -> Self {
        let all = variant.all_identities();
        Self {
            state: GameState::new(&variant, num_seats),
            public: PublicCards::new(&variant),
            known: KnownCards::default(),
            notes: Box::new([CardNote::new(all); MAX_CARDS]),
            interpretations: Interpretations::default(),
            sarcastic: SmallVec::new(),
            context: TurnContext::default(),
            info: [IdentityInfo::default(); MAX_IDENTITIES],
            turn: 0,
            history: None,
            sets: PlaySets::new(&variant, &GameState::new(&variant, num_seats)),
            last_rule: "",
            trace: TraceLog::default(),
            seats: None,
            pending_receiver: None,
            hints: Box::new([IdentitySet::EMPTY; MAX_CARDS]),
            variant,
            profile,
            me,
        }
    }

    /// A root view that also keeps the view of every other seat, so that predictions of
    /// teammates never depend on cards those teammates cannot see.
    pub fn with_seat_views(
        variant: Arc<Variant>,
        profile: Arc<Profile>,
        num_seats: u8,
        me: Seat,
    ) -> Self {
        let seats = (0..num_seats)
            .map(|s| View::new(variant.clone(), profile.clone(), num_seats, Seat(s)))
            .collect();
        let mut view = View::new(variant, profile, num_seats, me);
        view.seats = Some(Arc::new(seats));
        view
    }

    /// A copy for simulations: without the per-seat views, which hypothetical events would
    /// otherwise have to update.
    pub fn detached(&self) -> View {
        let mut view = self.clone();
        view.seats = None;
        view.pending_receiver = None;
        view
    }

    /// Start collecting an interpretation trace of observed clues.
    pub fn enable_trace(&mut self) {
        self.trace.0.get_or_insert_with(Vec::new);
    }

    pub fn rule(&self, rule: Rule) -> bool {
        self.profile.is_active(rule)
    }

    pub fn num_seats(&self) -> u8 {
        self.state.num_seats
    }

    /// Seats other than `me`, in table order starting after `me`.
    pub fn others(&self) -> impl Iterator<Item = Seat> + use<> {
        let me = self.me;
        let n = self.state.num_seats;
        (1..n).map(move |offset| me.after(offset, n))
    }

    /// All seats in table order starting with `me`.
    pub fn seats_from_me(&self) -> impl Iterator<Item = Seat> + use<> {
        let me = self.me;
        let n = self.state.num_seats;
        (0..n).map(move |offset| me.after(offset, n))
    }

    /// The identity of a card if this view can see it.
    pub fn identity(&self, card: CardId) -> Option<Identity> {
        if self.state.hand(self.me).contains(&card) {
            None
        } else {
            self.known.get(card)
        }
    }

    /// A hand newest first with positions (0 = newest), as hand positions are counted.
    pub fn newest_first(
        &self,
        seat: Seat,
    ) -> impl DoubleEndedIterator<Item = (u8, CardId)> + Clone + '_ {
        self.state
            .hand(seat)
            .iter()
            .rev()
            .enumerate()
            .map(|(pos, &card)| (pos as u8, card))
    }

    pub fn hand_len(&self, seat: Seat) -> u8 {
        self.state.hand(seat).len() as u8
    }

    pub fn note(&self, card: CardId) -> &CardNote {
        &self.notes[card.index()]
    }

    pub fn note_mut(&mut self, card: CardId) -> &mut CardNote {
        &mut self.notes[card.index()]
    }

    // ----- play states -----

    pub fn playable_set(&self) -> IdentitySet {
        self.sets.playable
    }

    /// Played identities, and identities that can no longer be played (including the lost
    /// identity itself).
    pub fn trash_set(&self) -> IdentitySet {
        self.sets.trash
    }

    pub fn play_state(&self, identity: Identity) -> PlayState {
        let sets = &self.sets;
        if sets.played.contains(identity) {
            PlayState::Trash
        } else if sets.trash.contains(identity) {
            PlayState::Dead
        } else {
            match (
                sets.critical.contains(identity),
                sets.playable.contains(identity),
            ) {
                (true, true) => PlayState::CriticalPlayable,
                (true, false) => PlayState::Critical,
                (false, true) => PlayState::Playable,
                (false, false) => PlayState::Normal,
            }
        }
    }

    pub fn is_critical(&self, identity: Identity) -> bool {
        matches!(
            self.play_state(identity),
            PlayState::Critical | PlayState::CriticalPlayable
        )
    }

    pub fn score(&self) -> u8 {
        self.state.score(&self.variant)
    }

    // ----- observing events -----

    /// Fold one observed event into the view (and into the kept views of the other seats).
    pub fn observe(&mut self, event: &Event) {
        self.observe_own(event);
        if let Some(seats) = &mut self.seats {
            let me = self.me;
            // Others see our cards; tell their views the ones our notes identify.
            let identified: SmallVec<[(CardId, Identity); 6]> = self
                .state
                .hand(me)
                .iter()
                .filter_map(|&c| self.notes[c.index()].belief.identity().map(|i| (c, i)))
                .collect();
            let hints: SmallVec<[(CardId, IdentitySet); 6]> = self
                .state
                .hand(me)
                .iter()
                .map(|&c| {
                    let belief = self.notes[c.index()].belief;
                    (
                        c,
                        if belief.soft.is_empty() {
                            belief.hard
                        } else {
                            belief.soft
                        },
                    )
                })
                .collect();
            for view in Arc::make_mut(seats).iter_mut().filter(|v| v.me != me) {
                for &(card, identity) in &identified {
                    if view.known.get(card).is_none() {
                        view.known.set(card, identity);
                    }
                }
                for &(card, hint) in &hints {
                    view.hints[card.index()] = hint;
                }
                // Another seat does not see its own draws; we cannot tell it ours.
                let seen = match *event {
                    Event::Draw { seat, card, .. } if seat == view.me => Event::Draw {
                        seat,
                        card,
                        identity: None,
                    },
                    other => other,
                };
                view.observe(&seen);
            }
        }
    }

    fn observe_own(&mut self, event: &Event) {
        match *event {
            Event::Draw {
                seat,
                card,
                identity,
            } => {
                self.drawn(seat, card, identity);
            }
            Event::Play {
                seat,
                card,
                identity,
                success,
            } => {
                self.played(
                    seat,
                    card,
                    identity.expect("plays are public"),
                    success,
                    event,
                );
            }
            Event::Discard {
                seat,
                card,
                identity,
            } => {
                self.discarded(seat, card, identity.expect("discards are public"), event);
            }
            Event::Clue {
                giver,
                target,
                clue,
                touched,
            } => {
                self.clued(giver, target, clue, touched, event);
            }
            Event::GameOver { .. } => {
                engine::apply_event(&self.variant, &mut self.state, event);
            }
        }
    }

    pub(super) fn apply_public(&mut self, event: &Event) {
        let before = self.state;
        self.public.apply(&self.variant, &before, event);
        self.known.observe(event);
        engine::apply_event(&self.variant, &mut self.state, event);
        if matches!(event, Event::Play { .. } | Event::Discard { .. }) {
            self.sets = PlaySets::new(&self.variant, &self.state);
        }
    }

    /// Copies of `identity` known to this view outside `seat`'s hand, and whether any is
    /// in that hand.
    fn copies_known_outside(&self, identity: Identity, seat: Seat) -> (u8, bool) {
        let hand = self.state.hand_set(seat);
        let mut outside = 0;
        let mut inside = false;
        for card in self.state.dealt().iter() {
            if self.known.get(card) == Some(identity) {
                if hand.contains(card) {
                    inside = true;
                } else {
                    outside += 1;
                }
            }
        }
        (outside, inside)
    }

    /// Per identity: known copies outside `seat`'s hand, and copies known inside it.
    fn known_split(&self, seat: Seat) -> ([u8; MAX_IDENTITIES], [u8; MAX_IDENTITIES]) {
        let hand = self.state.hand_set(seat);
        let mut outside = [0u8; MAX_IDENTITIES];
        let mut inside = [0u8; MAX_IDENTITIES];
        for card in self.state.dealt().iter() {
            if let Some(identity) = self.known.get(card) {
                if hand.contains(card) {
                    inside[identity.index()] += 1;
                } else {
                    outside[identity.index()] += 1;
                }
            }
        }
        (outside, inside)
    }

    fn drawn(&mut self, seat: Seat, card: CardId, identity: Option<Identity>) {
        let event = Event::Draw {
            seat,
            card,
            identity,
        };
        self.apply_public(&event);
        let mut note = CardNote::new(self.variant.all_identities());
        note.starting = self.state.turn == 0;
        note.drawn_at = self.state.turn as i16;
        let (outside, inside) = self.known_split(seat);
        for i in self.variant.all_identities().iter() {
            if outside[i.index()] >= self.variant.copies(i) && inside[i.index()] == 0 {
                note.remove(i, false);
            }
        }
        self.notes[card.index()] = note;
        if let Some(identity) = self.identity(card) {
            self.track(identity);
        }
    }

    /// Once every copy of an identity is known, remove it from the hands of the seats that
    /// can see all of them.
    fn track(&mut self, identity: Identity) {
        let total: u8 = {
            let mut n = 0;
            for card in self.state.dealt().iter() {
                if self.known.get(card) == Some(identity) {
                    n += 1;
                }
            }
            n
        };
        if total < self.variant.copies(identity) {
            return;
        }
        let playable = self.playable_set();
        let trash = self.trash_set();
        for seat in self.state.seats() {
            let (_, inside) = self.copies_known_outside(identity, seat);
            if inside {
                continue;
            }
            for &card in self.state.hand(seat) {
                if self.identity(card) != Some(identity) {
                    let note = &mut self.notes[card.index()];
                    note.remove(identity, false);
                    note.update_attributes(playable, trash);
                }
            }
        }
    }

    /// Hand of `seat` without `removed`, newest first, as reactions need it.
    pub(super) fn hand_after(&self, seat: Seat, removed: CardId) -> SmallVec<[HandEntry; 6]> {
        self.newest_first(seat)
            .filter(|&(_, card)| card != removed)
            .map(|(_, card)| {
                let note = &self.notes[card.index()];
                HandEntry {
                    card,
                    clued: note.clued,
                    belief: note.base,
                    drawn_at: note.drawn_at,
                    chop_moved: note.chop_moved,
                    starting: note.starting,
                    one_chop_focus: note.one_chop_focus,
                }
            })
            .collect()
    }

    pub(super) fn ones_for_reactions(&self) -> Option<IdentitySet> {
        self.rule(Rule::OnesOrder)
            .then(|| self.variant.rank_identities(1))
    }

    fn played(
        &mut self,
        seat: Seat,
        card: CardId,
        identity: Identity,
        successful: bool,
        event: &Event,
    ) {
        if successful {
            self.sarcastic
                .retain(|transfer| transfer.identity != identity);
        }
        self.context.last_loss = (!successful).then_some((seat, identity));
        self.turn += 1;
        let note = self.notes[card.index()];
        let hand_after = self.hand_after(seat, card);
        // Order chop move: playing an unknown 1 out of the agreed order moves a chop.
        let order_chop_move = if successful {
            self.order_chop_move_target(seat, card)
                .and_then(|target| self.chop(target))
        } else {
            None
        };
        if !successful && self.rule(Rule::StrikeRecovery) {
            self.recover_strike(seat, card, note, &hand_after);
        }
        self.apply_public(event);
        let ones = self.ones_for_reactions();
        self.interpretations
            .on_play(seat, card, identity, successful, &hand_after, ones);
        if let Some(chop) = order_chop_move {
            self.notes[chop.index()].chop_moved = true;
        }
        self.track(identity);
        if successful {
            for other in self.state.seats() {
                for &c in self.state.hand(other) {
                    let n = &mut self.notes[c.index()];
                    if n.clued && !n.locked {
                        n.remove(identity, true);
                    }
                }
            }
        }
        self.refresh_derived();
        self.refresh_clued_cards();
    }

    fn discarded(&mut self, seat: Seat, card: CardId, identity: Identity, event: &Event) {
        if self.rule(Rule::SarcasticDiscard) {
            self.receive_sarcastic(seat, card, identity);
        }
        let note = self.notes[card.index()];
        if !note.clued && note.promised.is_none() && !note.trash {
            self.context.early_game = false;
        }
        self.context.last_loss = Some((seat, identity));
        self.turn += 1;
        self.interpretations.on_non_play(seat);
        self.interpretations.on_discard(card);
        self.apply_public(event);
        self.track(identity);
        self.refresh_derived();
        self.refresh_clued_cards();
    }

    fn recover_strike(
        &mut self,
        seat: Seat,
        played: CardId,
        played_note: CardNote,
        hand_after: &[HandEntry],
    ) {
        let affected: Vec<ClueRecord> = self
            .interpretations
            .records
            .iter()
            .filter(|record| {
                record.hypotheses.iter().any(|h| {
                    h.pending_steps().any(|step| {
                        step.card == played
                            || (step.holder == seat
                                && self.layer_cards(record, h, step).contains(&played))
                    })
                })
            })
            .cloned()
            .collect();
        let suspected = (!played_note.clued)
            .then(|| {
                self.interpretations
                    .records
                    .iter()
                    .filter(|record| {
                        record.giver != seat
                            && record.receiver != seat
                            && record.active().any(|h| {
                                h.expected_now()
                                    .is_some_and(|step| step.kind == StepKind::Focus)
                            })
                    })
                    .map(|r| r.turn)
                    .min()
            })
            .flatten();
        let Some(turn) = affected
            .iter()
            .map(|r| r.turn)
            .min()
            .or(suspected)
            .or_else(|| self.history.as_ref().map(|s| s.turn))
        else {
            return;
        };

        // A wrong prompt is the only exception to withdrawing the mistaken clue. Require its
        // original finesse card to still exist and respect all literal clues; never
        // substitute the card drawn after the misplay.
        let wrong_prompt = self.rule(Rule::WrongPrompt);
        let playable = self.playable_set();
        let mut recovered = Vec::new();
        for mut record in affected {
            record.hypotheses.retain_mut(|h| {
                if !wrong_prompt {
                    return false;
                }
                let Some(step) = h.expected_now() else {
                    return false;
                };
                if step.card != played || step.kind != StepKind::Prompt {
                    return false;
                }
                let Some(fallback) = step.fallback else {
                    return false;
                };
                let Some(entry) = hand_after.iter().find(|entry| entry.card == fallback) else {
                    return false;
                };
                if entry.clued
                    || entry.chop_moved
                    || !entry.belief.contains_hard(step.identity)
                    || !playable.contains(step.identity)
                {
                    return false;
                }
                if seat != self.me && self.identity(fallback).is_some_and(|i| i != step.identity) {
                    return false;
                }
                let step = h
                    .steps
                    .iter_mut()
                    .find(|s| !s.done && s.card == played)
                    .unwrap();
                step.card = fallback;
                step.kind = StepKind::Finesse;
                step.fallback = None;
                step.layers = 0;
                step.known_play = false;
                // In the holder's view a later finesse may have occupied the same unknown
                // card. Shift that suffix across the original hand, leaving fixed prompts and
                // other clues' cards reserved.
                let mut used: Vec<CardId> = h
                    .pending_steps()
                    .filter(|s| s.kind != StepKind::Finesse || s.holder != seat)
                    .map(|s| s.card)
                    .collect();
                used.extend(
                    self.interpretations
                        .records
                        .iter()
                        .filter(|r| r.turn < turn)
                        .flat_map(|r| r.active())
                        .flat_map(|other| other.pending_steps())
                        .map(|s| s.card),
                );
                if used.contains(&fallback) {
                    return false;
                }
                for step in h
                    .steps
                    .iter_mut()
                    .filter(|s| !s.done && s.holder == seat && s.kind == StepKind::Finesse)
                {
                    if used.contains(&step.card) {
                        let Some(next) = hand_after.iter().find(|entry| {
                            !entry.clued
                                && !entry.chop_moved
                                && entry.drawn_at <= record.turn
                                && !used.contains(&entry.card)
                        }) else {
                            return false;
                        };
                        if !next.belief.contains_hard(step.identity) {
                            return false;
                        }
                        step.card = next.card;
                    }
                    used.push(step.card);
                }
                true
            });
            if !record.hypotheses.is_empty() {
                recovered.push(record);
            }
        }
        let snapshot = std::iter::successors(self.history.as_deref(), |s| s.previous.as_deref())
            .take_while(|s| s.turn >= turn)
            .last();
        for seat in self.state.seats() {
            for &card in self.state.hand(seat) {
                let previous = snapshot.map(|s| s.notes[card.index()]);
                let note = &mut self.notes[card.index()];
                note.base.reset_soft();
                // a card drawn after the snapshot has no earlier inference
                match previous.filter(|p| {
                    p.drawn_at == note.drawn_at && snapshot.is_some_and(|s| note.drawn_at <= s.turn)
                }) {
                    Some(previous) => {
                        note.base.soft_limit(previous.base.soft);
                        note.locked = previous.locked;
                        note.fixed = previous.fixed;
                        note.stopped = previous.stopped;
                        note.one_chop_focus = previous.one_chop_focus;
                        note.chop_moved = previous.chop_moved;
                    }
                    None => {
                        note.chop_moved = false;
                        note.locked = false;
                        note.fixed = false;
                        note.stopped = false;
                        note.one_chop_focus = false;
                    }
                }
            }
        }
        self.interpretations.records.retain(|r| r.turn < turn);
        self.interpretations.records.extend(recovered);
        while self.history.as_ref().is_some_and(|s| s.turn > turn) {
            self.history = self.history.as_ref().unwrap().previous.clone();
        }
    }

    // ----- derived notes -----

    /// The identity of a record's focus as this view sees it (never for its own cards).
    pub(super) fn known_focus(&self, record: &ClueRecord) -> Option<Identity> {
        let holder = self.state.holder(record.focus)?;
        if holder == self.me {
            None
        } else {
            self.identity(record.focus)
        }
    }

    /// Rebuild the inferred notes from clue evidence and the remaining readings.
    pub(super) fn refresh_derived(&mut self) {
        self.refresh_sarcastic();
        let trash = self.trash_set();
        let playable = self.playable_set();
        for seat in self.state.seats() {
            for &card in self.state.hand(seat) {
                let note = &mut self.notes[card.index()];
                if note.clued && self.profile.is_active(Rule::GoodTouch) {
                    note.base.soft_exclude(trash);
                }
                note.belief = note.base;
                note.play = false;
                note.trash = false;
                note.delayed = 0;
                note.promised = None;
                note.update_attributes(playable, trash);
            }
        }
        if self.interpretations.is_empty() {
            self.apply_sarcastic_ambiguity();
            return;
        }
        let records = std::mem::take(&mut self.interpretations.records);
        for record in &records {
            let known = self.known_focus(record);
            if let Some(hypothesis) = record.resolved() {
                let mut earlier = 0;
                for (index, step) in hypothesis.steps.iter().enumerate().filter(|(_, s)| !s.done) {
                    let delay = earlier + step.layers;
                    earlier += 1 + step.layers;
                    if (step.holder != record.receiver
                        && known.is_some_and(|i| i != hypothesis.focus_identity))
                        || hypothesis.steps[..index]
                            .iter()
                            .any(|previous| !previous.done && previous.card == step.card)
                    {
                        continue;
                    }
                    if let Some(holder) = self.state.holder(step.card) {
                        let note = &mut self.notes[step.card.index()];
                        note.belief.soft_limit(IdentitySet::single(step.identity));
                        note.delayed = note.delayed.max(delay);
                        if step.kind == StepKind::Finesse
                            || (holder == self.me && holder != record.receiver)
                        {
                            note.promised = Some(record.turn);
                        }
                        note.play = delay == 0;
                    }
                }
                continue;
            }
            let observer = record.receiver.after(1, self.num_seats());
            for seat in self.state.seats() {
                let relevant: Vec<&Hypothesis> = if seat == record.receiver {
                    record.for_seat(record.receiver, known).collect()
                } else {
                    record.for_seat(observer, known).collect()
                };
                if relevant.is_empty() {
                    continue;
                }
                let hand: SmallVec<[CardId; 6]> = self.state.hand(seat).iter().copied().collect();
                let mut cards = [IdentitySet::EMPTY; 6];
                let mut references = [0usize; 6];
                let mut finesses = [true; 6];
                let mut delays = [0u8; 6];
                let meanings = relevant.len();
                for hypothesis in &relevant {
                    let mut seen = 0u8;
                    let mut earlier = 0;
                    for step in hypothesis.pending_steps() {
                        if step.holder == seat
                            && let Some(index) = hand.iter().position(|&c| c == step.card)
                            && seen & (1 << index) == 0
                        {
                            seen |= 1 << index;
                            references[index] += 1;
                            cards[index].insert(step.identity);
                            finesses[index] &= step.kind == StepKind::Finesse;
                            delays[index] = delays[index].max(earlier + step.layers);
                        }
                        earlier += 1 + step.layers;
                    }
                }
                for (index, &card) in hand.iter().enumerate() {
                    let note = &mut self.notes[card.index()];
                    if references[index] == meanings {
                        note.belief.soft_limit(cards[index]);
                        note.delayed = note.delayed.max(delays[index]);
                        if finesses[index] || (seat == self.me && seat != record.receiver) {
                            note.promised = Some(record.turn);
                        }
                        note.play = delays[index] == 0;
                    } else if references[index] > 0 {
                        note.delayed = note.delayed.max(delays[index]);
                    }
                }
            }
        }
        for record in &records {
            let known = self.known_focus(record);
            if !record
                .hypotheses
                .iter()
                .any(|h| h.pending_steps().any(|s| s.layers > 0))
            {
                continue;
            }
            for seat in self.state.seats() {
                // The same card can be a layer in one meaning and a target in another.
                // Intersect their combined obligations, keeping the latest timing.
                let mut common: SmallVec<[(CardId, u8, bool); 5]> = SmallVec::new();
                for (index, hypothesis) in record.for_seat(seat, known).enumerate() {
                    let mut obligations: SmallVec<[(CardId, u8, bool); 5]> = SmallVec::new();
                    for step in hypothesis.pending_steps().filter(|s| s.holder == seat) {
                        let earlier = hypothesis.pending_before(step.card).unwrap_or(0)
                            + hypothesis
                                .pending_steps()
                                .take_while(|s| s.card != step.card)
                                .map(|s| s.layers)
                                .sum::<u8>();
                        for (offset, card) in self
                            .layer_cards(record, hypothesis, step)
                            .into_iter()
                            .enumerate()
                        {
                            obligations.push((card, earlier + offset as u8, true));
                        }
                        obligations.push((step.card, earlier + step.layers, false));
                    }
                    if index == 0 {
                        common = obligations;
                    } else {
                        common.retain_mut(|(card, delay, includes_layer)| {
                            if let Some((_, other, layer)) =
                                obligations.iter().find(|(c, _, _)| c == card)
                            {
                                *delay = (*delay).max(*other);
                                *includes_layer |= layer;
                                true
                            } else {
                                false
                            }
                        });
                    }
                }
                for (card, delay, includes_layer) in common {
                    if !includes_layer {
                        continue;
                    }
                    let identity = self.identity(card);
                    let holder = self.state.holder(card);
                    let note = &mut self.notes[card.index()];
                    note.promised = Some(record.turn);
                    note.delayed = note.delayed.max(delay);
                    note.play = note.delayed == 0;
                    if holder != Some(self.me)
                        && let Some(identity) = identity
                    {
                        note.belief.soft_clear();
                        note.belief.add_soft(identity);
                    }
                }
            }
        }
        self.interpretations.records = records;
        for seat in self.state.seats() {
            for &card in self.state.hand(seat) {
                let note = &mut self.notes[card.index()];
                note.update_attributes(playable, trash);
                if note.delayed > 0 {
                    note.play = false;
                }
            }
        }
        self.apply_sarcastic_ambiguity();
    }

    /// Rebuild where identities are known to be clued.
    pub(super) fn refresh_clued_cards(&mut self) {
        for identity in self.variant.all_identities().iter() {
            let played = self.play_state(identity) == PlayState::Trash;
            let info = &mut self.info[identity.index()];
            info.clued = played.then_some(Clued::Everyone);
            info.locked = None;
        }
        let records = &self.interpretations.records;
        for seat in self.seats_from_me() {
            // newest first: with duplicates, the oldest card ends up as the known location
            for &card in self.state.hand(seat).iter().rev() {
                let note = &self.notes[card.index()];
                let identity = self.identity(card);
                if note.clued
                    && seat != self.me
                    && let Some(identity) = identity
                {
                    self.info[identity.index()]
                        .clued
                        .get_or_insert(Clued::Holder(seat));
                }
                let finessed = records.iter().any(|record| {
                    record.resolved().is_some_and(|h| {
                        h.pending_steps()
                            .any(|s| s.card == card && s.kind == StepKind::Finesse)
                    })
                });
                if (note.clued || finessed)
                    && let Some(single) = note.belief.identity()
                    && (seat == self.me || identity == Some(single))
                {
                    let info = &mut self.info[single.index()];
                    info.clued = Some(Clued::Everyone);
                    info.locked = Some((seat, card));
                }
            }
        }
    }

    /// The unclued cards in front of a layered finesse target that must be played first.
    pub(super) fn layer_cards(
        &self,
        record: &ClueRecord,
        hypothesis: &Hypothesis,
        step: &Step,
    ) -> SmallVec<[CardId; 5]> {
        if step.layers == 0 {
            return SmallVec::new();
        }
        let Some(holder) = self.state.holder(step.card) else {
            return SmallVec::new();
        };
        let target = self
            .newest_first(holder)
            .find(|&(_, c)| c == step.card)
            .map(|(p, _)| p)
            .unwrap();
        let eligible: SmallVec<[CardId; 5]> = self
            .newest_first(holder)
            .filter(|&(pos, card)| {
                let note = &self.notes[card.index()];
                pos < target
                    && !note.clued
                    && !note.chop_moved
                    && note.drawn_at <= record.turn
                    && !hypothesis.steps.iter().any(|s| s.card == card)
                    && note.promised.is_none_or(|turn| turn == record.turn)
            })
            .map(|(_, card)| card)
            .collect();
        SmallVec::from_slice(&eligible[eligible.len().saturating_sub(step.layers as usize)..])
    }

    /// Whether a card this view sees may be `identity`: its identity if known, otherwise
    /// the hint of a kept seat view (a card the seat sees but our model cannot identify).
    pub fn may_be(&self, card: CardId, identity: Identity) -> bool {
        match self.identity(card) {
            Some(known) => known == identity,
            None => {
                self.state
                    .holder(card)
                    .is_some_and(|holder| holder != self.me)
                    && self.hints[card.index()].contains(identity)
            }
        }
    }

    pub(super) fn matches_visible_hand(&self, hypothesis: &Hypothesis) -> bool {
        hypothesis.pending_steps().all(|step| {
            self.state.holder(step.card).is_some_and(|holder| {
                if holder == self.me {
                    self.notes[step.card.index()]
                        .belief
                        .contains_hard(step.identity)
                } else {
                    self.may_be(step.card, step.identity)
                }
            })
        })
    }

    pub(super) fn true_hypothesis<'a>(&self, record: &'a ClueRecord) -> Option<&'a Hypothesis> {
        let holder = self.state.holder(record.focus)?;
        if holder == self.me {
            return None;
        }
        let identity = self.identity(record.focus)?;
        record
            .hypotheses
            .iter()
            .filter(|h| h.focus_identity == identity && self.matches_visible_hand(h))
            .min_by_key(|h| h.tier)
    }

    /// Whether the seats between giver and receiver can disambiguate this clue before its
    /// receiver acts.
    pub(super) fn resolves_before_receiver(&self, record: &ClueRecord, identity: Identity) -> bool {
        let Some(truth) = record
            .hypotheses
            .iter()
            .filter(|h| h.focus_identity == identity && self.matches_visible_hand(h))
            .min_by_key(|h| h.tier)
        else {
            return false;
        };
        let mut future = Interpretations {
            records: vec![record.clone()],
        };
        let n = self.num_seats();
        let mut seat = record.giver.after(1, n);
        while seat != record.receiver {
            if truth.expected_now().is_none_or(|step| step.holder != seat) {
                future.on_non_play(seat);
            }
            seat = seat.after(1, n);
        }
        future.records.first().is_some_and(|r| {
            r.active()
                .any(|h| h.focus_identity == identity && self.matches_visible_hand(h))
        })
    }

    /// This view as `seat` would see it, as far as this view can tell: `seat`'s own cards
    /// become unknown and our own cards are known where our notes identify them.
    pub fn perspective(&self, seat: Seat) -> View {
        if seat == self.me {
            return self.detached();
        }
        if let Some(seats) = &self.seats {
            // The kept view of that seat, built only from what it observed. It sees our
            // cards; we can only tell the ones our notes identify.
            let mut view = seats[seat.index()].clone();
            for &card in self.state.hand(self.me) {
                if let Some(identity) = self.notes[card.index()].belief.identity() {
                    view.known.set(card, identity);
                }
            }
            return view;
        }
        // Fallback for views without kept seat views (simulations and the seat views
        // themselves): our notes with the seat's own cards hidden. Deductions we made from
        // cards that seat cannot see remain (an approximation of depth-2 sight).
        let mut view = self.detached();
        let mut known = KnownCards::default();
        for card in self.state.dealt().iter() {
            if self.state.hand(seat).contains(&card) {
                continue;
            }
            if let Some(identity) = self.identity(card) {
                known.set(card, identity);
            } else if self.state.hand(self.me).contains(&card)
                && let Some(identity) = self.notes[card.index()].belief.identity()
            {
                known.set(card, identity);
            }
        }
        view.known = known;
        view.me = seat;
        for info in &mut view.info {
            if info.clued == Some(Clued::Holder(seat)) {
                info.clued = None;
            }
        }
        view
    }

    /// `perspective` plus a rebuild of the derived notes, for predicting `seat`'s decisions.
    pub fn decision_view(&self, seat: Seat) -> View {
        let mut view = self.perspective(seat);
        view.refresh_derived();
        view.refresh_clued_cards();
        view
    }

    pub(super) fn push_snapshot(&mut self) {
        self.history = Some(Arc::new(Snapshot {
            turn: self.turn + 1,
            notes: self.notes.clone(),
            previous: self.history.clone(),
        }));
    }

    /// The kept view of `seat`, fed only what that seat observed (root views only).
    pub fn seat_view(&self, seat: Seat) -> Option<&View> {
        self.seats.as_ref().map(|seats| &seats[seat.index()])
    }

    /// What this viewpoint believes about the unseen cards, for the world sampler: physical
    /// constraints from its sight, plus the convention inferences about its own cards as
    /// `Restrict` constraints (weights, so a wrong inference keeps its worlds possible).
    pub fn constraint_store(&self) -> ConstraintStore {
        let sight = Sight::of_seat(&self.state, self.me);
        let mut store =
            ConstraintStore::new(&self.variant, &self.state, &self.public, sight, &self.known);
        for &card in self.state.hand(self.me) {
            let belief = self.notes[card.index()].belief;
            // A contradiction (soft outside hard) is a convention mistake: trust only the
            // physical constraints for that card.
            if !belief.soft.is_empty()
                && belief.soft.is_subset(belief.hard)
                && belief.soft != belief.hard
            {
                store.convention.push(ConventionConstraint::Restrict {
                    card,
                    identities: belief.soft,
                });
            }
        }
        store
    }

    /// Cards of `seat` that are clued or promised (protected from discarding).
    pub fn protected(&self, seat: Seat) -> CardSet {
        self.state
            .hand(seat)
            .iter()
            .copied()
            .filter(|&c| self.notes[c.index()].touched())
            .collect()
    }
}
