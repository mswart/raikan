//! The H-Group bot: one convention view per seat, decisions by the blueprint policy and,
//! when enabled, the root planner.

use std::sync::Arc;

use super::Agent;
use crate::conventions::{Profile, View};
use crate::domain::Seat;
use crate::engine::{Action, Event, GameState};
use crate::policy::{self, Decision};
use crate::search::{self, PlanResult, SearchConfig};
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
    trace: Option<Vec<String>>,
}

impl BotAgent {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            view: None,
            last: None,
            last_plan: None,
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
        if let Some(trace) = &mut self.trace {
            let entry = crate::trace::DecisionTrace {
                kind: "decision",
                turn: state.turn,
                seat: view.me.0,
                action: format!("{action:?}"),
                reason: format!("{:?}", decision.reason),
                profile_digest: format!("{:016x}", view.profile.digest),
                search: plan
                    .as_ref()
                    .map(|plan| serde_json::to_value(plan).expect("plan serialises")),
            };
            trace.push(serde_json::to_string(&entry).expect("trace serialises"));
        }
        self.last = Some(decision);
        self.last_plan = plan;
        action
    }

    fn describe(&self) -> String {
        match &self.view {
            Some(view) => format!("{view:?}last decision: {:?}", self.last),
            None => String::new(),
        }
    }
}
