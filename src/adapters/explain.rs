//! `raikan explain` (docs/architecture.md §8.2): recompute one decision of a game and show
//! what was chosen, what it was compared against and why.
//!
//! A decision is reproducible from (replay prefix, configuration, seed), so nothing is
//! logged live: the replay is fed to fresh bots up to the turn, the bot of the seat to move
//! decides again, and every option of interest is evaluated on the same sampled worlds:
//!
//! - the position as the deciding seat sees it (true identities added for the reader);
//! - the blueprint's choice and reason, and the bot's choice under the configuration
//!   (with the planner's own table when search is on);
//! - a comparison of the blueprint candidates, the action actually played and any
//!   requested action, each with the legibility guard's verdict, how every seat would read
//!   a clue, and its simulated value with paired difference to the blueprint's choice;
//! - optionally one rollout step by step in a chosen sampled world;
//! - optionally the history of one card's note: which events (and rules) changed it.

use std::sync::Arc;

use serde::Serialize;

use super::text::{self, Names};
use crate::agent::{Agent, BotAgent, Config};
use crate::conventions::{Profile, View};
use crate::domain::{CardId, Seat};
use crate::engine::{self, Action, Event, GameState, Replay, World};
use crate::policy;
use crate::search::{self, SearchConfig, SearchMode};
use crate::trace::InterpretationTrace;
use crate::variants::Variant;

/// What to explain.
#[derive(Clone, Debug)]
pub struct Request {
    /// hanab.live turn number (1 = the first action): the decision before that action.
    pub turn: usize,
    /// Extra actions to compare, in text (`play 2`, `clue Bob red`).
    pub candidates: Vec<String>,
    /// Compare every legal action, not only the blueprint's candidates.
    pub all: bool,
    /// Show the rollouts of the compared actions in this sampled world.
    pub world: Option<usize>,
    /// Show the history of this card's note (`#17` or `SEAT:SLOT`).
    pub card: Option<String>,
    /// Whose notes the card history shows (default: the deciding seat).
    pub viewer: Option<String>,
    /// Budget of the comparison (worlds per action); `Off` uses the `fast` defaults.
    pub compare: SearchConfig,
}

impl Request {
    pub fn new(turn: usize) -> Self {
        Self {
            turn,
            candidates: Vec::new(),
            all: false,
            world: None,
            card: None,
            viewer: None,
            compare: SearchConfig::new(SearchMode::Fast),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Explanation {
    pub turn: usize,
    pub seat: u8,
    pub player: String,
    pub level: u8,
    pub profile_digest: String,
    pub position: Position,
    /// The blueprint's choice (no search).
    pub blueprint: Choice,
    /// The bot's choice under the configuration (the planner's when search is on).
    pub chosen: Choice,
    /// The action of the replay at this turn, if the game went on.
    pub played: Option<String>,
    /// The planner's own candidate table, when search is on.
    pub plan: Option<PlanTable>,
    pub comparison: ComparisonTable,
    pub rollouts: Vec<RolloutTrace>,
    pub card: Option<CardHistory>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Position {
    pub stacks: String,
    pub score: u8,
    pub clue_tokens: u8,
    pub strikes: u8,
    pub deck: u8,
    pub discards: String,
    pub hands: Vec<HandView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandView {
    pub seat: u8,
    pub player: String,
    /// Newest first.
    pub cards: Vec<CardView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CardView {
    pub slot: usize,
    pub card: u8,
    /// The true identity (from the replay; the deciding seat does not see its own).
    pub identity: String,
    /// The deciding seat's note on the card.
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    pub action: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanTable {
    pub reason: String,
    pub worlds: usize,
    pub effective_size: f64,
    pub seed: u64,
    pub paired_difference: Option<(f64, f64)>,
    pub rows: Vec<PlanRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanRow {
    pub action: String,
    pub visits: usize,
    pub mean: f64,
    pub stderr: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComparisonTable {
    pub worlds: usize,
    pub effective_size: f64,
    pub seed: u64,
    /// The first row is the blueprint's choice, the reference of the differences.
    pub rows: Vec<ComparisonRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComparisonRow {
    pub action: String,
    /// Why the row is listed: `blueprint`, `candidate`, `chosen`, `played`, `requested`,
    /// `legal`.
    pub sources: Vec<&'static str>,
    /// The legibility guard's verdict (§7.5).
    pub guard: String,
    /// How each seat reads a clue (rule, focus and readings).
    pub readings: Vec<String>,
    pub visits: usize,
    pub mean: f64,
    pub stderr: f64,
    /// Paired difference to the blueprint's choice and its standard error.
    pub difference: Option<(f64, f64)>,
    pub breakdown: Vec<(&'static str, f64)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RolloutTrace {
    pub action: String,
    pub world: usize,
    /// The deciding seat's hand in that world, newest first.
    pub hand: String,
    pub steps: Vec<String>,
    pub value: f64,
    pub breakdown: Vec<(&'static str, f64)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CardHistory {
    pub card: u8,
    pub identity: String,
    pub viewer: String,
    pub entries: Vec<CardEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CardEntry {
    /// hanab.live turn of the action (0 = the deal).
    pub turn: usize,
    pub event: String,
    /// The rule that read the clue, for clue events.
    pub rule: Option<String>,
    pub note: String,
}

fn breakdown(b: &search::Breakdown) -> Vec<(&'static str, f64)> {
    b.terms().to_vec()
}

fn reading_text(state: &GameState, t: &InterpretationTrace) -> String {
    let focus = text::slot(state, CardId(t.focus)).map_or_else(
        || format!("#{}", t.focus),
        |(_, s)| format!("slot {s} (#{})", t.focus),
    );
    let mut out = format!("{} focus {focus}", t.rule);
    if !t.readings.is_empty() {
        let readings: Vec<String> = t
            .readings
            .iter()
            .map(|r| {
                if r.steps.is_empty() {
                    format!("{} [{}]", r.focus, r.tier)
                } else {
                    format!("{} [{}] via {}", r.focus, r.tier, r.steps.join(", "))
                }
            })
            .collect();
        out.push_str(": ");
        out.push_str(&readings.join(" | "));
    }
    if t.errors > 0 {
        out.push_str(&format!(" ({} errors)", t.errors));
    }
    out
}

/// How every seat reads our clue: the giver's intended reading, then each teammate in its
/// own kept view.
fn readings(view: &View, names: &Names, target: Seat, clue: crate::domain::ClueId) -> Vec<String> {
    let touched = view.visible_touched(target, clue);
    if touched.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut after = view.detached();
    after.enable_trace();
    after.give_clue(target, clue);
    if let Some(t) = after.trace.take().pop() {
        out.push(format!(
            "{} (giver): {}",
            names.seat(view.me),
            reading_text(&view.state, &t)
        ));
    }
    let event = Event::Clue {
        giver: view.me,
        target,
        clue,
        touched,
    };
    for seat in view.others() {
        let mut seat_view = view.perspective(seat);
        seat_view.enable_trace();
        seat_view.observe(&event.observed_by(seat));
        if let Some(t) = seat_view.trace.take().pop() {
            out.push(format!(
                "{}: {}",
                names.seat(seat),
                reading_text(&view.state, &t)
            ));
        }
    }
    out
}

fn guard_verdict(view: &View, baseline: &crate::conventions::Worth, action: Action) -> String {
    match action {
        Action::Play(card) if policy::guard::known_playable(view, card) => {
            "ok (known playable)".to_string()
        }
        Action::Play(_) => "ok".to_string(),
        Action::Discard(card) if policy::guard::discard_passes_play_clue(view, card) => {
            "rejected: fills the clue tokens while a play clue is available".to_string()
        }
        Action::Discard(card) if policy::guard::known_trash(view, card) => {
            "ok (known trash)".to_string()
        }
        Action::Discard(card) => {
            if policy::guard::legible_discard(view, card) {
                "ok".to_string()
            } else {
                "rejected: discard while holding a convention play".to_string()
            }
        }
        Action::Clue { target, clue }
            if policy::guard::delays_play_for_finesse(view, target, clue) =>
        {
            "rejected: delays a convention play for a potential finesse".to_string()
        }
        Action::Clue { target, clue } => match view.clue_views_expecting(
            target,
            clue,
            |_, _| true,
            Some(&policy::guard::expected_action),
        ) {
            None => "rejected: touches nothing".to_string(),
            Some((worth, _)) if worth.no_more_errors_than(baseline) => {
                format!(
                    "ok ({} errors, position has {})",
                    worth.errors, baseline.errors
                )
            }
            Some((worth, _)) => format!(
                "rejected: {} errors, position has {}",
                worth.errors, baseline.errors
            ),
        },
    }
}

/// Recompute and explain the decision before `request.turn` of `replay`.
pub fn explain(replay: &Replay, config: &Config, request: &Request) -> Result<Explanation, String> {
    let variant = Variant::by_name(&replay.variant).map_err(|e| e.to_string())?;
    let names = Names::new(&replay.players);
    let num_seats = replay.num_seats();
    let actions: Vec<Action> = replay.game_actions().collect();
    if request.turn == 0 || request.turn > actions.len() + 1 {
        return Err(format!(
            "turn {} is outside 1..={} of this game",
            request.turn,
            actions.len() + 1
        ));
    }
    let prefix = request.turn - 1;
    let world = &replay.deck;

    let mut bots: Vec<BotAgent> = (0..num_seats)
        .map(|_| BotAgent::new(config.clone()))
        .collect();
    for (s, bot) in bots.iter_mut().enumerate() {
        bot.start(&variant, num_seats, Seat(s as u8));
    }
    let (mut state, deal) = engine::deal(&variant, num_seats, world);
    let feed = |bots: &mut [BotAgent], event: &Event| {
        for (s, bot) in bots.iter_mut().enumerate() {
            bot.observe(&event.observed_by(Seat(s as u8)));
        }
    };
    // Every event with its turn, for the card history.
    let mut events: Vec<(usize, Event)> = Vec::new();
    for event in &deal {
        feed(&mut bots, event);
        events.push((0, *event));
    }
    for (i, &action) in actions.iter().take(prefix).enumerate() {
        engine::check_legal(&variant, &state, action)
            .map_err(|e| format!("turn {}: {action:?} is illegal: {e:?}", i + 1))?;
        for event in engine::step_recorded(&variant, &mut state, world, action) {
            feed(&mut bots, &event);
            events.push((i + 1, event));
        }
    }
    if state.is_over() {
        return Err(format!(
            "the game is over before turn {} (its last decision is turn {})",
            request.turn,
            actions.len()
        ));
    }

    let seat = state.current;
    let bot = &mut bots[seat.index()];
    // a full copy: the comparison must see the kept teammate views, as the planner does
    let view = bot.view().clone();
    let blueprint = policy::choose_action(&view);
    let chosen_action = bot.choose_action(&state);
    let plan = bot.last_plan().cloned();
    let fmt = |a: Action| text::action(&variant, &names, &state, Some(world), a);

    let card_history = match &request.card {
        Some(spec) => {
            let card = text::parse_card(&names, &state, spec)?;
            if card.0 >= state.next_card {
                return Err(format!(
                    "card #{} is not dealt by turn {}",
                    card.0, request.turn
                ));
            }
            let viewer = match &request.viewer {
                Some(v) => names
                    .parse_seat(v)
                    .ok_or_else(|| format!("unknown seat {v}"))?,
                None => seat,
            };
            Some(card_history(
                &variant, &names, config, num_seats, viewer, card, world, &events,
            )?)
        }
        None => None,
    };

    // Rows: the blueprint's choice first (the reference), then its other candidates, the
    // bot's choice, the action played and requested actions.
    let mut rows: Vec<(Action, Vec<&'static str>)> = vec![(blueprint.action, vec!["blueprint"])];
    let add = |rows: &mut Vec<(Action, Vec<&'static str>)>, action: Action, source| match rows
        .iter_mut()
        .find(|(a, _)| *a == action)
    {
        Some((_, sources)) => {
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        None => rows.push((action, vec![source])),
    };
    for action in policy::candidates(&view) {
        add(&mut rows, action, "candidate");
    }
    add(&mut rows, chosen_action, "chosen");
    let played = actions.get(prefix).copied();
    if let Some(action) = played {
        add(&mut rows, action, "played");
    }
    for spec in &request.candidates {
        let action = text::parse_action(&variant, &names, &state, seat, spec)?;
        engine::check_legal(&variant, &state, action)
            .map_err(|e| format!("{spec}: illegal ({e:?})"))?;
        if let Action::Clue { target, clue } = action
            && view.visible_touched(target, clue).is_empty()
        {
            return Err(format!("{spec}: the clue touches nothing"));
        }
        add(&mut rows, action, "requested");
    }
    if request.all {
        for action in engine::legal_commands(&variant, &state, world) {
            add(&mut rows, action, "legal");
        }
    }

    let mut compare_config = if config.search.enabled() {
        config.search.clone()
    } else {
        request.compare.clone()
    };
    compare_config.seed = config.search.seed.or(request.compare.seed);
    let row_actions: Vec<Action> = rows.iter().map(|(a, _)| *a).collect();
    let comparison = search::compare(&view, &row_actions, &compare_config);
    let baseline = view.worth(0);
    let comparison_rows = rows
        .iter()
        .zip(&comparison.candidates)
        .zip(&comparison.paired)
        .map(|(((action, sources), stats), paired)| ComparisonRow {
            action: fmt(*action),
            sources: sources.clone(),
            guard: guard_verdict(&view, &baseline, *action),
            readings: match *action {
                Action::Clue { target, clue } => readings(&view, &names, target, clue),
                _ => Vec::new(),
            },
            visits: stats.visits,
            mean: stats.mean,
            stderr: stats.stderr,
            difference: *paired,
            breakdown: breakdown(&stats.breakdown),
        })
        .collect();

    // Rollouts in one world: the requested actions, else the blueprint's and the chosen.
    let mut rollouts = Vec::new();
    if let Some(w) = request.world {
        let (seed, samples) = search::sample_worlds(&view, &compare_config);
        let sampled = samples.worlds.get(w).ok_or_else(|| {
            format!(
                "world {w} does not exist ({} worlds sampled)",
                samples.worlds.len()
            )
        })?;
        let mut shown: Vec<Action> = rows
            .iter()
            .filter(|(_, s)| s.contains(&"requested"))
            .map(|(a, _)| *a)
            .collect();
        if shown.is_empty() {
            shown.push(blueprint.action);
            if chosen_action != blueprint.action {
                shown.push(chosen_action);
            }
        }
        for action in shown {
            rollouts.push(rollout(
                &variant,
                &names,
                &view,
                &compare_config,
                seed,
                sampled,
                w,
                action,
            ));
        }
    }

    let hands = state
        .seats()
        .map(|s| {
            let hand = state.hand(s);
            HandView {
                seat: s.0,
                player: names.seat(s),
                cards: hand
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(i, &c)| CardView {
                        slot: hand.len() - i,
                        card: c.0,
                        identity: variant.identity_name(world.identity(c)),
                        note: text::note(&variant, view.note(c)),
                    })
                    .collect(),
            }
        })
        .collect();
    let discards = variant
        .all_identities()
        .iter()
        .filter(|i| state.discarded[i.index()] > 0)
        .map(|i| {
            let n = state.discarded[i.index()];
            if n > 1 {
                format!("{}x{n}", variant.identity_name(i))
            } else {
                variant.identity_name(i)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    Ok(Explanation {
        turn: request.turn,
        seat: seat.0,
        player: names.seat(seat),
        level: config.max_level,
        profile_digest: format!("{:016x}", view.profile.digest),
        position: Position {
            stacks: text::stacks(&variant, &state),
            score: state.score(&variant),
            clue_tokens: state.clue_tokens(),
            strikes: state.strikes,
            deck: state.cards_left(),
            discards,
            hands,
        },
        blueprint: Choice {
            action: fmt(blueprint.action),
            reason: format!("{:?}", blueprint.reason),
        },
        chosen: Choice {
            action: fmt(chosen_action),
            reason: plan.as_ref().map_or_else(
                || format!("{:?}", blueprint.reason),
                |p| format!("{:?}", p.reason),
            ),
        },
        played: played.map(fmt),
        plan: plan.map(|p| PlanTable {
            reason: format!("{:?}", p.reason),
            worlds: p.worlds,
            effective_size: p.effective_size,
            seed: p.seed,
            paired_difference: p.paired_difference,
            rows: p
                .candidates
                .iter()
                .map(|c| PlanRow {
                    action: fmt(c.action),
                    visits: c.visits,
                    mean: c.mean,
                    stderr: c.stderr,
                })
                .collect(),
        }),
        comparison: ComparisonTable {
            worlds: comparison.worlds,
            effective_size: comparison.effective_size,
            seed: comparison.seed,
            rows: comparison_rows,
        },
        rollouts,
        card: card_history,
    })
}

/// The viewer's note on `card` after every event that touched it or changed the note.
#[allow(clippy::too_many_arguments)]
fn card_history(
    variant: &Variant,
    names: &Names,
    config: &Config,
    num_seats: u8,
    viewer: Seat,
    card: CardId,
    world: &World,
    events: &[(usize, Event)],
) -> Result<CardHistory, String> {
    let profile = Profile::new(config.max_level, variant, config.require_complete)
        .map_err(|e| format!("{e:?}"))?;
    let mut view = View::with_seat_views(
        Arc::new(variant.clone()),
        Arc::new(profile),
        num_seats,
        viewer,
    );
    view.enable_trace();
    let mut entries: Vec<CardEntry> = Vec::new();
    let mut last: Option<String> = None;
    for (turn, event) in events {
        view.observe(&event.observed_by(viewer));
        let traces = view.trace.take();
        if card.0 >= view.state.next_card {
            continue;
        }
        let involves = match *event {
            Event::Draw { card: c, .. }
            | Event::Play { card: c, .. }
            | Event::Discard { card: c, .. } => c == card,
            Event::Clue { touched, .. } => touched.contains(card),
            Event::GameOver { .. } => false,
        };
        let rule = match event {
            Event::Clue { touched, .. } if touched.contains(card) => {
                traces.last().map(|t| t.rule.to_string())
            }
            _ => None,
        };
        let note = text::note(variant, view.note(card));
        if involves || last.as_deref() != Some(note.as_str()) {
            entries.push(CardEntry {
                turn: *turn,
                event: text::event(variant, names, event),
                rule,
                note: note.clone(),
            });
            last = Some(note);
        }
    }
    Ok(CardHistory {
        card: card.0,
        identity: variant.identity_name(world.identity(card)),
        viewer: names.seat(viewer),
        entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn rollout(
    variant: &Variant,
    names: &Names,
    view: &View,
    config: &SearchConfig,
    seed: u64,
    world: &World,
    index: usize,
    action: Action,
) -> RolloutTrace {
    let views = search::rollout::seat_views(view, world);
    let partners = search::rollout::Partners::for_world(config.partner_spread, seed, index);
    let (leaf, steps) = search::simulate_traced(view, world, views, action, partners);
    let mut state = view.state;
    let hand = state
        .hand(view.me)
        .iter()
        .rev()
        .map(|&c| variant.identity_name(world.identity(c)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = Vec::new();
    for step in &steps {
        let mut line = format!(
            "{}: {}",
            names.seat(step.seat),
            text::action(variant, names, &state, Some(world), step.action)
        );
        if let Some(reason) = step.reason {
            line.push_str(&format!(" [{reason:?}]"));
        }
        if step.alternative {
            line.push_str(" [partner alternative]");
        }
        if step.fallback {
            line.push_str(" [fallback: prediction illegal in this world]");
        }
        let events = engine::step(variant, &mut state, world, step.action);
        for event in events.iter() {
            if let Event::Play { success: false, .. } = event {
                line.push_str(" -> misplay");
            }
        }
        lines.push(line);
    }
    RolloutTrace {
        action: text::action(variant, names, &view.state, Some(world), action),
        world: index,
        hand,
        steps: lines,
        value: leaf.total(),
        breakdown: breakdown(&leaf),
    }
}

fn signed(value: f64) -> String {
    format!("{value:+.2}")
}

impl Explanation {
    /// The text rendering for the terminal.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let p = &self.position;
        out.push_str(&format!(
            "Turn {} — {} (seat {}) to move; level {}, profile {}\n",
            self.turn, self.player, self.seat, self.level, self.profile_digest
        ));
        out.push_str(&format!(
            "stacks {} (score {}), clues {}, strikes {}, deck {}\ndiscards: {}\n\n",
            p.stacks,
            p.score,
            p.clue_tokens,
            p.strikes,
            p.deck,
            if p.discards.is_empty() {
                "-"
            } else {
                &p.discards
            }
        ));
        out.push_str(&format!("Hands ({}'s notes):\n", self.player));
        for hand in &p.hands {
            let marker = if hand.seat == self.seat {
                " (to move)"
            } else {
                ""
            };
            out.push_str(&format!("  {}{marker}\n", hand.player));
            for card in &hand.cards {
                out.push_str(&format!(
                    "    {} #{:<2} {:<3} {}\n",
                    card.slot, card.card, card.identity, card.note
                ));
            }
        }
        out.push_str(&format!(
            "\nBlueprint: {} [{}]\n",
            self.blueprint.action, self.blueprint.reason
        ));
        out.push_str(&format!(
            "Bot:       {} [{}]\n",
            self.chosen.action, self.chosen.reason
        ));
        match &self.played {
            Some(played) => out.push_str(&format!("Played:    {played}\n")),
            None => out.push_str("Played:    - (end of the replay)\n"),
        }
        if let Some(plan) = &self.plan {
            out.push_str(&format!(
                "\nPlanner: {} on {} worlds (ESS {:.1}, seed {:016x})",
                plan.reason, plan.worlds, plan.effective_size, plan.seed
            ));
            if let Some((d, se)) = plan.paired_difference {
                out.push_str(&format!(", best − blueprint {} ± {se:.2}", signed(d)));
            }
            out.push('\n');
            for row in &plan.rows {
                out.push_str(&format!(
                    "  {:>7.2} ± {:<5.2} n={:<4} {}\n",
                    row.mean, row.stderr, row.visits, row.action
                ));
            }
        }
        let c = &self.comparison;
        out.push_str(&format!(
            "\nComparison on {} common worlds (ESS {:.1}, seed {:016x}); difference to the blueprint's choice:\n",
            c.worlds, c.effective_size, c.seed
        ));
        let mut order: Vec<usize> = (0..c.rows.len()).collect();
        order.sort_by(|&a, &b| c.rows[b].mean.total_cmp(&c.rows[a].mean).then(a.cmp(&b)));
        for i in order {
            let row = &c.rows[i];
            let diff = row.difference.map_or_else(
                || "  (ref)       ".to_string(),
                |(d, se)| format!("{:>6} ± {:<5.2}", signed(d), se),
            );
            out.push_str(&format!(
                "  {:>6.2} {diff} {} [{}]\n",
                row.mean,
                row.action,
                row.sources.join(",")
            ));
            if row.guard != "ok" {
                out.push_str(&format!("      guard: {}\n", row.guard));
            }
            for reading in &row.readings {
                out.push_str(&format!("      {reading}\n"));
            }
            let terms: Vec<String> = row
                .breakdown
                .iter()
                .filter(|(_, v)| v.abs() >= 0.005)
                .map(|(n, v)| format!("{n} {v:.2}"))
                .collect();
            out.push_str(&format!("      value: {}\n", terms.join(", ")));
        }
        for r in &self.rollouts {
            out.push_str(&format!(
                "\nRollout of {} in world {} (own hand {}):\n",
                r.action, r.world, r.hand
            ));
            for step in &r.steps {
                out.push_str(&format!("  {step}\n"));
            }
            let terms: Vec<String> = r
                .breakdown
                .iter()
                .filter(|(_, v)| v.abs() >= 0.005)
                .map(|(n, v)| format!("{n} {v:.2}"))
                .collect();
            out.push_str(&format!("  leaf {:.2}: {}\n", r.value, terms.join(", ")));
        }
        if let Some(card) = &self.card {
            out.push_str(&format!(
                "\nCard #{} ({}) as {} saw it:\n",
                card.card, card.identity, card.viewer
            ));
            for e in &card.entries {
                let rule = e
                    .rule
                    .as_deref()
                    .map(|r| format!(" [{r}]"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  turn {:>2}: {}{rule}\n           -> {}\n",
                    e.turn, e.event, e.note
                ));
            }
        }
        out
    }
}
