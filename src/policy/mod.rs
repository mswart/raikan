//! Action choice without search: candidates, legibility guard, blueprint and partner model.

pub mod blueprint;
pub mod guard;
pub mod partner;

pub use blueprint::{Decision, Reason, candidates, choose_action};
pub use guard::{permitted, permitted_candidates};
pub use partner::{partner_action, partner_decision};
