// Legacy implementation (kept as the parity baseline until the port is complete).
pub mod dump_strategies;
pub mod game;

mod card_quantum;
pub use card_quantum::CardQuantum;
pub use card_quantum::Variant;
pub mod hyphenated;
mod position_set;
pub use position_set::PositionSet;

// Target architecture (docs/architecture.md), lowest layer first.
pub mod adapters;
pub mod agent;
pub mod conventions;
pub mod domain;
pub mod engine;
pub mod knowledge;
pub mod policy;
pub mod trace;
pub mod variants;
