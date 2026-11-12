//! Bounded cooperative action search.
//!
//! Retain the established policy unless it predicts an emergency, then maximize
//! over convention-compatible alternatives, averaging unknown own-card outcomes
//! by remaining copy counts. Continue through the other seats' predicted
//! plays and zero-token discards, stopping at a clue-giving choice or one rotation.
//! No future draw identities are invented, and simulated hands are not refilled.
//!
//! Candidate generation, transitions, rollout, and leaf evaluation are separate
//! extension points. A deeper search can branch at the stopping point. Teammates
//! are cooperative MAX nodes using their own information, not adversarial MIN
//! nodes. Robust minimax could instead take the minimum over plausible hidden
//! outcomes; expectimax would retain the weighted average used here. Deeper search
//! also needs consistent joint hand beliefs, draw outcomes, and the final-turn
//! counter, rather than treating these marginal beliefs as perfect information.

use super::stall::{StallKind, StallSeverity};
use super::{Line, LineScore};
use crate::{
    Variant,
    game::{Card, CardPlayState, Clue, GameStatus, Move},
};
use smallvec::SmallVec;

const LOST_POINT_COST: f64 = 25.0;
const STRIKE_COST: f64 = 50.0;
const STRIKEOUT_COST: f64 = 1000.0;

pub(super) fn choose_action(line: &Line, status: &GameStatus, debug: bool) -> Move {
    choose_action_with_pruning(line, status, debug, true)
}

fn choose_action_with_pruning(line: &Line, status: &GameStatus, debug: bool, prune: bool) -> Move {
    let mut position;
    let line = if line.context.clues == status.clues {
        line
    } else {
        position = line.clone();
        position.context.clues = status.clues;
        &position
    };
    let severity = line.stall_severity(0);
    let baseline = line.score(0);
    let mut normal_clue_available = false;
    let mut best_stall: Option<(StallKind, u8, Move, Evaluation)> = None;
    let mut best = None;
    let mut preferred: Option<(Move, Evaluation)> = None;
    let mut best_fix: Option<(Move, f64)> = None;
    for action in candidates(line, status.clues) {
        let stall = match action {
            Move::Clue(receiver, clue) => line.clue_stall(receiver, clue).filter(|kind| {
                !(kind.is_general_save() && line.saves_critical_chop(receiver, clue))
            }),
            _ => None,
        };
        let Some(evaluation) = evaluate_filtered(line, status, action, &baseline, |upper| {
            debug
                || !prune
                || !baseline.no_more_errors_than(upper)
                || stall.is_some()
                || best.is_none_or(|(_, value)| upper.utility() as f64 > value)
                || preferred.as_ref().is_none_or(|(previous, evaluation)| {
                    !matches!(previous, Move::Play(_)) && *upper > evaluation.priority
                })
                || (!normal_clue_available
                    && *upper > baseline
                    && upper.no_more_errors_than(&baseline))
        }) else {
            continue;
        };
        if debug {
            println!("considered {action:?}: {evaluation:?}");
        }
        let value = evaluation.value;
        if best.is_none_or(|(_, previous)| value > previous) {
            best = Some((action, value));
        }
        if let Move::Clue(receiver, clue) = action {
            if !baseline.no_more_errors_than(&evaluation.priority)
                && line.clue_fixes(receiver, clue)
                && evaluation.urgent == 0.0
                && best_fix.is_none_or(|(_, previous)| value > previous)
            {
                best_fix = Some((action, value));
            }
            if let Some(kind) = stall {
                if evaluation.urgent == 0.0 && evaluation.priority.no_more_errors_than(&baseline) {
                    let distance = if kind == StallKind::Five {
                        line.five_stall_distance(receiver)
                    } else {
                        0
                    };
                    if best_stall
                        .as_ref()
                        .is_none_or(|(old_kind, old_distance, _, old)| {
                            (kind.precedence(), distance) < (old_kind.precedence(), *old_distance)
                                || ((kind.precedence(), distance)
                                    == (old_kind.precedence(), *old_distance)
                                    && evaluation.value > old.value)
                        })
                    {
                        best_stall = Some((kind, distance, action, evaluation));
                    }
                }
                continue;
            }
            normal_clue_available |= evaluation.priority > baseline
                && evaluation.priority.no_more_errors_than(&baseline)
                && evaluation.urgent == 0.0;
        }
        // Preserve expected play order when it is safe. Skipping a promised play
        // can retract a teammate's connection, so only an emergency warrants it.
        if ((matches!(action, Move::Play(_)) && !baseline.has_errors())
            || (status.clues < 8 && Some(action) == line.sarcastic_discard()))
            && evaluation.danger == 0.0
        {
            return action;
        }
        if preferred
            .as_ref()
            .is_none_or(|(previous_action, previous)| {
                !matches!(previous_action, Move::Play(_)) && evaluation.priority > previous.priority
            })
        {
            preferred = Some((action, evaluation));
        }
    }
    if let Some((action, _)) = best_fix {
        return action;
    }
    if !normal_clue_available
        && (severity >= StallSeverity::DoubleDiscard
            || (severity == StallSeverity::Early && !line.context.early_five_stalled))
        && let Some((_, _, action, _)) = best_stall
    {
        return action;
    }
    // Unknown chop cards always carry some background risk. That alone should
    // not make the bot replace normal discards with weak clues and lock hands.
    // Use the search to intervene in predictable emergencies, including a known
    // protected-card loss or a downstream forced discard/misplay.
    if let Some((action, evaluation)) = preferred
        && evaluation.urgent == 0.0
    {
        return action;
    }
    best.map(|(action, _)| action).unwrap_or_else(|| {
        // Inconsistent beliefs must not cause an illegal discard at eight clues.
        if status.clues == 8 {
            Move::Play(0)
        } else {
            line.discard()
        }
    })
}

fn candidates(line: &Line, clues: u8) -> SmallVec<[Move; 42]> {
    let mut actions = SmallVec::new();
    // Keep the convention's play order and discard position. Arbitrarily choosing
    // another slot could itself communicate an unsupported convention.
    if let Some(action) = line.next_play()
        && (clues == 8 || line.sarcastic_discard().is_none())
    {
        actions.push(action);
    }
    if clues < 8 {
        actions.push(line.discard());
    }
    if clues > 0 {
        for player in 1..line.hands.num_players {
            for clue in (Variant {})
                .suits()
                .iter()
                .map(|s| Clue::Color(s.clue_color()))
                .chain((1..=5).map(Clue::Rank))
            {
                if line
                    .hands
                    .iter_hand(player)
                    .any(|(_, slot)| slot.card.affected(clue))
                {
                    actions.push(Move::Clue(player, clue));
                }
            }
        }
    }
    actions
}

#[derive(Debug)]
struct Evaluation {
    priority: LineScore,
    value: f64,
    danger: f64,
    urgent: f64,
}

#[cfg(test)]
fn evaluate(line: &Line, status: &GameStatus, action: Move) -> Option<Evaluation> {
    evaluate_filtered(line, status, action, &line.score(0), |_| true)
}

fn evaluate_filtered(
    line: &Line,
    status: &GameStatus,
    action: Move,
    baseline: &LineScore,
    accept: impl FnOnce(&LineScore) -> bool,
) -> Option<Evaluation> {
    match action {
        Move::Clue(receiver, clue) => {
            let mut needs_fix = false;
            let (mut forecast, priority) =
                Forecast::clue_filtered(line, status, receiver, clue, |after, upper| {
                    let gets_new_card = (0..line.hands.num_players).any(|player| {
                        line.hands.iter_hand(player).any(|(pos, before)| {
                            let after = after.hands.slot(player, pos);
                            !before.clued
                                && before.promised.is_none()
                                && (after.clued || after.promised.is_some())
                        })
                    });
                    // Minimum clue value: ordinary tempo clues must not replace a
                    // discard. Fixes and stalls have their own reason to spend a clue;
                    // a re-clue that gets a new finesse also meets the principle.
                    needs_fix = !gets_new_card && line.clue_stall(receiver, clue).is_none();
                    if needs_fix && baseline.no_more_errors_than(upper) {
                        return false;
                    }
                    accept(upper)
                })?;
            if needs_fix && baseline.no_more_errors_than(&priority) {
                return None;
            }
            let quality = priority.utility() as f64;
            forecast.rollout();
            let danger = forecast.danger();
            Some(Evaluation {
                priority,
                value: quality - danger,
                danger,
                urgent: danger,
            })
        }
        Move::Play(pos) | Move::Discard(pos) => {
            let outcomes = own_card_outcomes(line, pos);
            let total: u32 = outcomes.iter().map(|(_, weight)| *weight as u32).sum();
            if total == 0 {
                return None;
            }
            let mut value = 0.0;
            let mut danger = 0.0;
            let mut urgent = 0.0;
            let priority = baseline.clone();
            for (card, weight) in outcomes {
                let mut forecast = Forecast::own_card(line, status, action, card);
                let slot = line.hands.slot(0, pos);
                let background_risk = if matches!(action, Move::Discard(_))
                    && !slot.clued
                    && slot.promised.is_none()
                    && slot.quantum.size() > 1
                {
                    forecast.lost_points as f64 * LOST_POINT_COST
                } else {
                    0.0
                };
                // Keep the established information-value baseline. Hypothetical
                // discards must not earn credit by making conflicting beliefs
                // disappear; their concrete consequences are priced as danger.
                let quality = priority.utility() as f64
                    + if matches!(action, Move::Play(_)) && forecast.new_strikes == 0 {
                        10.0
                    } else {
                        0.0
                    };
                forecast.rollout();
                danger += forecast.danger() * weight as f64;
                urgent += (forecast.danger() - background_risk) * weight as f64;
                value += (quality - forecast.danger()) * weight as f64;
            }
            Some(Evaluation {
                priority,
                value: value / total as f64,
                danger: danger / total as f64,
                urgent: urgent / total as f64,
            })
        }
    }
}

fn own_card_outcomes(line: &Line, pos: u8) -> SmallVec<[(Card, u8); 25]> {
    let slot = line.hands.slot(0, pos);
    let options = |hard: bool| {
        line.card_states
            .iter()
            .filter_map(|(card, state)| {
                let possible = if hard {
                    slot.quantum.contains_hard(&card)
                } else {
                    slot.quantum.contains(&card)
                };
                let remaining = card
                    .suit
                    .card_count(card.rank)
                    .saturating_sub(state.tracked_count);
                (possible && remaining > 0).then_some((card, remaining))
            })
            .collect::<SmallVec<[(Card, u8); 25]>>()
    };
    let inferred = options(false);
    // Known duplicates can have an empty soft set. Literal clue information
    // still supplies possible identities for the discard simulation.
    if inferred.is_empty() {
        options(true)
    } else {
        inferred
    }
}

struct Forecast {
    views: Vec<Line>,
    // Card-action forecasts often stop at the next seat. Build later views
    // only if reached, replaying the same public events from this initial view.
    initial: Option<Line>,
    actions: SmallVec<[(u8, Move, Card, bool); 5]>,
    clues: u8,
    strikes: u8,
    new_strikes: u8,
    lost_points: u8,
}

impl Forecast {
    fn from_line(mut line: Line, clues: u8, strikes: u8) -> Self {
        line.context.clues = clues;
        let initial = Some(line.clone());
        let mut views = Vec::with_capacity(line.hands.num_players as usize);
        views.push(line);
        Self {
            views,
            initial,
            actions: SmallVec::new(),
            clues,
            strikes,
            new_strikes: 0,
            lost_points: 0,
        }
    }

    #[cfg(test)]
    fn clue(
        before: &Line,
        status: &GameStatus,
        receiver: u8,
        clue: Clue,
    ) -> Option<(Self, LineScore)> {
        Self::clue_filtered(before, status, receiver, clue, |_, _| true)
    }

    fn clue_filtered(
        before: &Line,
        status: &GameStatus,
        receiver: u8,
        clue: Clue,
        accept: impl FnOnce(&Line, &LineScore) -> bool,
    ) -> Option<(Self, LineScore)> {
        let mut after = before.clone();
        after.context.clues = status.clues;
        let (score, views) = after.clue_views(receiver as usize, clue, accept)?;
        Some((
            Self {
                views,
                initial: None,
                actions: SmallVec::new(),
                clues: status.clues - 1,
                strikes: status.num_strikes,
                new_strikes: 0,
                lost_points: 0,
            },
            score,
        ))
    }

    fn own_card(before: &Line, status: &GameStatus, action: Move, card: Card) -> Self {
        let mut after = before.clone();
        after.context.clues = status.clues;
        let mut clues = status.clues;
        let mut new_strikes = 0;
        match action {
            Move::Play(pos) => {
                let success = before.card_states.play_quantum.contains(&card);
                after.played(0, pos as usize, card, success);
                if success && card.rank == 5 {
                    clues = (clues + 1).min(8);
                }
                if !success {
                    new_strikes = 1;
                }
            }
            Move::Discard(pos) => {
                after.discarded(0, pos as usize, card);
                clues += 1;
            }
            Move::Clue(..) => unreachable!(),
        }
        let lost_points = newly_dead(before, &after);
        let mut forecast = Self::from_line(after, clues, status.num_strikes + new_strikes);
        forecast.new_strikes = new_strikes;
        forecast.lost_points = lost_points;
        forecast
    }

    fn rollout(&mut self) {
        let players = self.views[0].hands.num_players as usize;
        for player in 1..players {
            if self.strikes >= 3
                || self.views[0]
                    .card_states
                    .iter()
                    .all(|(_, state)| state.play == CardPlayState::Trash())
            {
                break;
            }
            if player == self.views.len() {
                let mut view = self
                    .initial
                    .as_ref()
                    .expect("lazy card forecast")
                    .decision_view(player as u8);
                for &(holder, action, card, successful) in &self.actions {
                    let relative = (players + holder as usize - player) % players;
                    match action {
                        Move::Play(pos) => view.played(relative, pos as usize, card, successful),
                        Move::Discard(pos) => view.discarded(relative, pos as usize, card),
                        Move::Clue(..) => unreachable!(),
                    }
                }
                self.views.push(view);
            }
            let action = if let Some(action) = self.views[player]
                .sarcastic_discard()
                .filter(|_| self.clues < 8)
            {
                action
            } else if let Some(action) = self.views[player].play() {
                action
            } else if self.clues == 0 {
                self.views[player].discard()
            } else {
                // This player can choose a clue. Do not pretend that a discard
                // is forced or invent a clue involving cards we cannot see.
                break;
            };
            self.apply_visible_card(player as u8, action);
        }
    }

    fn apply_visible_card(&mut self, player: u8, action: Move) {
        let pos = match action {
            Move::Play(pos) | Move::Discard(pos) => pos,
            Move::Clue(..) => unreachable!(),
        };
        let card = self.views[0].hands.slot(player, pos).card;
        let before = self.views[0].card_states.clone();
        let success = before.play_quantum.contains(&card);
        let players = self.views[0].hands.num_players as usize;
        for (observer, view) in self.views.iter_mut().enumerate() {
            // This bounded forecast never revisits a seat. Keep the root and
            // future decision-makers current; spent seats need no deductions.
            if observer != 0 && observer <= player as usize {
                continue;
            }
            let holder = (players + player as usize - observer) % players;
            match action {
                Move::Play(_) => view.played(holder, pos as usize, card, success),
                Move::Discard(_) => view.discarded(holder, pos as usize, card),
                Move::Clue(..) => unreachable!(),
            }
        }
        self.lost_points += newly_dead_states(&before, &self.views[0].card_states);
        if self.initial.is_some() {
            self.actions.push((player, action, card, success));
        }
        match action {
            Move::Play(_) if !success => {
                self.strikes += 1;
                self.new_strikes += 1;
            }
            Move::Play(_) if card.rank == 5 => self.clues = (self.clues + 1).min(8),
            Move::Discard(_) => self.clues += 1,
            _ => {}
        }
        for view in &mut self.views {
            view.context.clues = self.clues;
        }
    }

    fn danger(&self) -> f64 {
        self.lost_points as f64 * LOST_POINT_COST
            + self.new_strikes as f64 * STRIKE_COST
            + if self.strikes >= 3 {
                STRIKEOUT_COST
            } else {
                0.0
            }
    }
}

fn newly_dead(before: &Line, after: &Line) -> u8 {
    newly_dead_states(&before.card_states, &after.card_states)
}

fn newly_dead_states(
    before: &super::card_states::CardStates,
    after: &super::card_states::CardStates,
) -> u8 {
    before
        .iter()
        .filter(|(card, state)| {
            state.play != CardPlayState::Dead() && after[card].play == CardPlayState::Dead()
        })
        .count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CardQuantum, PositionSet,
        game::{Game, PlayerStrategy, Suit},
        hyphenated::HyphenatedPlayer,
    };

    fn card(suit: Suit, rank: u8) -> Card {
        Card { suit, rank }
    }

    fn status(clues: u8) -> GameStatus {
        GameStatus {
            turn: 0,
            score: 0,
            max_score: 25,
            num_strikes: 0,
            clues,
            blind_plays: 0,
        }
    }

    #[test]
    fn pruning_preserves_every_decision_in_seeded_games() {
        #[derive(Debug)]
        struct CheckedPlayer(HyphenatedPlayer);

        impl PlayerStrategy for CheckedPlayer {
            fn init(&mut self, count: u8, own: u8) {
                self.0.init(count, own);
            }
            fn drawn(&mut self, player: usize, card: Card) {
                self.0.drawn(player, card);
            }
            fn own_drawn(&mut self) {
                self.0.own_drawn();
            }
            fn played(&mut self, player: usize, pos: usize, card: Card, successful: bool) {
                self.0.played(player, pos, card, successful);
            }
            fn discarded(&mut self, player: usize, pos: usize, card: Card) {
                self.0.discarded(player, pos, card);
            }
            fn clued(&mut self, who: usize, whom: usize, clue: Clue, touched: PositionSet) {
                self.0.clued(who, whom, clue, touched);
            }
            fn act(&mut self, status: &GameStatus) -> Move {
                let line = self.0.line();
                let pruned = choose_action_with_pruning(&line, status, false, true);
                let exhaustive = choose_action_with_pruning(&line, status, false, false);
                assert_eq!(pruned, exhaustive, "turn {}", status.turn);
                pruned
            }
        }

        for count in 2..=5 {
            for seed in 0..3 {
                let mut bots: Vec<_> = (0..count)
                    .map(|_| CheckedPlayer(HyphenatedPlayer::new(false)))
                    .collect();
                let mut players: Vec<&mut dyn PlayerStrategy> = bots
                    .iter_mut()
                    .map(|bot| bot as &mut dyn PlayerStrategy)
                    .collect();
                let mut game = Game::new(&mut players, false, seed);
                game.run(&mut players);
            }
        }
    }

    fn position() -> Line {
        position_with_next(card(Suit::Red(), 4))
    }

    fn position_with_next(first: Card) -> Line {
        use Suit::*;
        let mut line = Line::new(4, 0);
        for _ in 0..4 {
            line.own_drawn();
        }
        for (player, hand) in [
            [
                (first.suit, first.rank),
                (Blue(), 4),
                (Green(), 4),
                (Yellow(), 5),
            ],
            [(Purple(), 3), (Blue(), 1), (Green(), 1), (Red(), 3)],
            [(Yellow(), 4), (Purple(), 4), (Blue(), 3), (Green(), 3)],
        ]
        .iter()
        .enumerate()
        {
            for &(suit, rank) in hand.iter().rev() {
                line.drawn(player + 1, card(suit, rank));
            }
        }
        line
    }

    fn know(line: &mut Line, player: u8, pos: u8, card: Card) {
        let mut quantum = CardQuantum::new(Variant {});
        quantum.clear();
        quantum.add_card(&card, false);
        let slot = line.hands.slot_mut(player, pos);
        slot.clued = true;
        slot.quantum = quantum;
        slot.base_quantum = quantum;
        slot.update_slot_attributes(&line.card_states);
    }

    #[test]
    fn discard_generates_a_token_instead_of_playing_and_losing_a_five() {
        let mut line = position();
        know(&mut line, 0, 0, card(Suit::Red(), 1));
        let trash = card(Suit::Yellow(), 1);
        line.card_states.played(&trash);
        know(&mut line, 0, 3, trash);
        let before = line.clone();
        assert_eq!(choose_action(&line, &status(0), false), Move::Discard(3));
        assert_eq!(line, before, "search must not mutate live knowledge");
        let mut played = Forecast::own_card(&line, &status(0), Move::Play(0), card(Suit::Red(), 1));
        played.rollout();
        assert_eq!(played.lost_points, 1);
        assert_eq!(
            played.clues, 1,
            "the next player's forced discard restores a token"
        );
        assert!(
            played
                .views
                .iter()
                .all(|view| view.context.clues == played.clues)
        );
    }

    #[test]
    fn lazy_forecasts_match_eager_player_views() {
        for clues in 0..=2 {
            let mut line = position();
            know(&mut line, 1, 0, card(Suit::Red(), 1));
            // A known play followed by an unknown hand exercises both replay
            // of earlier actions and stopping before unused seats are built.
            for (identity, _) in own_card_outcomes(&line, 0) {
                for action in [Move::Play(0), Move::Discard(0)] {
                    let status = status(clues);
                    let mut lazy = Forecast::own_card(&line, &status, action, identity);
                    let mut eager = Forecast::own_card(&line, &status, action, identity);
                    let initial = eager.initial.take().unwrap();
                    for player in 1..initial.hands.num_players {
                        eager.views.push(initial.decision_view(player));
                    }
                    lazy.rollout();
                    eager.rollout();
                    assert_eq!(lazy.views[0], eager.views[0]);
                    assert_eq!(lazy.clues, eager.clues);
                    assert_eq!(lazy.danger(), eager.danger());
                }
            }
        }
    }

    #[test]
    fn last_token_clue_must_not_strand_the_next_players_critical_chop() {
        let line = position();
        let unsafe_clue = Move::Clue(2, Clue::Rank(1));
        let (mut forecast, _) = Forecast::clue(&line, &status(1), 2, Clue::Rank(1)).unwrap();
        forecast.rollout();
        assert_eq!(forecast.lost_points, 1);
        let chosen = choose_action(&line, &status(1), false);
        assert_ne!(chosen, unsafe_clue);
        assert!(
            evaluate(&line, &status(1), chosen).unwrap().value
                > evaluate(&line, &status(1), unsafe_clue).unwrap().value
        );

        let (mut funded, _) = Forecast::clue(&line, &status(2), 2, Clue::Rank(1)).unwrap();
        funded.rollout();
        assert_eq!(
            funded.lost_points, 0,
            "with a token the discard is not forced"
        );
    }

    #[test]
    fn known_trash_is_discarded_before_a_critical_chop() {
        let mut line = position();
        let trash = card(Suit::Red(), 4);
        line.card_states.played(&trash);
        know(&mut line, 1, 0, trash);
        let (mut forecast, _) = Forecast::clue(&line, &status(1), 2, Clue::Rank(1)).unwrap();
        forecast.rollout();
        assert_eq!(forecast.lost_points, 0);
        assert_eq!(forecast.clues, 1);
        assert!(
            forecast.views[0]
                .hands
                .iter_hand(1)
                .any(|(_, slot)| slot.card == card(Suit::Yellow(), 5))
        );
    }

    #[test]
    fn playing_a_five_funds_the_next_players_turn() {
        let mut line = position();
        for rank in 1..5 {
            line.card_states.played(&card(Suit::Red(), rank));
        }
        know(&mut line, 0, 0, card(Suit::Red(), 5));
        let mut forecast =
            Forecast::own_card(&line, &status(0), Move::Play(0), card(Suit::Red(), 5));
        forecast.rollout();
        assert_eq!(forecast.lost_points, 0);
        assert_eq!(forecast.clues, 1);
        assert_eq!(choose_action(&line, &status(0), false), Move::Play(0));
        assert!(
            forecast
                .views
                .iter()
                .all(|view| view.context.clues == forecast.clues)
        );
    }

    #[test]
    fn another_players_five_prevents_a_later_forced_discard() {
        let mut line = position_with_next(card(Suit::Red(), 5));
        for rank in 1..5 {
            line.card_states.played(&card(Suit::Red(), rank));
        }
        know(&mut line, 1, 0, card(Suit::Red(), 5));
        // After the last-token clue, the next seat plays r5. Its returned token
        // lets the following seat choose a clue instead of discarding.
        let (mut forecast, _) = Forecast::clue(&line, &status(1), 1, Clue::Rank(5)).unwrap();
        let following_hand = forecast.views[0].hands.hand_sizes[2];
        forecast.rollout();
        assert_eq!(forecast.clues, 1);
        assert_eq!(forecast.lost_points, 0);
        assert_eq!(forecast.views[0].hands.hand_sizes[2], following_hand);
    }

    #[test]
    fn ordinary_unknown_chop_risk_does_not_replace_a_normal_discard() {
        let mut line = Line::new(4, 0);
        for _ in 0..4 {
            line.own_drawn();
        }
        for player in 1..4 {
            let rank = if player == 2 { 4 } else { 3 };
            for suit in [Suit::Red(), Suit::Yellow(), Suit::Green(), Suit::Blue()] {
                line.drawn(player, card(suit, rank));
            }
        }
        let evaluation = evaluate(&line, &status(3), Move::Discard(3)).unwrap();
        assert!(evaluation.danger > 0.0, "an unknown chop could be a five");
        assert_eq!(evaluation.urgent, 0.0);
        assert_eq!(choose_action(&line, &status(3), false), Move::Discard(3));
    }

    #[test]
    fn third_strike_ends_the_forecast() {
        let mut line = position_with_next(card(Suit::Red(), 2));
        know(&mut line, 1, 0, card(Suit::Red(), 1));
        let mut forecast = Forecast::from_line(line.clone(), 0, 2);
        forecast.rollout();
        assert_eq!(forecast.strikes, 3);
        assert_eq!(forecast.new_strikes, 1);
        assert!(forecast.danger() >= STRIKEOUT_COST);
        assert_eq!(
            forecast.clues, 0,
            "no discard should be simulated after a strikeout"
        );
        assert_eq!(
            forecast.views[0].hands.hand_sizes[2],
            line.hands.hand_sizes[2]
        );
    }

    #[test]
    fn urgent_fix_precedes_our_own_play() {
        let mut line = position_with_next(card(Suit::Red(), 2));
        know(&mut line, 0, 0, card(Suit::Purple(), 1));
        // A color-clued r2 has been incorrectly inferred to be r1. A rank clue
        // corrects the soft inference without contradicting literal information.
        let mut quantum = CardQuantum::new(Variant {});
        quantum.limit_by_suit(&Suit::Red(), true);
        quantum.soft_clear();
        quantum.add_card(&card(Suit::Red(), 1), true);
        let slot = line.hands.slot_mut(1, 0);
        slot.clued = true;
        slot.quantum = quantum;
        slot.base_quantum = quantum;
        slot.play = true;
        let mut forecast =
            Forecast::own_card(&line, &status(1), Move::Play(0), card(Suit::Purple(), 1));
        forecast.rollout();
        assert_eq!(forecast.new_strikes, 1);
        assert_eq!(
            choose_action(&line, &status(1), false),
            Move::Clue(1, Clue::Rank(2))
        );
    }

    #[test]
    fn own_unknown_cards_use_remaining_copies_and_literal_fallback() {
        let mut line = position();
        let outcomes = own_card_outcomes(&line, 3);
        assert!(outcomes.contains(&(card(Suit::Purple(), 1), 3)));
        assert!(outcomes.contains(&(card(Suit::Blue(), 1), 2)));
        assert!(
            !outcomes
                .iter()
                .any(|(identity, _)| *identity == card(Suit::Yellow(), 5))
        );
        let slot = line.hands.slot_mut(0, 3);
        slot.quantum.limit_by_rank(1, true);
        slot.quantum.soft_clear();
        let outcomes = own_card_outcomes(&line, 3);
        assert!(!outcomes.is_empty());
        assert!(outcomes.iter().all(|(card, _)| card.rank == 1));
    }

    #[test]
    fn candidate_generation_respects_token_limits() {
        let line = position();
        assert!(
            candidates(&line, 0)
                .iter()
                .all(|action| !matches!(action, Move::Clue(..)))
        );
        assert!(
            candidates(&line, 8)
                .iter()
                .all(|action| !matches!(action, Move::Discard(..)))
        );
        assert!(matches!(
            choose_action(&line, &status(8), false),
            Move::Clue(..)
        ));
    }

    #[test]
    fn critical_loss_counts_blocked_higher_ranks_only_once() {
        let mut line = position();
        line.card_states.discarded(&card(Suit::Purple(), 4));
        line.card_states.discarded(&card(Suit::Purple(), 4));
        line.card_states.discarded(&card(Suit::Purple(), 2));
        let mut after = line.clone();
        after.card_states.discarded(&card(Suit::Purple(), 2));
        assert_eq!(
            newly_dead(&line, &after),
            2,
            "only p2 and p3 remain achievable"
        );
    }

    #[test]
    fn replay_31554_rejects_the_clue_that_sacrifices_yellow_five() {
        // Replaces the ignored line_tests regression. The clue's interpretation
        // is valid; its safety depends on the token budget and action policy.
        let mut bots: Vec<_> = (0..4).map(|_| HyphenatedPlayer::new(false)).collect();
        let mut players: Vec<&mut dyn PlayerStrategy> = bots
            .iter_mut()
            .map(|p| p as &mut dyn PlayerStrategy)
            .collect();
        let game = Game::from_replay(
            20,
            "415tdcbfjvfmhuixnglskwuqigaahmnlsyapqoxudrekvpfwkprcb",
            "05pbahDa6codaeakaoubar6b6cudagasapbb1caibm7bbfajubbcocbzpaaybu7bbt0ba6bwan6cb8a9vaadbBa4bxvd0c7aa7b5bqaC0aa3bEibbHaaaMb2bNqa",
            "0",
            &mut players,
        );
        let line = bots[0].line();
        assert_eq!(game.status.clues, 1);
        let (mut forecast, _) = Forecast::clue(&line, &game.status, 1, Clue::Rank(4)).unwrap();
        forecast.rollout();
        assert_eq!(
            forecast.lost_points, 1,
            "a delayed play does not prevent the y5 discard"
        );
        assert_ne!(bots[0].act(&game.status), Move::Clue(1, Clue::Rank(4)));
    }
}
