//! Bug reports as convention scenarios (the format of `tests/scenarios`): the deal, the
//! draws and every action of a game up to one decision, then the action the bot should take
//! there (`action S: …`) and the one it should not (`not-action S: …`). Dropped into
//! `tests/scenarios`, a report is a failing test until the bot decides as expected.

use serde::Serialize;

use super::explain::{GameRecord, event_action, replay_events};
use super::text::{self, Names};
use crate::agent::{Agent, Config};
use crate::domain::Seat;
use crate::engine::{self, Action, Event, Replay};
use crate::search::SearchMode;
use crate::variants::Variant;

/// What a report says about one decision.
#[derive(Clone, Debug, Default)]
pub struct ReportRequest {
    /// hanab.live turn number of the decision (1 = the first action).
    pub turn: usize,
    /// The better action (`play 2`, `clue 1 red`, `clue Bob 4`).
    pub expected: Option<String>,
    /// The action that is wrong here; defaults to the bot's choice.
    pub rejected: Option<String>,
    /// Rule id the scenario covers (`L1.finesse-position`); `TODO` when empty.
    pub rule: Option<String>,
    /// Free text, kept as comment lines.
    pub comment: String,
    /// Where the game comes from (`hanab.live game 2073098`), for the header.
    pub source: Option<String>,
    /// hanab.live game id, for the file name.
    pub game_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// A file name for `tests/scenarios`.
    pub filename: String,
    pub text: String,
    pub seat: u8,
    pub player: String,
    /// The bot's choice at the decision (`play 2`), as the scenario recomputes it.
    pub chosen: String,
    /// Ways in which the scenario would not fail with the current bot.
    pub warnings: Vec<String>,
}

/// Write the scenario of `request` for `replay`, with the bot configured as `config` (its
/// level and search mode go into the scenario; a time limit is dropped, as scenarios are
/// reproducible).
pub fn scenario(
    replay: &Replay,
    config: &Config,
    request: &ReportRequest,
) -> Result<Report, String> {
    let variant = Variant::by_name(&replay.variant).map_err(|e| e.to_string())?;
    let events = replay_events(&variant, replay)?;
    let record = GameRecord {
        variant: &variant,
        players: &replay.players,
        events: &events,
        world: Some(&replay.deck),
        observer: None,
    };
    let names = Names::new(&replay.players);
    let turns = record.actions().len();
    let turn = request.turn;
    if turn == 0 || turn > turns + 1 {
        return Err(format!(
            "turn {turn} is outside 1..={} of this game",
            turns + 1
        ));
    }
    let state = record.state_before(turn);
    if state.is_over() {
        return Err(format!("the game is over before turn {turn}"));
    }
    let seat = state.current;
    let mut config = config.clone();
    config.search.time_limit = None;
    let mut bot = record.bot_before(&config, seat, turn);
    let chosen = bot.choose_action(&state);
    let reason = match bot.last_plan() {
        Some(plan) => format!("{:?}", plan.reason),
        None => bot
            .last_decision()
            .map_or_else(String::new, |d| format!("{:?}", d.reason)),
    };

    let parse = |spec: &Option<String>| -> Result<Option<Action>, String> {
        let Some(spec) = spec.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let action = text::parse_action(&variant, &names, &state, seat, spec)?;
        engine::check_legal(&variant, &state, action)
            .map_err(|e| format!("{spec}: illegal ({e:?})"))?;
        if let Action::Clue { target, clue } = action
            && engine::touched_cards(&variant, &state, &replay.deck, target, clue).is_empty()
        {
            return Err(format!("{spec}: the clue touches nothing"));
        }
        Ok(Some(action))
    };
    let expected = parse(&request.expected)?;
    let rejected = match parse(&request.rejected)? {
        Some(action) => Some(action),
        None if expected != Some(chosen) => Some(chosen),
        None => None,
    };
    if expected.is_none() && rejected.is_none() {
        return Err("name the better action or the wrong one".to_string());
    }
    if expected.is_some() && expected == rejected {
        return Err("the better and the wrong action are the same".to_string());
    }
    let mut warnings = Vec::new();
    if let Some(action) = expected
        && action == chosen
    {
        warnings.push("the bot already chooses the better action here".to_string());
    }
    if let Some(action) = rejected
        && action != chosen
    {
        warnings.push(format!(
            "the bot chooses {}, not the wrong action",
            text::spec(&variant, &state, chosen)
        ));
    }

    let human = |a: Action| text::action(&variant, &names, &state, Some(&replay.deck), a);
    let mut out = String::new();
    let rule = request
        .rule
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .unwrap_or("TODO");
    out.push_str(&format!("# rule: {rule}\n"));
    let source = request.source.as_deref().unwrap_or("bug report");
    out.push_str(&format!(
        "# {source}, turn {turn}: {} (seat {}) chose {} [{reason}].\n",
        names.seat(seat),
        seat.0,
        human(chosen)
    ));
    if let Some(action) = expected {
        out.push_str(&format!("# Better: {}.\n", human(action)));
    }
    if let Some(action) = rejected.filter(|&a| a != chosen) {
        out.push_str(&format!("# Not: {}.\n", human(action)));
    }
    for line in request.comment.lines() {
        let line = line.trim_end();
        out.push_str(if line.is_empty() { "#\n" } else { "# " });
        if !line.is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    let mut command = format!("raikan explain '{}' --turn {turn}", replay.to_url());
    if config.max_level != Config::default().max_level {
        command.push_str(&format!(" --level {}", config.max_level));
    }
    if config.search.mode != SearchMode::Off {
        command.push_str(&format!(" --search {}", mode_name(config.search.mode)));
    }
    out.push_str(&format!("# {command}\n"));
    out.push_str(&format!("variant {}\n", variant.name));
    out.push_str(&format!("level {}\n", config.max_level));
    if config.search.mode != SearchMode::Off {
        out.push_str(&format!("search {}\n", mode_name(config.search.mode)));
    }

    // The deal, then the draws and the moves before the decision.
    let mut before = record.state_before(1);
    for s in before.seats() {
        let hand: Vec<String> = before
            .hand(s)
            .iter()
            .rev()
            .map(|&c| variant.identity_name(replay.deck.identity(c)))
            .collect();
        out.push_str(&format!("hand {}: {}\n", s.0, hand.join(" ")));
    }
    let mut draws = Vec::new();
    let mut moves = Vec::new();
    for (_, event) in events.iter().filter(|(t, _)| (1..turn).contains(t)) {
        if let Event::Draw { card, .. } = *event {
            draws.push(variant.identity_name(replay.deck.identity(card)));
        } else if let Some(action) = event_action(event) {
            moves.push(seat_spec(
                before.current,
                &text::spec(&variant, &before, action),
            ));
        }
        engine::apply_event(&variant, &mut before, event);
    }
    if !draws.is_empty() {
        out.push_str(&format!("draws: {}\n", draws.join(" ")));
    }
    for line in &moves {
        out.push_str(line);
        out.push('\n');
    }
    if let Some(action) = expected {
        out.push_str(&format!(
            "action {}: {}\n",
            seat.0,
            text::spec(&variant, &state, action)
        ));
    }
    if let Some(action) = rejected {
        out.push_str(&format!(
            "not-action {}: {}\n",
            seat.0,
            text::spec(&variant, &state, action)
        ));
    }

    let filename = match request.game_id {
        Some(id) => format!("report-hanab-{id}-t{turn}.txt"),
        None => format!("report-t{turn}.txt"),
    };
    Ok(Report {
        filename,
        text: out,
        seat: seat.0,
        player: names.seat(seat),
        chosen: text::spec(&variant, &state, chosen),
        warnings,
    })
}

/// `play 2` by seat 1 as the scenario command `play 1 2`.
fn seat_spec(seat: Seat, spec: &str) -> String {
    match spec.split_once(' ') {
        Some((verb, rest)) => format!("{verb} {} {rest}", seat.0),
        None => spec.to_string(),
    }
}

fn mode_name(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Off => "off",
        SearchMode::Fast => "fast",
        SearchMode::Full => "full",
    }
}
