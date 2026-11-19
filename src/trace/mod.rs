//! Trace data model. Every layer emits its own payloads; this module only holds the shared
//! record types, so it sits beside the rules layer and may be used by any layer.
//!
//! Traces are written as one JSON object per line.

use serde::Serialize;

/// One connecting play of a reading, in text form (`r1@#4 finesse`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReadingTrace {
    pub focus: String,
    pub tier: String,
    pub steps: Vec<String>,
}

/// How one viewpoint read one observed clue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InterpretationTrace {
    pub kind: &'static str,
    pub turn: i16,
    pub viewer: u8,
    pub giver: u8,
    pub receiver: u8,
    pub clue: String,
    pub focus: u8,
    /// The rule whose reading was accepted (e.g. `L1.save-5`, `L9.stall.tempo`).
    pub rule: &'static str,
    /// Readings of a play clue still alive after the clue.
    pub readings: Vec<ReadingTrace>,
    /// Convention errors the clue makes from this viewpoint (only meaningful for the giver).
    pub errors: u8,
}

/// One decision of a seat and why it was taken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DecisionTrace {
    pub kind: &'static str,
    pub turn: u16,
    pub seat: u8,
    pub action: String,
    pub reason: String,
    pub profile_digest: String,
}
