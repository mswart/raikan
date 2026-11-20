//! Clue reader: classifies a clue (fix, stall, re-touch, save, play) in precedence order and
//! records the readings of play clues.

use smallvec::SmallVec;

use super::locator::{ClueContext, FirstAction, PlayEvaluation};
use super::profile::Rule;
use super::records::{ClueRecord, Hypothesis, StepKind};
use super::rules::Fix;
use super::stall::StallKind;
use super::view::{Clued, View};
use crate::domain::{CardId, CardSet, ClueId, Identity, IdentitySet, Seat, SeatSet};
use crate::engine::Event;
use crate::variants::ClueValue;

impl View {
    /// Good touch for a newly clued card: it is none of the identities already clued
    /// elsewhere (except a resolved assignment to this very card).
    fn exclude_clued_elsewhere(&mut self, receiver: Seat, card: CardId) {
        for identity in self.variant.all_identities().iter() {
            let info = self.info[identity.index()];
            if info.clued.is_some()
                && info.clued != Some(Clued::Holder(receiver))
                && info.locked != Some((receiver, card))
            {
                self.note_mut(card).remove(identity, true);
            }
        }
    }

    /// Remove `identity` from the clued cards of every seat except `giver` and `receiver`.
    fn exclude_from_third_parties(&mut self, giver: Seat, receiver: Seat, identity: Identity) {
        for seat in self.state.seats() {
            if seat == giver || seat == receiver {
                continue;
            }
            for &card in self.state.hand(seat) {
                if self.notes[card.index()].clued {
                    self.notes[card.index()].remove(identity, true);
                }
            }
        }
    }

    /// Process a clue. Returns the number of convention errors it makes (only meaningful
    /// when judging our own candidate clues).
    pub(super) fn clued(
        &mut self,
        giver: Seat,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
        event: &Event,
    ) -> u8 {
        if touched.is_empty() {
            // A clue that touches nothing (Color Blind and similar variants) carries no
            // information and no convention meaning; it only passes the turn.
            self.push_snapshot();
            self.interpretations.on_non_play(giver);
            self.turn += 1;
            self.apply_public(event);
            self.refresh_derived();
            return 0;
        }
        self.last_rule = "L1.play-clue";
        let focus_before = self.clue_focus(receiver, touched);
        // For our own clues: how the receiver will read it, from its view before the clue.
        self.pending_receiver =
            (giver == self.me && receiver != self.me).then(|| Box::new(self.perspective(receiver)));
        let error = self.clued_inner(giver, receiver, clue, touched, event);
        self.pending_receiver = None;
        if self.trace.is_some() {
            let focus = self
                .interpretations
                .records
                .iter()
                .find(|r| r.turn == self.turn && r.receiver == receiver)
                .map(|r| r.focus)
                .or(focus_before)
                .map_or(0, |c| c.0);
            let readings = self
                .interpretations
                .records
                .iter()
                .filter(|r| r.turn == self.turn && r.receiver == receiver)
                .flat_map(|r| r.hypotheses.iter())
                .map(|h| crate::trace::ReadingTrace {
                    focus: self.variant.identity_name(h.focus_identity),
                    tier: format!("{:?}", h.tier),
                    steps: h
                        .steps
                        .iter()
                        .map(|s| {
                            format!(
                                "{}@#{} {:?}",
                                self.variant.identity_name(s.identity),
                                s.card.0,
                                s.kind
                            )
                        })
                        .collect(),
                })
                .collect();
            let entry = crate::trace::InterpretationTrace {
                kind: "interpretation",
                turn: self.turn,
                viewer: self.me.0,
                giver: giver.0,
                receiver: receiver.0,
                clue: self.variant.clue_name(clue),
                focus,
                rule: self.last_rule,
                readings,
                errors: error,
            };
            self.trace.push(entry);
        }
        error
    }

    #[allow(clippy::too_many_lines)]
    fn clued_inner(
        &mut self,
        giver: Seat,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
        event: &Event,
    ) -> u8 {
        self.push_snapshot();
        let touch = self.variant.touch(clue);
        let old_chop = self.chop(receiver);
        let newly_clued = self.newly_touched(receiver, touched);
        let fix = self.fix_kind(receiver, clue, touched);
        let mut focus = self
            .clue_focus(receiver, touched)
            .expect("clues touch at least one card");
        if self.is_rank_clue(clue, 1) && self.rule(Rule::OnesOrder) {
            focus = old_chop
                .filter(|c| newly_clued.contains(*c))
                .or_else(|| self.first_one(receiver, touched))
                .expect("nonempty rank-1 clue");
            if fix.is_none() && old_chop == Some(focus) && newly_clued.contains(focus) {
                self.note_mut(focus).one_chop_focus = true;
            }
        }
        let mut save_options = IdentitySet::EMPTY;
        if old_chop == Some(focus) && newly_clued.contains(focus) {
            for identity in self.note(focus).belief.soft.iter() {
                if touch.contains(identity) && self.can_save(giver, receiver, clue, identity) {
                    save_options.insert(identity);
                }
            }
        }
        let stall = if fix.is_some() {
            None
        } else {
            self.stall_kind(giver, receiver, clue, touched)
        };
        let tempo_focus = (stall == Some(StallKind::Tempo))
            .then(|| self.stall_focus(receiver, touched))
            .flatten();
        if stall == Some(StallKind::Five) && self.context.early_game {
            self.context.early_five_stalled = true;
        }
        self.context.last_loss = None;
        let skipped_finesse = giver == self.me
            && self.state.hand(self.me).iter().any(|&c| {
                let note = self.note(c);
                note.promised.is_some() && note.play && !note.clued
            });
        self.interpretations.on_non_play(giver);
        let touched_cards: SmallVec<[CardId; 6]> = touched.iter().collect();
        self.interpretations.on_clue(&touched_cards);
        self.turn += 1;
        self.apply_public(event);
        self.refresh_derived();

        let mut error = if skipped_finesse { 2 } else { 0 };
        let playable = self.playable_set();
        let trash = self.trash_set();
        let hand: SmallVec<[CardId; 6]> = self.newest_first(receiver).map(|(_, c)| c).collect();
        for &card in &hand {
            let identity = self.identity(card);
            let note = self.note_mut(card);
            let old_size = note.belief.size();
            let previous_first = note.belief.soft.min();
            note.apply_clue(touch, touched.contains(card));
            if matches!(fix, Some(Fix::Literal(conflicts)) if conflicts.contains(card)) {
                note.belief.reset_soft();
                note.base.reset_soft();
                note.fixed = true;
                note.locked = false;
                note.stopped = false;
                if old_size == 1 {
                    self.info[previous_first.expect("size was tested").index()].clued = None;
                }
            }
            let note = self.note_mut(card);
            if touched.contains(card) {
                note.clued = true;
            }
            if let Some(single) = note.belief.identity() {
                if note.clued {
                    if !note.play {
                        note.locked = true;
                    }
                    if receiver == self.me || identity == Some(single) {
                        let info = &mut self.info[single.index()];
                        info.clued = Some(Clued::Everyone);
                        info.locked = Some((receiver, card));
                    }
                }
                let note = self.note_mut(card);
                note.update_attributes(playable, trash);
                // Knowing an unclued card's identity does not promise it will be kept; good
                // touch only reserves protected cards.
                if note.clued {
                    for &other in &hand {
                        if other != card && self.notes[other.index()].clued {
                            self.notes[other.index()].remove(single, true);
                        }
                    }
                }
            }
        }
        // A finesse on a card that the clue now rules out moves to a touched card (prompt).
        for record in &mut self.interpretations.records {
            for hypothesis in &mut record.hypotheses {
                let targets: SmallVec<[CardId; 5]> =
                    hypothesis.pending_steps().map(|s| s.card).collect();
                for step in hypothesis
                    .steps
                    .iter_mut()
                    .filter(|s| !s.done && s.kind == StepKind::Finesse && s.holder == receiver)
                {
                    if self.state.hand(receiver).contains(&step.card)
                        && !self.notes[step.card.index()]
                            .belief
                            .contains_hard(step.identity)
                        && let Some(&card) = hand.iter().find(|&&c| {
                            touched.contains(c)
                                && !targets.contains(&c)
                                && self.notes[c.index()].belief.contains_hard(step.identity)
                        })
                    {
                        step.card = card;
                        step.kind = StepKind::Prompt;
                        step.layers = 0;
                    }
                }
            }
        }
        {
            let state = &self.state;
            let notes = &self.notes;
            self.interpretations.retain_consistent(|step| {
                state.holder(step.card).is_some()
                    && notes[step.card.index()].belief.contains_hard(step.identity)
            });
        }
        if let Some(fix) = fix {
            match fix {
                Fix::Literal(conflicts) => {
                    self.interpretations
                        .retain_consistent(|step| !conflicts.contains(step.card));
                }
                Fix::Stop(card) => {
                    let options = self.stop_options(receiver, card);
                    let note = self.note_mut(card);
                    note.stopped = true;
                    note.fixed = true;
                    note.base = options;
                    self.interpretations
                        .retain_consistent(|step| step.card != card);
                }
            }
            // A correction never promises ancillary cards as new plays. Keep their literal
            // information and apply only ordinary good touch.
            for card in newly_clued.iter() {
                self.exclude_clued_elsewhere(receiver, card);
            }
            self.refresh_derived();
            self.refresh_clued_cards();
            self.last_rule = match fix {
                Fix::Literal(_) => "L3.fix",
                Fix::Stop(_) => "L3.stop",
            };
            return error;
        }
        self.refresh_derived();
        if stall.is_some() {
            // Tempo promises an immediate play of the focus only. Other stalls convey literal
            // information and good touch without a new chain.
            if let Some(focus) = tempo_focus {
                self.note_mut(focus).base.soft_limit(playable);
            }
            for card in newly_clued.iter() {
                if !self.note(card).locked {
                    self.exclude_clued_elsewhere(receiver, card);
                }
                if receiver != self.me
                    && let Some(identity) = self.identity(card)
                {
                    self.exclude_from_third_parties(giver, receiver, identity);
                }
            }
            self.refresh_derived();
            self.refresh_clued_cards();
            self.last_rule = match stall {
                Some(StallKind::Five) if self.context.early_game => "L2.five-stall",
                Some(StallKind::Five) => "L9.stall.five",
                Some(StallKind::Tempo) => "L9.stall.tempo",
                Some(StallKind::FillIn) => "L9.stall.fill-in",
                Some(StallKind::LockedSave) => "L9.stall.locked-save",
                Some(StallKind::EightClueSave) => "L9.stall.eight-clue-save",
                _ => "L9.stall.burn",
            };
            return error;
        }
        if newly_clued.is_empty() {
            let note = self.note_mut(focus);
            if note.play && !note.locked && !note.fixed {
                // useless re-clue
                error += 1;
            }
            note.play = true;
            error += self.resolve_play_clue(giver, receiver, focus, touched, clue);
            self.refresh_derived();
            self.refresh_clued_cards();
            self.last_rule = "L1.play-clue";
            return error;
        }

        let top = self.variant.num_ranks;
        let rank_clue = matches!(self.variant.clue_value(clue), ClueValue::Rank(_));
        // The focus comes first, even when it is a re-touched card (as with 1s order).
        let order: SmallVec<[CardId; 6]> = std::iter::once(focus)
            .chain(
                self.newest_first(receiver)
                    .map(|(_, c)| c)
                    .filter(|&c| c != focus && newly_clued.contains(c)),
            )
            .collect();
        for card in order {
            let duplicate = (card == focus && save_options.is_empty() && rank_clue)
                .then(|| self.duplicated_play(receiver, card))
                .flatten();
            if !self.note(card).locked {
                self.exclude_clued_elsewhere(receiver, card);
            }
            let identity = self.identity(card);
            if card == focus {
                if let Some(duplicate) = duplicate {
                    let note = self.note_mut(card);
                    note.base.soft_clear();
                    note.base.add_soft(duplicate);
                    note.belief = note.base;
                    // Recover from duplicates, but do not reward giving them.
                    error += 1;
                    self.last_rule = "L3.duplicated-play";
                } else if self.note(card).belief.intersects(save_options)
                    && (receiver == self.me
                        || !(identity.is_some_and(|i| self.variant.rank_of(i) == top)
                            && !self.is_rank_clue(clue, top)))
                {
                    let note = self.note_mut(card);
                    for potential in note.belief.soft.iter() {
                        if !save_options.contains(potential) && !playable.contains(potential) {
                            note.remove(potential, true);
                        }
                    }
                    self.last_rule = "L1.save";
                } else {
                    self.last_rule = "L1.play-clue";
                    self.note_mut(card).play = true;
                    error += self.resolve_play_clue(giver, receiver, card, touched, clue);
                }
                if let Some(single) = self.note(card).belief.identity()
                    && (receiver == self.me || identity == Some(single))
                {
                    let info = &mut self.info[single.index()];
                    info.clued = Some(Clued::Everyone);
                    info.locked = Some((receiver, card));
                    if receiver == self.me {
                        let me = self.me;
                        for seat in self.state.seats() {
                            if seat == me || seat == giver {
                                continue;
                            }
                            for &other in self.state.hand(seat) {
                                if self.notes[other.index()].clued {
                                    self.notes[other.index()].remove(single, true);
                                }
                            }
                        }
                    }
                }
            }
            let playable = self.playable_set();
            let trash = self.trash_set();
            let note = self.note_mut(card);
            note.update_attributes(playable, trash);
            if card == focus && note.trash {
                error += 5;
            }
            if receiver != self.me
                && let Some(identity) = identity
            {
                if self.info[identity.index()].clued.is_none() {
                    self.info[identity.index()].clued = Some(Clued::Holder(receiver));
                } else {
                    // potentially a bad re-clue
                    let sure_trash = self
                        .note(card)
                        .belief
                        .soft
                        .iter()
                        .all(|alternative| self.info[alternative.index()].clued.is_some());
                    if !sure_trash {
                        error += 1;
                    }
                }
                self.exclude_from_third_parties(giver, receiver, identity);
            }
        }
        self.refresh_derived();
        self.refresh_clued_cards();
        error
    }

    /// How the Free Choice rule applies to a colour clue, as this view can tell. A view that
    /// sees the receiver's hand knows whether another colour would have touched exactly the
    /// same cards; the receiver cannot and keeps both readings.
    fn clue_contexts(
        &self,
        receiver: Seat,
        clue: ClueId,
        touched: CardSet,
    ) -> SmallVec<[ClueContext; 2]> {
        let plain = ClueContext {
            clue: Some(clue),
            matching: false,
        };
        if !self.rule(Rule::FreeChoiceFinesse)
            || !matches!(self.variant.clue_value(clue), ClueValue::Color(_))
        {
            return smallvec::smallvec![plain];
        }
        let hand = self.state.hand(receiver);
        if hand.iter().any(|&c| self.identity(c).is_none()) {
            return smallvec::smallvec![
                plain,
                ClueContext {
                    matching: true,
                    ..plain
                }
            ];
        }
        let free_choice = self.variant.clue_ids().any(|other| {
            other != clue
                && matches!(self.variant.clue_value(other), ClueValue::Color(_))
                && self.visible_touched(receiver, other) == touched
        });
        smallvec::smallvec![ClueContext {
            matching: free_choice,
            ..plain
        }]
    }

    /// Record the readings of a play clue on `focus`. Returns convention errors when we are
    /// the giver (the clue cannot be read correctly in time).
    pub(super) fn resolve_play_clue(
        &mut self,
        giver: Seat,
        receiver: Seat,
        focus: CardId,
        premarked: CardSet,
        clue: ClueId,
    ) -> u8 {
        let mut error = 0;
        let contexts = self.clue_contexts(receiver, clue, premarked);
        let me = self.me;
        if giver == me
            && let Some(clued) = self.identity(focus)
        {
            for &card in self.state.hand(me) {
                let note = self.note(card);
                if note.clued && note.belief.contains(clued) {
                    // potential double clue of a card we already hold
                    error += 1;
                }
            }
        }
        let receiver_check = if giver == me {
            self.pending_receiver.take()
        } else {
            None
        };
        // Observers can condition connecting cards on the visible focus. Use a temporary view
        // so these assumptions stay retractable with the record.
        let mut view = self.detached();
        // A kept seat view does not know our cards; it then reads like the receiver.
        let seen_focus = if receiver != me {
            self.identity(focus)
        } else {
            None
        };
        let observed = if let Some(identity) = seen_focus {
            PlayEvaluation::test(
                self,
                identity,
                giver,
                receiver,
                focus,
                premarked,
                FirstAction::SelfFinesse,
                SeatSet::EMPTY,
                contexts[0],
            )
            .map(|e| e.into_hypothesis(self))
        } else {
            None
        };
        if let Some(hypothesis) = &observed {
            for step in hypothesis.pending_steps().filter(|s| s.holder != receiver) {
                let playable = view.playable_set();
                let trash = view.trash_set();
                let note = view.note_mut(step.card);
                note.belief.soft_clear();
                note.belief.add_soft(step.identity);
                note.update_attributes(playable, trash);
                if step.kind != StepKind::Finesse {
                    view.note_mut(focus).belief.remove(step.identity, true);
                }
            }
        }
        let candidates = view.note(focus).belief.soft;
        let tiers: &[FirstAction] = if self.rule(Rule::SelfFinesse) {
            &[
                FirstAction::NonSelf,
                FirstAction::SelfPrompt,
                FirstAction::SelfFinesse,
            ]
        } else {
            &[FirstAction::NonSelf, FirstAction::SelfPrompt]
        };
        let reverse = self.rule(Rule::ReverseFinesse);
        let mut hypotheses: Vec<Hypothesis> = Vec::new();
        for &first_action in tiers {
            for identity in candidates.iter() {
                for &context in &contexts {
                    let mut exclusions: SmallVec<[SeatSet; 8]> =
                        smallvec::smallvec![SeatSet::EMPTY];
                    let mut index = 0;
                    while index < exclusions.len() {
                        let excluded = exclusions[index];
                        index += 1;
                        if let Some(evaluation) = PlayEvaluation::test(
                            &view,
                            identity,
                            giver,
                            receiver,
                            focus,
                            premarked,
                            first_action,
                            excluded,
                            context,
                        ) {
                            let hypothesis = evaluation.into_hypothesis(&view);
                            if reverse {
                                for step in hypothesis
                                    .pending_steps()
                                    .filter(|s| s.kind == StepKind::Finesse && s.holder != receiver)
                                {
                                    let mut next = excluded;
                                    next.insert(step.holder);
                                    if !exclusions.contains(&next) {
                                        exclusions.push(next);
                                    }
                                }
                            }
                            if !hypotheses.iter().any(|h| h.steps == hypothesis.steps) {
                                hypotheses.push(hypothesis);
                            }
                        }
                    }
                }
            }
        }
        if let Some(observed) = observed {
            if !hypotheses.iter().any(|h| h.steps == observed.steps) {
                hypotheses.push(observed);
            }
        } else if seen_focus.is_some() {
            error += 2;
        }
        if hypotheses.is_empty() {
            error += 2;
        }
        self.interpretations.add(ClueRecord {
            turn: self.turn,
            giver,
            receiver,
            focus,
            hypotheses,
        });
        self.refresh_derived();
        if let Some(mut receiver_view) = receiver_check
            && !self.note(focus).trash
        {
            // The receiver reads the whole clue with its own conventions.
            let event = Event::Clue {
                giver,
                target: receiver,
                clue,
                touched: premarked,
            };
            receiver_view.clued(giver, receiver, clue, premarked, &event);
            let identity = self.identity(focus);
            let ok = identity.is_some_and(|identity| {
                receiver_view.interpretations.records.iter().any(|record| {
                    record.turn == receiver_view.turn
                        && record.receiver == receiver
                        && receiver_view.resolves_before_receiver(record, identity)
                })
            });
            if !ok {
                error += 2;
            }
        }
        error
    }
}
