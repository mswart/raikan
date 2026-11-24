//! Clue intent: what our own candidate clue means, as the giver reads it.
//!
//! The intent is derived from the giver's view after [`View::give_clue`], with the same
//! interpreter that reads real clues: the rule that classified the clue, its focus and the
//! giver's true reading of it (focus identity and the connecting plays). It answers two
//! questions: does the clue have a purpose at all (it gets a new card clued, promised or
//! chop moved, or is a stall, a fix or a valuable tempo clue), and how to name it for
//! traces and logs.

use smallvec::SmallVec;

use super::records::{Step, StepKind};
use super::stall::{StallKind, StallSeverity};
use super::view::View;
use crate::domain::{CardId, CardSet, ClueId, Identity, Seat};

/// What a clue is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentKind {
    /// Gets the focus (and possibly connecting cards) played.
    Play,
    /// Protects the focus from being discarded.
    Save,
    /// Chop moves cards instead of touching them for play.
    ChopMove,
    /// Corrects a misread earlier clue.
    Fix,
    Stall(StallKind),
    /// A re-clue of an already clued card that is worth a clue (level 6).
    Tempo,
    /// No new card is clued, promised or chop moved, and nothing else justifies the clue.
    None,
}

/// The meaning of our own candidate clue.
#[derive(Clone, Debug, PartialEq)]
pub struct ClueIntent {
    pub receiver: Seat,
    pub clue: ClueId,
    /// The rule that classified the clue (e.g. `L1.play-clue`, `L1.save`, `L9.stall.five`).
    pub rule: &'static str,
    pub kind: IntentKind,
    pub focus: Option<CardId>,
    /// The focus card's true identity (the giver sees it).
    pub focus_identity: Option<Identity>,
    /// The connecting plays of the giver's true reading, before the focus (finesses,
    /// prompts, queued plays).
    pub connections: SmallVec<[Step; 5]>,
    /// Cards that become clued, promised or chop moved.
    pub new_cards: CardSet,
}

impl ClueIntent {
    /// Minimum clue value: a new card is clued, promised or chop moved, or the clue is a
    /// stall or a valuable tempo clue. A fix has value only through the errors it removes,
    /// which the caller judges.
    pub fn has_value(&self) -> bool {
        !self.new_cards.is_empty() || matches!(self.kind, IntentKind::Stall(_) | IntentKind::Tempo)
    }

    pub fn is_stall(&self) -> bool {
        matches!(self.kind, IntentKind::Stall(_))
    }

    /// One line naming the clue and what it means, e.g.
    /// `clue Alice blue: L1.play-clue, Alice slot 1 (#18) is b4 via finesse b2 Cathy slot 1`.
    pub fn describe(&self, view: &View, seat_name: impl Fn(Seat) -> String) -> String {
        let card_name = |card: CardId| {
            let holder = view.state.holder(card);
            let identity = view
                .identity(card)
                .map(|i| format!(" {}", view.variant.identity_name(i)))
                .unwrap_or_default();
            match holder.and_then(|seat| view.slot_of(seat, card).map(|slot| (seat, slot))) {
                Some((seat, slot)) => {
                    format!("{} slot {slot} (#{}{identity})", seat_name(seat), card.0)
                }
                None => format!("#{}{identity}", card.0),
            }
        };
        let mut out = format!(
            "clue {} {}: {} [{}]",
            seat_name(self.receiver),
            view.variant.clue_name(self.clue),
            self.rule,
            kind_name(self.kind)
        );
        if let Some(focus) = self.focus {
            out.push_str(&format!(", focus {}", card_name(focus)));
        }
        if self.kind == IntentKind::Play
            && let Some(identity) = self.focus_identity
        {
            out.push_str(&format!(
                " plays as {}",
                view.variant.identity_name(identity)
            ));
        }
        if !self.connections.is_empty() {
            let steps: Vec<String> = self
                .connections
                .iter()
                .map(|step| {
                    let kind = match step.kind {
                        StepKind::Queued => "queued",
                        StepKind::Prompt => "prompt",
                        StepKind::Finesse => "finesse",
                        StepKind::Focus => "focus",
                    };
                    let slot = view
                        .slot_of(step.holder, step.card)
                        .map_or_else(|| format!("#{}", step.card.0), |s| format!("slot {s}"));
                    format!(
                        "{kind} {} {} {slot}",
                        view.variant.identity_name(step.identity),
                        seat_name(step.holder)
                    )
                })
                .collect();
            out.push_str(" via ");
            out.push_str(&steps.join(", "));
        }
        let others: Vec<String> = self
            .new_cards
            .iter()
            .filter(|&card| {
                Some(card) != self.focus && !self.connections.iter().any(|s| s.card == card)
            })
            .map(card_name)
            .collect();
        if !others.is_empty() {
            out.push_str(&format!("; also {}", others.join(", ")));
        }
        out
    }
}

fn kind_name(kind: IntentKind) -> String {
    match kind {
        IntentKind::Play => "play".into(),
        IntentKind::Save => "save".into(),
        IntentKind::ChopMove => "chop move".into(),
        IntentKind::Fix => "fix".into(),
        IntentKind::Stall(kind) => format!("stall {kind:?}").to_lowercase(),
        IntentKind::Tempo => "tempo".into(),
        IntentKind::None => "no purpose".into(),
    }
}

impl View {
    /// Cards that become clued, promised or chop moved between this view and `after`
    /// (this view after our clue).
    pub fn newly_secured(&self, after: &View) -> CardSet {
        self.state
            .seats()
            .flat_map(|seat| self.state.hand(seat).iter().copied())
            .filter(|&card| {
                let before = self.note(card);
                let now = after.note(card);
                !before.clued
                    && before.promised.is_none()
                    && (now.clued
                        || now.promised.is_some()
                        || (now.chop_moved && !before.chop_moved))
            })
            .collect()
    }

    /// Our stall clue may be given now: the stall severity calls for one (a double discard,
    /// locked hand or 8 clues; the first early 5 stall; a burn the end-game allows).
    pub fn may_stall(&self) -> bool {
        let severity = self.stall_severity(self.me);
        severity >= StallSeverity::DoubleDiscard
            || (severity == StallSeverity::Early && !self.context.early_five_stalled)
            || (severity == StallSeverity::EndGame && self.burn_allowed())
    }

    /// The intent of our clue to `receiver`, given `after` (this view after the clue, as
    /// [`View::give_clue`] leaves it). `reduces_errors`: the clue removes convention errors
    /// of the position (a fix).
    pub fn clue_intent_after(
        &self,
        after: &View,
        receiver: Seat,
        clue: ClueId,
        reduces_errors: bool,
    ) -> ClueIntent {
        let rule = after.last_rule();
        let new_cards = self.newly_secured(after);
        let stall = self
            .clue_stall(receiver, clue)
            .filter(|kind| !(kind.is_general_save() && self.saves_critical_chop(receiver, clue)));
        let record = after
            .interpretations
            .records
            .iter()
            .find(|r| r.turn == after.turn && r.receiver == receiver);
        let touched = self.visible_touched(receiver, clue);
        let focus = record
            .map(|r| r.focus)
            .or_else(|| self.clue_focus(receiver, touched));
        let connections = record
            .and_then(|r| after.true_hypothesis(r))
            .map(|h| {
                h.steps
                    .iter()
                    .filter(|s| !s.done && s.kind != StepKind::Focus)
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        let kind = if rule == "L3.fix" || rule == "L3.stop" {
            IntentKind::Fix
        } else if let Some(kind) = stall {
            IntentKind::Stall(kind)
        } else if rule == "L6.valuable-tempo-clue" {
            IntentKind::Tempo
        } else if new_cards.is_empty() {
            if reduces_errors {
                IntentKind::Fix
            } else {
                IntentKind::None
            }
        } else if rule.starts_with("L4.") || rule == "L6.tempo-clue-chop-move" {
            IntentKind::ChopMove
        } else if rule == "L1.save" {
            IntentKind::Save
        } else {
            IntentKind::Play
        };
        ClueIntent {
            receiver,
            clue,
            rule,
            kind,
            focus,
            focus_identity: focus.and_then(|f| self.identity(f)),
            connections,
            new_cards,
        }
    }

    /// The intent of our clue to `receiver`; `None` if it touches nothing.
    pub fn clue_intent(&self, receiver: Seat, clue: ClueId) -> Option<ClueIntent> {
        let baseline = self.worth(0);
        let mut after = self.detached();
        let error = after.give_clue(receiver, clue)?;
        let reduces_errors = after.worth(error).errors < baseline.errors;
        Some(self.clue_intent_after(&after, receiver, clue, reduces_errors))
    }
}
