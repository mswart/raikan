//! The H-Group bot: one convention view per seat, decisions by the blueprint policy and,
//! when enabled, the root planner.

use std::sync::Arc;

use super::Agent;
use crate::conventions::{ClueIntent, Profile, View};
use crate::domain::Seat;
use crate::engine::{Action, Event, GameState};
use crate::policy::{self, Decision};
use crate::search::{self, PlanReason, PlanResult, SearchConfig};
use crate::variants::Variant;

/// Bot configuration (the `[h_group]` section of docs/architecture.md §9).
#[derive(Clone, Debug)]
pub struct Config {
    pub max_level: u8,
    pub require_complete: bool,
    /// The `[search]` section; off by default (blueprint only).
    pub search: SearchConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_level: 10,
            require_complete: false,
            search: SearchConfig::default(),
        }
    }
}

pub struct BotAgent {
    config: Config,
    view: Option<View>,
    last: Option<Decision>,
    last_plan: Option<PlanResult>,
    /// What the last action meant, when it was a clue.
    last_intent: Option<ClueIntent>,
    last_action: Option<Action>,
    trace: Option<Vec<String>>,
}

impl BotAgent {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            view: None,
            last: None,
            last_plan: None,
            last_intent: None,
            last_action: None,
            trace: None,
        }
    }

    pub fn view(&self) -> &View {
        self.view.as_ref().expect("start() must be called first")
    }

    /// Collect interpretation and decision traces (JSON lines, see [`BotAgent::take_trace`]).
    pub fn enable_trace(&mut self) {
        self.trace = Some(Vec::new());
        if let Some(view) = &mut self.view {
            view.enable_trace();
        }
    }

    /// The trace lines collected so far, as JSON objects.
    pub fn take_trace(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(view) = &mut self.view {
            lines.extend(
                view.trace
                    .take()
                    .iter()
                    .map(|e| serde_json::to_string(e).expect("trace serialises")),
            );
        }
        if let Some(trace) = &mut self.trace {
            lines.append(trace);
        }
        lines
    }

    /// The last decision and why it was taken.
    pub fn last_decision(&self) -> Option<&Decision> {
        self.last.as_ref()
    }

    /// The root planner's result for the last decision, when search is on.
    pub fn last_plan(&self) -> Option<&PlanResult> {
        self.last_plan.as_ref()
    }

    /// What the last action meant, when it was a clue.
    pub fn last_intent(&self) -> Option<&ClueIntent> {
        self.last_intent.as_ref()
    }

    fn action_text(&self, action: Action, seat_name: &dyn Fn(Seat) -> String) -> String {
        let view = self.view();
        let card_text = |card| {
            let slot = view
                .slot_of(view.me, card)
                .map_or_else(String::new, |s| format!("slot {s} "));
            format!("{slot}(#{})", card.0)
        };
        match action {
            Action::Play(card) => format!("play {}", card_text(card)),
            Action::Discard(card) => format!("discard {}", card_text(card)),
            Action::Clue { target, clue } => match &self.last_intent {
                Some(intent) if intent.receiver == target && intent.clue == clue => {
                    intent.describe(view, seat_name)
                }
                _ => format!(
                    "clue {} {}",
                    seat_name(target),
                    view.variant.clue_name(clue)
                ),
            },
        }
    }
}

impl Default for BotAgent {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Agent for BotAgent {
    fn start(&mut self, variant: &Variant, num_seats: u8, seat: Seat) {
        let profile = Profile::new(self.config.max_level, variant, self.config.require_complete)
            .unwrap_or_else(|err| panic!("cannot compile convention profile: {err:?}"));
        let mut view = View::with_seat_views(
            Arc::new(variant.clone()),
            Arc::new(profile),
            num_seats,
            seat,
        );
        if self.trace.is_some() {
            view.enable_trace();
        }
        self.view = Some(view);
        self.last = None;
        self.last_plan = None;
        self.last_intent = None;
        self.last_action = None;
    }

    fn observe(&mut self, event: &Event) {
        self.view
            .as_mut()
            .expect("start() must be called first")
            .observe(event);
    }

    fn choose_action(&mut self, state: &GameState) -> Action {
        let view = self.view.as_ref().expect("start() must be called first");
        debug_assert_eq!(
            view.state.turn, state.turn,
            "view must be in sync with the game"
        );
        let decision = policy::choose_action(view);
        let plan = self
            .config
            .search
            .enabled()
            .then(|| search::plan(view, &decision, &self.config.search));
        let action = plan.as_ref().map_or(decision.action, |plan| plan.action);
        let intent = match action {
            Action::Clue { target, clue } => view.clue_intent(target, clue),
            _ => None,
        };
        if let Some(trace) = &mut self.trace {
            let entry = crate::trace::DecisionTrace {
                kind: "decision",
                turn: state.turn,
                seat: view.me.0,
                action: format!("{action:?}"),
                reason: format!("{:?}", decision.reason),
                intent: intent
                    .as_ref()
                    .map(|i| i.describe(view, |s: Seat| format!("seat {}", s.0))),
                profile_digest: format!("{:016x}", view.profile.digest),
                search: plan
                    .as_ref()
                    .map(|plan| serde_json::to_value(plan).expect("plan serialises")),
            };
            trace.push(serde_json::to_string(&entry).expect("trace serialises"));
        }
        self.last = Some(decision);
        self.last_plan = plan;
        self.last_intent = intent;
        self.last_action = Some(action);
        action
    }

    fn overrode_blueprint(&self) -> bool {
        self.last_plan
            .as_ref()
            .is_some_and(|plan| plan.reason == PlanReason::Searched)
    }

    fn summary(&self, names: &[String]) -> String {
        let (Some(action), Some(decision)) = (self.last_action, &self.last) else {
            return String::new();
        };
        let seat_name = |seat: Seat| {
            names
                .get(seat.index())
                .cloned()
                .unwrap_or_else(|| format!("seat {}", seat.0))
        };
        let mut out = format!(
            "{} [blueprint {:?}",
            self.action_text(action, &seat_name),
            decision.reason
        );
        if let Some(plan) = &self.last_plan {
            out.push_str(&format!(", plan {:?}", plan.reason));
            if plan.reason == PlanReason::Searched {
                out.push_str(&format!(
                    " over {}",
                    self.action_text(decision.action, &seat_name)
                ));
                if let Some((gain, stderr)) = plan.paired_difference {
                    out.push_str(&format!(" by {gain:+.2} ± {stderr:.2}"));
                }
            }
            if plan.to_end {
                out.push_str(", to the end");
            }
            let mut top: Vec<_> = plan.candidates.iter().filter(|c| c.visits > 0).collect();
            top.sort_by(|a, b| b.mean.total_cmp(&a.mean));
            let top: Vec<String> = top
                .iter()
                .take(3)
                .map(|c| {
                    let text = match c.action {
                        Action::Clue { target, clue } => format!(
                            "clue {} {}",
                            seat_name(target),
                            self.view().variant.clue_name(clue)
                        ),
                        other => self.action_text(other, &seat_name),
                    };
                    format!("{text} {:.2}", c.mean)
                })
                .collect();
            if !top.is_empty() {
                out.push_str(&format!("; top: {}", top.join(", ")));
            }
        }
        out.push(']');
        out
    }

    fn describe(&self) -> String {
        match &self.view {
            Some(view) => format!("{view:?}last decision: {:?}", self.last),
            None => String::new(),
        }
    }
}
