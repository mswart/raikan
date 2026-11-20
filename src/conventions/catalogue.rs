//! Every convention H-Group levels 1–20 require, independent of implementation.
//!
//! Section names and levels are taken from the level pages of https://hanabi.github.io
//! (read 2026-11-19). Each entry names the implemented rules that cover it, if any, and an
//! honest coverage status. The profile uses this list to report what a requested level
//! still misses; [`support_matrix`] renders it.

use super::profile::{Coverage, Rule};

/// What kind of section an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A move or convention with its own meaning.
    Move,
    /// A general principle or definition that shapes other conventions.
    Principle,
    /// A named mistake and how to recover from it.
    Mistake,
}

#[derive(Clone, Copy, Debug)]
pub struct Required {
    pub id: &'static str,
    pub level: u8,
    pub name: &'static str,
    /// Section anchor on `https://hanabi.github.io/level-<level>`, or a full URL.
    pub anchor: &'static str,
    pub kind: Kind,
    pub rules: &'static [Rule],
    pub coverage: Coverage,
}

impl Required {
    /// Variant-specific conventions live on other pages; their anchor is a full URL.
    pub fn url(&self) -> String {
        if self.anchor.starts_with("https://") {
            self.anchor.to_string()
        } else {
            format!(
                "https://hanabi.github.io/level-{}#{}",
                self.level, self.anchor
            )
        }
    }
}

use Coverage::{Complete, Missing, Partial};
use Kind::{Mistake, Move, Principle};
use Rule::*;

macro_rules! required {
    ($( $level:literal $id:literal $kind:ident $name:literal $anchor:literal [$($rule:ident),*] $coverage:expr; )*) => {
        &[$( Required {
            id: $id,
            level: $level,
            name: $name,
            anchor: $anchor,
            kind: $kind,
            rules: &[$($rule),*],
            coverage: $coverage,
        }, )*]
    };
}

pub const REQUIRED: &[Required] = required! {
    1 "L1.chop" Principle "The Chop" "the-chop" [PlayClue] Complete;
    1 "L1.playable" Principle "The Definition of Playable" "the-definition-of-playable" [PlayClue] Complete;
    1 "L1.trash" Principle "The Definition of Trash" "the-definition-of-trash" [GoodTouch] Complete;
    1 "L1.save-clues" Principle "Save Clues" "save-clues" [Save2, Save5, SaveCritical] Complete;
    1 "L1.focus" Principle "Clue Focus" "clue-focus" [PlayClue] Complete;
    1 "L1.early-game" Move "The Early Game" "the-early-game" [FiveStallEarly] Complete;
    1 "L1.save-5" Move "The 5 Save" "the-5-save" [Save5] Complete;
    1 "L1.save-2" Move "The 2 Save" "the-2-save" [Save2] Complete;
    1 "L1.visible-rule" Move "The Visible Rule" "the-visible-rule" [Save2] Complete;
    1 "L1.double-chop-2s" Move "Double Chop 2's" "double-chop-2s" [Save2] Complete;
    1 "L1.prompt" Move "The Prompt" "the-prompt" [Prompt] Complete;
    1 "L1.finesse" Move "The Finesse" "the-finesse" [Finesse] Complete;
    1 "L1.finesse-position" Principle "Finesse Position" "finesse-position" [Finesse] Complete;
    1 "L1.finessed-cards" Principle "Finessed Cards" "finessed-cards" [Finesse] Complete;
    1 "L1.prompts-over-finesses" Principle "Prompts > Finesses" "prompts--finesses" [Prompt] Complete;

    2 "L2.five-stall" Move "The 5 Stall" "the-5-stall-cluing-off-chop-5s" [FiveStallEarly] Complete;
    2 "L2.multiple-prompts" Move "The Double/Triple/Quadruple Prompt" "the-double-prompt--triple-prompt--quadruple-prompt" [Prompt] Complete;
    2 "L2.multiple-finesses" Move "The Double/Triple/Quadruple Finesse" "the-double-finesse--triple-finesse--quadruple-finesse" [Finesse] Complete;
    2 "L2.prompt-finesse" Move "The Prompt + Finesse" "the-prompt--finesse" [Prompt, Finesse] Complete;
    2 "L2.reverse-finesse" Move "The Reverse Finesse" "the-reverse-finesse" [ReverseFinesse] Complete;
    2 "L2.self-finesse" Move "The Self-Finesse" "the-self-finesse" [SelfFinesse] Complete;
    2 "L2.trash" Principle "Trash" "trash" [GoodTouch] Complete;
    2 "L2.one-away" Principle "One-Away-From-Playable Cards" "one-away-from-playable-cards" [PlayClue] Complete;
    2 "L2.stomping" Mistake "Stomping on a Finesse" "stomping-on-a-finesse" [Finesse] Complete;
    2 "L2.wrong-prompt" Mistake "The Wrong Prompt" "the-wrong-prompt" [WrongPrompt, StrikeRecovery] Complete;

    3 "L3.multiple-1s" Move "Playing Multiple 1's" "playing-multiple-1s" [OnesOrder] Complete;
    3 "L3.fix" Move "The Fix Clue" "the-fix-clue" [Fix] Complete;
    3 "L3.fix-multiple" Move "The Fix Clue (That Touches Multiple Cards)" "the-fix-clue-that-touches-multiple-cards" [Fix] Complete;
    3 "L3.fix-no-information" Move "The Fix Clue (That Gives No Additional Information)" "the-fix-clue-that-gives-no-additional-information" [Stop] Complete;
    3 "L3.sarcastic-discard" Move "The Sarcastic Discard" "the-sarcastic-discard-sd" [SarcasticDiscard, DuplicatedPlay] Complete;

    4 "L4.chop-moves" Principle "Chop Moves" "chop-moves" [ChopMove] Complete;
    4 "L4.trash-chop-move" Move "The Trash Chop Move" "the-trash-chop-move-tcm" [TrashChopMove] Complete;
    4 "L4.five-chop-move" Move "The 5's Chop Move" "the-5s-chop-move-5cm" [FiveChopMove] Complete;
    4 "L4.order-chop-move" Move "The Order Chop Move" "the-order-chop-move-ocm" [OrderChopMove] Complete;

    5 "L5.multi-color-prompt" Move "Prompts in Multi-Color Variants" "prompts-in-multi-color-variants" [MultiColorPrompt] Complete;
    5 "L5.free-choice-finesse" Move "The Free Choice Finesse (rainbow)" "https://hanabi.github.io/variant-specific/rainbow#the-free-choice-finesse" [FreeChoiceFinesse] Partial("receivers keep both readings; never given deliberately");
    5 "L5.hidden-finesse" Move "The Hidden Finesse" "the-hidden-finesse" [LayeredFinesse] Complete;
    5 "L5.layered-finesse" Move "The Layered Finesse" "the-layered-finesse" [LayeredFinesse] Complete;
    5 "L5.clandestine-finesse" Move "The Clandestine Finesse" "the-clandestine-finesse" [LayeredFinesse] Complete;
    5 "L5.queued-finesse" Move "The Queued Finesse" "the-queued-finesse" [Finesse] Complete;
    5 "L5.ambiguous-finesse" Move "The Ambiguous Finesse" "the-ambiguous-finesse" [Finesse] Complete;
    5 "L5.layered-finesse-dupe" Move "The Layered Finesse Dupe Clue" "the-layered-finesse-dupe-clue" [LayeredFinesseDupe] Complete;
    5 "L5.urgency" Principle "Urgency Principle" "urgency-principle-playing-into-finesses-as-soon-as-possible" [Urgency] Complete;
    5 "L5.schrodingers-cat" Principle "Schrödinger's Cat Principle" "schrödingers-cat-principle" [Prompt, Finesse] Complete;

    6 "L6.tempo-clue" Move "The Tempo Clue" "the-tempo-clue" [PlayClue, TempoClue] Complete;
    6 "L6.valuable-tempo-clue" Move "The Valuable Tempo Clue" "the-valuable-tempo-clue" [ValuableTempoClue] Complete;
    6 "L6.tempo-clue-stall" Move "The Tempo Clue Stall" "the-tempo-clue-stall-a-non-valuable-tempo-clue" [StallTempo] Complete;
    6 "L6.tempo-clue-chop-move" Move "The Tempo Clue Chop Move" "the-tempo-clue-chop-move-tccm" [TempoClueChopMove] Complete;

    7 "L7.scream-discard" Move "The Scream Discard Chop Move" "the-scream-discard-chop-move-sdcm" [ScreamDiscard] Complete;
    7 "L7.scream-discard-known-trash" Move "The Scream Discard Chop Move (With Known-Trash)" "the-scream-discard-chop-move-with-known-trash" [ScreamDiscard] Complete;
    7 "L7.shout-discard" Move "The Shout Discard Chop Move" "the-shout-discard-chop-move" [ShoutDiscard] Complete;
    7 "L7.generation-discard" Move "The Generation Discard" "the-generation-discard" [GenerationDiscard, EmergencyLookahead] Complete;
    7 "L7.blind-play-chop-moved" Principle "Blind-Playing Chop Moved Cards" "blind-playing-chop-moved-cards" [BlindPlayChopMoved] Complete;

    8 "L8.end-game" Principle "The End-Game" "the-end-game" [EndGame] Complete;
    8 "L8.no-chop-moves" Move "No Chop Moves in the End-Game" "no-chop-moves-in-the-end-game" [EndGame, ChopMove] Complete;
    8 "L8.positional-discard" Move "The Positional Discard" "the-positional-discard-indicating-a-play-with-a-discard" [PositionalDiscard] Complete;
    8 "L8.positional-misplay" Move "The Positional Misplay" "the-positional-misplay-indicating-a-play-with-a-misplay" [PositionalMisplay] Complete;
    8 "L8.double-positional-misplay" Move "The Double Positional Misplay" "the-double-positional-misplay-indicating-two-plays-with-a-misplay" [PositionalMisplay] Complete;
    8 "L8.distribution-clue" Move "The Distribution Clue" "the-distribution-clue" [DistributionClue] Complete;
    8 "L8.team-distribution" Principle "Team Distribution Principle" "team-distribution-principle" [DistributionClue, Burning] Complete;
    8 "L8.pace-plus-one" Principle "The Pace +1 Rule" "the-pace-1-rule" [Burning] Complete;
    8 "L8.burning" Move "Burning (End-Game Stalling)" "burning-end-game-stalling" [Burning] Complete;

    9 "L9.stalling-situations" Principle "Stalling Situations" "stalling-situations" [StallFive, StallTempo, StallFillIn, StallLockedSave, StallEightClueSave, StallBurn] Complete;
    9 "L9.stall-table" Principle "Allowable Stall Clues (Stall Table)" "allowable-stall-clues-stall-table" [StallFive, StallTempo, StallFillIn, StallLockedSave, StallEightClueSave, StallBurn] Complete;
    9 "L9.early-game" Move "The Early Game (Severity 1)" "the-early-game-severity-1-stalling" [FiveStallEarly] Complete;
    9 "L9.permission-to-discard" Move "Permission to Discard" "permission-to-discard-ptd" [PermissionToDiscard] Complete;
    9 "L9.double-discard-avoidance" Move "Double Discard Avoidance (Severity 2)" "double-discard-situations--double-discard-avoidance-dda-severity-2-stalling" [StallTempo, StallFillIn, StallBurn] Complete;
    9 "L9.locked-hands" Move "Locked Hands (Severity 3)" "locked-hands-severity-3-stalling" [StallLockedSave] Complete;
    9 "L9.eight-clues" Move "Clues Given While at 8 Clues (Severity 4)" "clues-given-while-at-8-clues-severity-4-stalling" [StallEightClueSave] Complete;
    9 "L9.five-stall" Move "The 5 Stall (Intermediate)" "the-5-stall-intermediate-section" [StallFive] Complete;
    9 "L9.five-stall-fpe" Move "The 5 Stall Finesse Position Exception" "the-5-stall-finesse-position-exception-fpe" [FiveStallFinesseException] Complete;
    9 "L9.fill-in" Move "The Fill-In Clue" "the-fill-in-clue" [StallFillIn] Complete;
    9 "L9.locked-hand-save" Move "The Locked Hand Save" "the-locked-hand-save-lhs" [StallLockedSave] Complete;
    9 "L9.anxiety-play" Move "The Anxiety Play" "the-anxiety-play-forcing-a-locked-player-to-play" [AnxietyPlay] Complete;
    9 "L9.eight-clue-save" Move "The 8 Clue Save" "the-8-clue-save-8cs" [StallEightClueSave] Complete;

    10 "L10.gentlemans-discard" Move "The Gentleman's Discard" "the-gentlemans-discard-gd" [] Missing;
    10 "L10.layered-gentlemans-discard" Move "The Layered Gentleman's Discard" "the-layered-gentlemans-discard" [] Missing;
    10 "L10.baton-discard" Move "The Baton Discard" "the-baton-discard-bd" [] Missing;
    10 "L10.sarcastic-finesse" Move "The Sarcastic Finesse" "the-sarcastic-finesse" [] Missing;
    10 "L10.certain-finesse" Move "The Certain Finesse & The Certain Discard" "the-certain-finesse--the-certain-discard" [] Missing;
    10 "L10.composition-finesse" Move "The Composition Finesse" "the-composition-finesse" [] Missing;
    10 "L10.double-gentlemans-discard" Mistake "The Double Gentleman's Discard (Illegal)" "the-double-gentlemans-discard-illegal" [] Missing;

    11 "L11.bluff" Move "The Bluff" "the-bluff" [] Missing;
    11 "L11.bluff-seat" Principle "The Bluff Seat" "the-bluff-seat" [] Missing;
    11 "L11.lie-principle" Principle "Lie Principle" "lie-principle" [] Missing;
    11 "L11.self-bluff" Move "The Self-Bluff" "the-self-bluff" [] Missing;
    11 "L11.bluff-through-clued" Move "Bluffs Through Already-Clued Cards" "bluffs-through-already-clued-cards" [] Missing;
    11 "L11.bobs-truth" Principle "Bob's Truth Principle" "bobs-truth-principle-part-1" [] Missing;
    11 "L11.cathys-connecting" Principle "Cathy's Connecting Principle" "cathys-connecting-principle-part-2" [] Missing;
    11 "L11.guide" Principle "Guide Principle" "guide-principle" [] Missing;
    11 "L11.occams-razor" Principle "Clue Interpretation & Occam's Razor" "clue-interpretation--occams-razor" [] Partial("Occam tiers for prompts and finesses; no bluffs");
    11 "L11.pang-of-guilt" Mistake "The Pang of Guilt" "the-pang-of-guilt" [] Missing;
    11 "L11.layered-vs-bluff" Mistake "Mistaking a Layered Finesse for a Bluff" "mistaking-a-layered-finesse-for-a-bluff" [] Missing;
    11 "L11.bluff-prompts" Mistake "Bluff Prompts / Prompt Bluffs (Illegal)" "bluff-prompts--prompt-bluffs-illegal" [] Missing;
    11 "L11.queued-bluffs" Mistake "Queued Bluffs (Illegal)" "queued-bluffs-illegal" [] Missing;

    12 "L12.asymmetric-information" Principle "Assuming Asymmetric Information" "assuming-asymmetric-information" [] Missing;
    12 "L12.duplication-responsibility" Principle "Duplication Responsibility" "duplication-responsibility" [] Missing;
    12 "L12.selfish-clue" Move "The Selfish Clue" "the-selfish-clue" [] Missing;
    12 "L12.selfish-finesse" Move "The Selfish Finesse" "the-selfish-finesse-a-finesse-through-your-own-hand" [] Missing;
    12 "L12.ego-clue" Mistake "The Ego Clue" "the-ego-clue-mistake" [] Missing;
    12 "L12.stale-ones" Move "The Stale 1's Clue" "the-stale-1s-clue" [] Missing;
    12 "L12.focus-inversion" Move "Focus Inversion" "focus-inversion" [] Missing;

    13 "L13.three-bluff" Move "The 3 Bluff" "the-3-bluff" [] Missing;
    13 "L13.critical-color-bluff" Move "The Critical Color Bluff" "the-critical-color-bluff-ccb" [] Missing;
    13 "L13.hard-bluff" Move "The Hard Bluff" "the-hard-bluff" [] Missing;
    13 "L13.hard-three-bluff" Move "The Hard 3 Bluff" "the-hard-3-bluff" [] Missing;
    13 "L13.known-bluff" Move "The Known Bluff" "the-known-bluff" [] Missing;
    13 "L13.good-touch-bluff" Move "The Good Touch Bluff" "the-good-touch-bluff" [] Missing;

    14 "L14.known-trash-order" Principle "Known-Trash Discard Order" "known-trash-discard-order" [] Missing;
    14 "L14.trash-order-chop-move" Move "The Trash Order Chop Move" "the-trash-order-chop-move-tocm" [] Missing;
    14 "L14.shout-discard-ocm" Move "The Shout Discard Order Chop Move" "the-shout-discard-order-chop-move" [] Missing;
    14 "L14.trash-push" Move "The Trash Push" "the-trash-push" [] Missing;
    14 "L14.trash-push-prompt-finesse" Move "The Trash Push Prompt & Finesse" "the-trash-push-prompt--the-trash-push-finesse" [] Missing;
    14 "L14.trash-finesse" Move "The Trash Finesse" "the-trash-finesse" [] Missing;
    14 "L14.reverse-trash-finesse" Move "The Reverse Trash Finesse" "the-reverse-trash-finesse" [] Missing;
    14 "L14.forced-gd-chop-move" Move "The Forced Gentleman's Discard Chop Move" "the-forced-gentlemans-discard-chop-move" [] Missing;
    14 "L14.trash-bluff" Move "The Trash Bluff" "the-trash-bluff" [] Missing;

    15 "L15.double-bluff" Move "The Double Bluff" "the-double-bluff" [] Missing;
    15 "L15.triple-bluff" Mistake "The Triple Bluff (Illegal)" "the-triple-bluff-illegal" [] Missing;
    15 "L15.hard-double-bluff" Move "The Hard Double Bluff" "the-hard-double-bluff" [] Missing;
    15 "L15.pestilent-double-bluff" Move "The Pestilent Double Bluff" "the-pestilent-double-bluff-pdb" [] Missing;
    15 "L15.deferring-bluff" Principle "Deferring a Bluff" "deferring-a-bluff" [] Missing;
    15 "L15.deferring-double-bluff" Principle "Deferring a Double Bluff" "deferring-a-double-bluff" [] Missing;

    16 "L16.ejections" Principle "Ejections" "ejections" [] Missing;
    16 "L16.discharges" Principle "Discharges" "discharges" [] Missing;
    16 "L16.five-color-ejection" Move "The 5 Color Ejection" "the-5-color-ejection-5ce" [] Missing;
    16 "L16.unknown-trash-discharge-1" Move "The Unknown Trash Discharge (1-for-1)" "the-unknown-trash-discharge-1-for-1-form-utd" [] Missing;
    16 "L16.unknown-trash-discharge-2" Move "The Unknown Trash Discharge (2-for-1)" "the-unknown-trash-discharge-2-for-1-form-utd" [] Missing;
    16 "L16.unknown-dupe-discharge" Move "The Unknown Dupe Discharge" "the-unknown-dupe-discharge-udd" [] Missing;

    17 "L17.duplicitous-value-clue" Move "The Duplicitous Value Clue" "the-duplicitous-value-clue" [] Missing;
    17 "L17.duplicitous-blind-play" Move "The Duplicitous Blind-Play" "the-duplicitous-blind-play" [] Missing;
    17 "L17.duplicitous-tempo-clue" Move "The Duplicitous Tempo Clue" "the-duplicitous-tempo-clue" [] Missing;
    17 "L17.assisted-trash-chop-move" Move "The Assisted Trash Chop Move" "the-assisted-trash-chop-move" [] Missing;
    17 "L17.time-travel-direct" Move "The Time Travel Chop Move (Direct)" "the-time-travel-chop-move-direct-form" [] Missing;
    17 "L17.time-travel-blind-play" Move "The Time Travel Chop Move (Blind-Play)" "the-time-travel-chop-move-blind-play-form" [] Missing;

    18 "L18.elimination" Principle "Elimination & Elimination Notes" "elimination--elimination-notes" [] Missing;
    18 "L18.double-discard-elimination" Move "Double Discard Elimination" "double-discard-elimination" [] Missing;
    18 "L18.two-elimination" Move "2 Elimination" "2-elimination" [] Missing;
    18 "L18.elimination-blind-play" Move "The Elimination Blind-Play" "the-elimination-blind-play" [] Missing;
    18 "L18.elimination-play-clue" Move "The Elimination Play Clue" "the-elimination-play-clue" [] Missing;
    18 "L18.elimination-chop-focus" Principle "Interaction Between Elimination & Chop-Focus" "interaction-between-elimination--chop-focus" [] Missing;
    18 "L18.riding-deduction" Move "The Elimination Riding Deduction" "the-elimination-riding-deduction" [] Missing;
    18 "L18.riding-deduction-bluff" Move "The Riding Deduction Bluff" "the-riding-deduction-bluff" [] Missing;
    18 "L18.elimination-self-chop-move" Move "The Elimination Self-Chop Move" "the-elimination-self-chop-move" [] Missing;
    18 "L18.elimination-finesse" Move "The Elimination Finesse" "the-elimination-finesse" [] Missing;
    18 "L18.trash-touch-elimination" Move "Trash Touch Elimination" "trash-touch-elimination-tte" [] Missing;

    19 "L19.score-phases" Principle "The Low Score Phase and the Normal Score Phase" "the-low-score-phase-and-the-normal-score-phase" [] Missing;
    19 "L19.no-five-play-clues" Move "No Play Clues with a Number 5 Clue in the Low Score Phase" "no-play-clues-with-a-number-5-clue-in-the-low-score-phase" [] Missing;
    19 "L19.early-five-chop-move" Move "The Early 5's Chop Move" "the-early-5s-chop-move" [] Missing;
    19 "L19.five-pull" Move "The 5 Pull" "the-5-pull" [] Missing;
    19 "L19.five-pull-prompt-finesse" Move "The 5 Pull Prompt & Finesse" "the-5-pull-prompt--the-5-pull-finesse" [] Missing;
    19 "L19.five-pull-double-finesse" Move "The 5 Pull Double Finesse" "the-5-pull-double-finesse" [] Missing;
    19 "L19.five-pull-clandestine" Move "The 5 Pull Clandestine Finesse" "the-5-pull-clandestine-finesse" [] Missing;
    19 "L19.five-pull-promise" Move "The 5 Pull Promise" "the-5-pull-promise-a-play-clue-after-a-5-pull" [] Missing;
    19 "L19.five-pull-certain" Principle "Finesses While 5 Pulled are Certain Finesses" "finesses-while-5-pulled-are-certain-finesses" [] Missing;
    19 "L19.five-pull-skip" Move "The 5 Pull Skip" "the-5-pull-skip" [] Missing;
    19 "L19.five-number-ejection" Move "The 5 Number Ejection" "the-5-number-ejection-5ne" [] Missing;
    19 "L19.five-number-discharge" Move "The 5 Number Discharge" "the-5-number-discharge-5nd" [] Missing;
    19 "L19.five-number-ejection-skips" Move "The 5 Number Ejection Finesse Position Skips" "the-5-number-ejection-finesse-position-skips" [] Missing;
    19 "L19.two-saves-five-stalls" Principle "Interaction Between 2 Saves & 5 Stalls" "interaction-between-2-saves--5-stalls" [] Missing;

    20 "L20.occupied-play-clue" Move "The Occupied Play Clue & Occupied Finesse" "the-occupied-play-clue--the-occupied-finesse-opc" [] Missing;
    20 "L20.out-of-order-play-clue" Move "The Out-of-Order Play Clue" "the-out-of-order-play-clue-triple-o--ooo" [] Missing;
    20 "L20.out-of-order-finesse" Move "The Out-of-Order Finesse" "the-out-of-order-finesse" [] Missing;
    20 "L20.out-of-order-corollary" Principle "The Out-of-Order Corollary" "the-out-of-order-corollary" [] Missing;
    20 "L20.suboptimal" Move "The Suboptimal Prompt, Finesse & Bluff" "the-suboptimal-prompt--the-suboptimal-finesse--the-suboptimal-bluff" [] Missing;
    20 "L20.no-information-finesse" Move "The No-Information Finesse" "the-no-information-finesse" [] Missing;
    20 "L20.no-information-double-bluff" Move "The No-Information Double Bluff" "the-no-information-double-bluff-nidb" [] Missing;
};

/// The highest level the catalogue covers. A profile above it cannot be checked.
pub const CATALOGUED_LEVELS: u8 = 20;

/// Markdown table of every required convention with its coverage.
pub fn support_matrix() -> String {
    let mut out = String::from(
        "| Level | Convention | Kind | Coverage | Rules |\n| --- | --- | --- | --- | --- |\n",
    );
    for entry in REQUIRED {
        let coverage = match entry.coverage {
            Complete => "complete".to_string(),
            Partial(gap) => format!("partial: {gap}"),
            Missing => "missing".to_string(),
        };
        let rules: Vec<&str> = entry
            .rules
            .iter()
            .map(|r| super::profile::meta_of(*r).id)
            .collect();
        out.push_str(&format!(
            "| {} | [{}]({}) | {:?} | {} | {} |\n",
            entry.level,
            entry.name,
            entry.url(),
            entry.kind,
            coverage,
            rules.join(", ")
        ));
    }
    out
}
