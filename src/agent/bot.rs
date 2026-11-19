//! The H-Group bot: one convention view per seat, decisions by the blueprint policy.

use std::sync::Arc;

use super::Agent;
use crate::conventions::{Profile, View};
use crate::domain::Seat;
use crate::engine::{Action, Event, GameState};
use crate::policy::{self, Decision};
use crate::variants::Variant;

/// Bot configuration (the `[h_group]` section of docs/architecture.md §9).
#[derive(Clone, Debug)]
pub struct Config {
    pub max_level: u8,
    pub require_complete: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_level: 9,
            require_complete: false,
        }
    }
}

pub struct BotAgent {
    config: Config,
    view: Option<View>,
    last: Option<Decision>,
}

impl BotAgent {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            view: None,
            last: None,
        }
    }

    pub fn view(&self) -> &View {
        self.view.as_ref().expect("start() must be called first")
    }

    /// The last decision and why it was taken.
    pub fn last_decision(&self) -> Option<&Decision> {
        self.last.as_ref()
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
        self.view = Some(View::new(
            Arc::new(variant.clone()),
            Arc::new(profile),
            num_seats,
            seat,
        ));
        self.last = None;
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
        let action = decision.action;
        self.last = Some(decision);
        action
    }

    fn describe(&self) -> String {
        match &self.view {
            Some(view) => format!("{view:?}last decision: {:?}", self.last),
            None => String::new(),
        }
    }
}
