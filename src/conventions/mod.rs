//! Conventions: profile, rules, records, interpreter and notes.
//!
//! A [`View`] folds observed events into notes and clue records for one viewpoint. Which
//! behaviours are active is decided by the [`Profile`] compiled from the H-Group level cap;
//! rule bodies ask the profile and never compare levels.

mod belief;
pub mod catalogue;
mod chop_move;
mod clue;
mod emergency;
mod grade;
mod locator;
pub mod profile;
mod records;
mod rules;
mod stall;
mod view;

pub use belief::Belief;
pub use emergency::DiscardSignal;
pub use grade::{Worth, card_event};
pub use profile::{Profile, Rule};
pub use records::{ClueRecord, Hypothesis, Step, StepKind, Tier};
pub use rules::Fix;
pub use stall::{StallKind, StallSeverity};
pub use view::{CardNote, Clued, PlayState, TraceLog, TurnContext, View};
