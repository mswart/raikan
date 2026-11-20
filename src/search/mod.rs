//! Search (docs/architecture.md §7.2–7.4): compares the permitted candidates by simulated
//! outcome in worlds sampled from the bot's beliefs.
//!
//! The first planner expands only the root ([`root::plan`]): every candidate is simulated
//! on the same sampled worlds for one rotation of partner-model moves (or to the real end
//! once the deck is small) and scored by a named-term leaf value ([`value`]).

pub mod rollout;
pub mod root;
pub mod value;

pub use root::{CandidateStats, PlanReason, PlanResult, plan};
pub use value::Breakdown;

/// How much search the bot does (the `[search]` section of docs/architecture.md §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize)]
pub enum SearchMode {
    /// Blueprint only.
    #[default]
    Off,
    /// A small budget, for bulk self-play.
    Fast,
    Full,
}

impl std::str::FromStr for SearchMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "fast" => Ok(Self::Fast),
            "full" => Ok(Self::Full),
            other => Err(format!("unknown search mode {other} (off, fast, full)")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchConfig {
    pub mode: SearchMode,
    /// Simulations per decision (candidate × world evaluations).
    pub simulations: u32,
    /// Worlds sampled per decision; no candidate is evaluated on more.
    pub worlds: u32,
    /// In paired standard errors: the best candidate replaces the blueprint's choice only
    /// when it is better by more than this. 0 always takes the best estimate.
    pub blueprint_margin: f64,
    /// Fixed seed; otherwise the seed is derived from the public state and the turn.
    pub seed: Option<u64>,
    /// Weight of a world that violates a convention inference (`[teammates] trust_epsilon`).
    pub epsilon: f64,
}

impl SearchConfig {
    /// The defaults of a mode.
    pub fn new(mode: SearchMode) -> Self {
        let (simulations, worlds) = match mode {
            SearchMode::Off => (0, 0),
            SearchMode::Fast => (300, 48),
            SearchMode::Full => (2000, 256),
        };
        Self {
            mode,
            simulations,
            worlds,
            blueprint_margin: 1.0,
            seed: None,
            epsilon: 0.02,
        }
    }

    pub fn enabled(&self) -> bool {
        self.mode != SearchMode::Off && self.simulations > 0 && self.worlds > 0
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self::new(SearchMode::Off)
    }
}
