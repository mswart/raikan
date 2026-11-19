//! Rule manifest and the convention profile compiled from it.
//!
//! Every behaviour is a rule with a stable id, the H-Group level that introduces it,
//! prerequisites and an honest coverage status. A [`Profile`] is compiled per game from the
//! maximum level and the variant; rule bodies ask the profile whether they are active and
//! never compare levels themselves.

use crate::variants::{CATALOGUE_REVISION, Variant};
use std::hash::{Hash, Hasher};

/// Revision of the convention implementation; part of the profile digest.
pub const CONVENTION_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    // level 1
    PlayClue,
    Save2,
    Save5,
    SaveCritical,
    GoodTouch,
    // level 2
    Prompt,
    Finesse,
    SelfFinesse,
    ReverseFinesse,
    WrongPrompt,
    StrikeRecovery,
    FiveStallEarly,
    // level 3
    OnesOrder,
    Fix,
    Stop,
    DuplicatedPlay,
    SarcasticDiscard,
    // level 5
    LayeredFinesse,
    // level 6
    TempoClue,
    // level 7
    EmergencyLookahead,
    // level 9
    StallFive,
    StallTempo,
    StallFillIn,
    StallLockedSave,
    StallEightClueSave,
    StallBurn,
}

/// What a rule implementation covers (see docs/architecture.md §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Interpretation, generation, follow-up and recovery.
    Complete,
    /// Recognised and generated, with known gaps.
    Partial(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Clue,
    Play,
    Discard,
    Situation,
}

#[derive(Clone, Copy, Debug)]
pub struct RuleMeta {
    pub rule: Rule,
    pub id: &'static str,
    pub level: u8,
    pub requires: &'static [Rule],
    pub doc: &'static str,
    pub trigger: Trigger,
    pub applies: fn(&Variant) -> bool,
    pub coverage: Coverage,
}

fn any_variant(_: &Variant) -> bool {
    true
}

const fn meta(
    rule: Rule,
    id: &'static str,
    level: u8,
    requires: &'static [Rule],
    doc: &'static str,
    trigger: Trigger,
    coverage: Coverage,
) -> RuleMeta {
    RuleMeta {
        rule,
        id,
        level,
        requires,
        doc,
        trigger,
        applies: any_variant,
        coverage,
    }
}

use Coverage::*;
use Rule::*;
use Trigger::*;

/// The manifest of every implemented rule.
pub const RULES: &[RuleMeta] = &[
    meta(
        PlayClue,
        "L1.play-clue",
        1,
        &[],
        "https://hanabi.github.io/learning-path#the-play-clue",
        Clue,
        Partial("direct and delayed play clues; prompts and finesses are separate rules"),
    ),
    meta(
        Save2,
        "L1.save-2",
        1,
        &[],
        "https://hanabi.github.io/learning-path#the-2-save",
        Clue,
        Complete,
    ),
    meta(
        Save5,
        "L1.save-5",
        1,
        &[],
        "https://hanabi.github.io/learning-path#the-5-save",
        Clue,
        Complete,
    ),
    meta(
        SaveCritical,
        "L1.save-critical",
        1,
        &[],
        "https://hanabi.github.io/learning-path#the-critical-save",
        Clue,
        Complete,
    ),
    meta(
        GoodTouch,
        "L1.good-touch",
        1,
        &[],
        "https://hanabi.github.io/learning-path#good-touch-principle",
        Situation,
        Complete,
    ),
    meta(
        Prompt,
        "L2.prompt",
        2,
        &[PlayClue],
        "https://hanabi.github.io/learning-path#the-prompt",
        Clue,
        Complete,
    ),
    meta(
        Finesse,
        "L2.finesse",
        2,
        &[PlayClue],
        "https://hanabi.github.io/learning-path#the-finesse",
        Clue,
        Complete,
    ),
    meta(
        SelfFinesse,
        "L2.self-finesse",
        2,
        &[Finesse],
        "https://hanabi.github.io/docs/beginner/self-finesse",
        Clue,
        Complete,
    ),
    meta(
        ReverseFinesse,
        "L2.reverse-finesse",
        2,
        &[Finesse],
        "https://hanabi.github.io/learning-path#the-reverse-finesse",
        Clue,
        Complete,
    ),
    meta(
        WrongPrompt,
        "L2.wrong-prompt",
        2,
        &[Prompt],
        "https://hanabi.github.io/docs/level-2#wrong-prompts",
        Play,
        Complete,
    ),
    meta(
        StrikeRecovery,
        "L2.strike-recovery",
        2,
        &[],
        "https://hanabi.github.io/docs/level-2",
        Play,
        Partial(
            "mistaken clues are withdrawn; later records are dropped rather than re-interpreted",
        ),
    ),
    meta(
        FiveStallEarly,
        "L2.five-stall",
        2,
        &[],
        "https://hanabi.github.io/learning-path#the-5-stall",
        Clue,
        Complete,
    ),
    meta(
        OnesOrder,
        "L3.ones-order",
        3,
        &[],
        "https://hanabi.github.io/learning-path#play-order-for-1s",
        Situation,
        Complete,
    ),
    meta(
        Fix,
        "L3.fix",
        3,
        &[],
        "https://hanabi.github.io/learning-path#fix-clues",
        Clue,
        Complete,
    ),
    meta(
        Stop,
        "L3.stop",
        3,
        &[Fix],
        "https://hanabi.github.io/learning-path#fix-clues",
        Clue,
        Partial("only stops the next queued play"),
    ),
    meta(
        DuplicatedPlay,
        "L3.duplicated-play",
        3,
        &[],
        "https://hanabi.github.io/learning-path#fix-clues",
        Clue,
        Complete,
    ),
    meta(
        SarcasticDiscard,
        "L3.sarcastic-discard",
        3,
        &[],
        "https://hanabi.github.io/learning-path#the-sarcastic-discard",
        Discard,
        Complete,
    ),
    meta(
        LayeredFinesse,
        "L5.layered-finesse",
        5,
        &[Finesse],
        "https://hanabi.github.io/learning-path#the-layered-finesse",
        Clue,
        Partial("hidden and clandestine finesses are not covered"),
    ),
    meta(
        TempoClue,
        "L6.tempo-clue",
        6,
        &[PlayClue],
        "https://hanabi.github.io/learning-path#the-tempo-clue",
        Clue,
        Partial("tempo clues only in stalling situations; no tempo clue chop move"),
    ),
    meta(
        EmergencyLookahead,
        "L7.emergency-lookahead",
        7,
        &[],
        "https://hanabi.github.io/learning-path#emergency-moves",
        Situation,
        Partial("tactical one-rotation forecast; no scream or shout discards"),
    ),
    meta(
        StallFive,
        "L9.stall.five",
        9,
        &[FiveStallEarly],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Complete,
    ),
    meta(
        StallTempo,
        "L9.stall.tempo",
        9,
        &[TempoClue],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Complete,
    ),
    meta(
        StallFillIn,
        "L9.stall.fill-in",
        9,
        &[],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Complete,
    ),
    meta(
        StallLockedSave,
        "L9.stall.locked-save",
        9,
        &[],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Complete,
    ),
    meta(
        StallEightClueSave,
        "L9.stall.eight-clue-save",
        9,
        &[],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Complete,
    ),
    meta(
        StallBurn,
        "L9.stall.burn",
        9,
        &[],
        "https://hanabi.github.io/docs/level-9",
        Clue,
        Partial("anxiety plays and the 5-stall finesse-position exception are missing"),
    ),
];

pub fn meta_of(rule: Rule) -> &'static RuleMeta {
    RULES
        .iter()
        .find(|m| m.rule == rule)
        .expect("every rule is in the manifest")
}

/// Why a profile could not be compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileError {
    /// `require_complete` was set and these rules of the requested levels are partial.
    Incomplete(Vec<&'static str>),
}

/// Immutable per-game convention profile.
#[derive(Clone, Debug)]
pub struct Profile {
    pub max_level: u8,
    pub variant: String,
    active: u64,
    pub digest: u64,
}

impl Profile {
    /// Compile the profile for a level cap and a variant.
    pub fn new(
        max_level: u8,
        variant: &Variant,
        require_complete: bool,
    ) -> Result<Self, ProfileError> {
        let mut active = 0u64;
        let mut gaps = Vec::new();
        // RULES is ordered so that prerequisites come first.
        for meta in RULES {
            let enabled = meta.level <= max_level
                && (meta.applies)(variant)
                && meta.requires.iter().all(|r| active & (1 << *r as u64) != 0);
            if enabled {
                active |= 1 << meta.rule as u64;
                if let Partial(_) = meta.coverage {
                    gaps.push(meta.id);
                }
            }
        }
        if require_complete && !gaps.is_empty() {
            return Err(ProfileError::Incomplete(gaps));
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (
            CONVENTION_REVISION,
            CATALOGUE_REVISION,
            &variant.name,
            active,
        )
            .hash(&mut hasher);
        Ok(Self {
            max_level,
            variant: variant.name.clone(),
            active,
            digest: hasher.finish(),
        })
    }

    /// The default profile: every implemented rule, partial coverage allowed.
    pub fn default_for(variant: &Variant) -> Self {
        Self::new(9, variant, false).expect("non-strict profiles always compile")
    }

    pub fn is_active(&self, rule: Rule) -> bool {
        self.active & (1 << rule as u64) != 0
    }

    pub fn active_rules(&self) -> impl Iterator<Item = &'static RuleMeta> + '_ {
        RULES.iter().filter(|m| self.is_active(m.rule))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_ordered_and_unique() {
        for (i, meta) in RULES.iter().enumerate() {
            assert!(
                RULES[..i]
                    .iter()
                    .all(|m| m.id != meta.id && m.rule != meta.rule)
            );
            for required in meta.requires {
                assert!(
                    RULES[..i].iter().any(|m| m.rule == *required),
                    "{} needs earlier {required:?}",
                    meta.id
                );
            }
        }
    }

    #[test]
    fn level_cap() {
        let variant = Variant::no_variant();
        let l1 = Profile::new(1, &variant, false).unwrap();
        assert!(l1.is_active(Rule::PlayClue));
        assert!(!l1.is_active(Rule::Finesse));
        assert!(!l1.is_active(Rule::Fix));
        let l2 = Profile::new(2, &variant, false).unwrap();
        assert!(l2.is_active(Rule::Finesse));
        assert!(!l2.is_active(Rule::SarcasticDiscard));
        let l9 = Profile::default_for(&variant);
        assert!(l9.is_active(Rule::StallBurn));
        assert_ne!(l2.digest, l9.digest);
        assert!(matches!(
            Profile::new(9, &variant, true),
            Err(ProfileError::Incomplete(_))
        ));
    }
}
