//! The built-in web UI (docs/architecture.md §8.2): an HTTP server over the games the bot
//! knows, showing a table the way hanab.live does plus what the bot believes.
//!
//! A [`Session`] is one game: either a live table (the events one seat observed, so only
//! that seat's view can be recomputed) or a complete replay (every card known, so every
//! seat's view, every decision and bug reports are available). The [`Hub`] holds the
//! sessions; the hanab.live client publishes its table after every batch of events and
//! `raikan serve` adds replays. Everything shown is recomputed from the events on request,
//! as decisions are reproducible from (events, configuration):
//!
//! - `GET /api/board?session&turn&viewer`: the position before a turn, every card with its
//!   true identity, the viewer's note, what the viewer believes its holder knows about it
//!   and (replays) what the holder actually believes; the action log with how the viewer
//!   read each clue;
//! - `GET /api/card?session&turn&viewer&card`: a card's note history (`explain --card`);
//! - `POST /api/explain`: the decision of a turn and its options ranked on common worlds
//!   ([`super::explain`]);
//! - `POST /api/report`: a bug report as a convention scenario ([`super::report`]);
//! - `POST /api/load`: add a replay (hanab.live game id or URL, replay-json URL or JSON).

mod http;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};

pub use http::{Request, Response};

use super::explain::{self, GameRecord, PlanTable, event_action, reading_text, replay_events};
use super::report::{self, ReportRequest};
use super::text::{self, Names};
use crate::agent::Config;
use crate::conventions::{CardNote, Profile, View};
use crate::domain::{CardId, Identity, Seat};
use crate::engine::{self, Action, Event, GameState, Replay, World};
use crate::search::{SearchConfig, SearchMode};
use crate::variants::{Direction, Variant};

const INDEX: &str = include_str!("index.html");

/// Sessions kept at most (the newest first).
const MAX_SESSIONS: usize = 24;

/// The default address of the web UI: local connections only.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7357";

/// One of the bot's own decisions in a live game, as it was taken.
#[derive(Clone, Debug, Serialize)]
pub struct LiveDecision {
    /// hanab.live turn of the decision (1 = the first action).
    pub turn: usize,
    pub action: String,
    /// The one-line summary of the web client's log.
    pub summary: String,
    /// The planner's table, when search is on.
    pub plan: Option<PlanTable>,
}

/// One game as the web UI shows it.
#[derive(Clone, Debug)]
pub struct Session {
    pub key: String,
    pub label: String,
    variant: Variant,
    players: Vec<String>,
    events: Vec<(usize, Event)>,
    /// The whole game with its deck, when every drawn card is known.
    replay: Option<Replay>,
    /// Live: the seat whose observations the events are.
    observer: Option<Seat>,
    /// The seat the bot played (it stays when a finished game becomes a replay).
    bot: Option<Seat>,
    live: bool,
    game_id: Option<u64>,
    /// The hanab.live table name.
    table: Option<String>,
    /// How the game ended or stopped (`normal`, `strikeout`, `terminatedByVote`,
    /// `abandoned`), when the client knows.
    outcome: Option<String>,
    /// Score and whether it is over, after the last event.
    score: u8,
    over: bool,
    decisions: Vec<LiveDecision>,
    version: u64,
}

impl Session {
    /// A complete game.
    pub fn from_replay(
        key: impl Into<String>,
        label: impl Into<String>,
        replay: Replay,
        game_id: Option<u64>,
    ) -> Result<Self, String> {
        let variant = Variant::by_name(&replay.variant).map_err(|e| e.to_string())?;
        let events = replay_events(&variant, &replay)?;
        Ok(Self {
            key: key.into(),
            label: label.into(),
            variant,
            players: replay.players.clone(),
            events,
            replay: Some(replay),
            observer: None,
            bot: None,
            live: false,
            game_id,
            table: None,
            outcome: None,
            score: 0,
            over: false,
            decisions: Vec::new(),
            version: 0,
        }
        .with_final_state())
    }

    /// A game from the events `observer` saw (`None`: a spectator). Once every drawn card
    /// is known (a finished game's history), it becomes a complete replay.
    pub fn observed(
        key: impl Into<String>,
        label: impl Into<String>,
        variant: &Variant,
        players: &[String],
        observer: Option<Seat>,
        events: &[Event],
        live: bool,
    ) -> Self {
        let mut turn = 0;
        let events: Vec<(usize, Event)> = events
            .iter()
            .map(|e| {
                if e.is_action() {
                    turn += 1;
                }
                (turn, *e)
            })
            .collect();
        let replay = complete_replay(variant, players, &events);
        Self {
            key: key.into(),
            label: label.into(),
            variant: variant.clone(),
            players: players.to_vec(),
            observer: if replay.is_some() { None } else { observer },
            bot: observer,
            events,
            replay,
            live,
            game_id: None,
            table: None,
            outcome: None,
            score: 0,
            over: false,
            decisions: Vec::new(),
            version: 0,
        }
        .with_final_state()
    }

    fn with_final_state(mut self) -> Self {
        let state = self.record().state_before(usize::MAX);
        self.score = state.score(&self.variant);
        self.over = state.is_over();
        self
    }

    pub fn with_decisions(mut self, decisions: Vec<LiveDecision>) -> Self {
        self.decisions = decisions;
        self
    }

    pub fn with_game_id(mut self, game_id: Option<u64>) -> Self {
        self.game_id = game_id;
        self
    }

    /// The hanab.live table the game was played at.
    pub fn with_table(mut self, table: Option<String>) -> Self {
        self.table = table;
        self
    }

    /// How the game ended (hanab.live's end condition) or why it stopped (`abandoned`).
    pub fn with_outcome(mut self, outcome: Option<String>) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    /// One line on how the game stands: `playing`, `score 23`, `score 4, strikeout`,
    /// `abandoned at turn 12`.
    pub fn result(&self) -> String {
        if self.live {
            return "playing".to_string();
        }
        let outcome = self
            .outcome
            .as_deref()
            .filter(|o| !o.is_empty() && *o != "normal")
            .map(words);
        let turn = self.events.last().map_or(0, |(t, _)| *t) + 1;
        match (self.over, outcome) {
            (true, None) => format!("score {}", self.score),
            (true, Some(outcome)) => format!("score {}, {outcome}", self.score),
            (false, Some(outcome)) => format!("{outcome} at turn {turn}"),
            (false, None) => format!("unfinished at turn {turn}"),
        }
    }

    /// Whether every card is known (all seats, decisions and bug reports available).
    pub fn is_complete(&self) -> bool {
        self.replay.is_some()
    }

    fn record(&self) -> GameRecord<'_> {
        GameRecord {
            variant: &self.variant,
            players: &self.players,
            events: &self.events,
            world: self.replay.as_ref().map(|r| &r.deck),
            observer: self.observer,
        }
    }
}

/// The replay of `events` when every drawn card's identity is known and the deck
/// reproduces them; the cards nobody drew are filled in catalogue order.
fn complete_replay(
    variant: &Variant,
    players: &[String],
    events: &[(usize, Event)],
) -> Option<Replay> {
    let mut known: Vec<Option<Identity>> = vec![None; variant.num_cards() as usize];
    for (_, event) in events {
        if let Event::Draw {
            card,
            identity: Some(identity),
            ..
        }
        | Event::Play {
            card,
            identity: Some(identity),
            ..
        }
        | Event::Discard {
            card,
            identity: Some(identity),
            ..
        } = *event
        {
            *known.get_mut(card.index())? = Some(identity);
        }
    }
    let drawn = events.iter().all(|(_, e)| match *e {
        Event::Draw { card, .. } => known.get(card.index()).is_some_and(Option::is_some),
        _ => true,
    });
    if !drawn {
        return None;
    }
    let mut rest = variant.deck_order();
    for identity in known.iter().flatten() {
        let pos = rest.iter().position(|i| i == identity)?;
        rest.remove(pos);
    }
    let mut rest = rest.into_iter();
    let deck: Vec<Identity> = known
        .iter()
        .map(|k| k.or_else(|| rest.next()))
        .collect::<Option<_>>()?;
    let mut replay = Replay::new(variant, players.len() as u8, World::new(&deck));
    replay.players = players.to_vec();
    replay.actions = events
        .iter()
        .filter_map(|(_, e)| event_action(e))
        .map(engine::ReplayAction::Action)
        .collect();
    // the simulator must reproduce what was observed
    let same = |a: &[(usize, Event)], b: &[(usize, Event)]| {
        let strip = |v: &[(usize, Event)]| -> Vec<(usize, Event)> {
            v.iter()
                .filter(|(_, e)| !matches!(e, Event::GameOver { .. }))
                .copied()
                .collect()
        };
        strip(a) == strip(b)
    };
    let simulated = replay_events(variant, &replay).ok()?;
    same(&simulated, events).then_some(replay)
}

struct HubState {
    sessions: Vec<Arc<Session>>,
    version: u64,
    uploads: u32,
    /// The bot's hanab.live user name, when it plays.
    bot_name: Option<String>,
}

/// `terminatedByVote` as `terminated by vote`.
fn words(camel: &str) -> String {
    let mut out = String::new();
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The games the web UI can show, and the bot configuration it recomputes them with.
pub struct Hub {
    config: Config,
    state: Mutex<HubState>,
}

impl Hub {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            state: Mutex::new(HubState {
                sessions: Vec::new(),
                version: 0,
                uploads: 0,
                bot_name: None,
            }),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HubState> {
        // a panicking request must not take the UI down for good
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The bot's user name: its seat is marked in every game it played.
    pub fn set_bot_name(&self, name: &str) {
        let mut state = self.lock();
        let name = Some(name.to_string()).filter(|n| !n.is_empty());
        if state.bot_name != name {
            state.bot_name = name;
            state.version += 1;
        }
    }

    /// Add `session`, replacing the one with the same key and any other session of the
    /// same hanab.live game (a table reattended after a reconnect); it becomes the newest.
    pub fn publish(&self, mut session: Session) {
        let mut state = self.lock();
        state.version += 1;
        session.version = state.version;
        if let Some(id) = session.game_id {
            for other in state
                .sessions
                .iter()
                .filter(|s| s.key != session.key && s.game_id == Some(id))
            {
                // keep what only the earlier session knew: the bot's decisions and seat
                if session.decisions.is_empty() {
                    session.decisions = other.decisions.clone();
                }
                session.bot = session.bot.or(other.bot);
                session.table = session.table.take().or_else(|| other.table.clone());
            }
            state
                .sessions
                .retain(|s| s.key == session.key || s.game_id != Some(id));
        }
        state.sessions.retain(|s| s.key != session.key);
        state.sessions.insert(0, Arc::new(session));
        state.sessions.truncate(MAX_SESSIONS);
    }

    /// Add a replay read from `text` (see [`super::load::replay_from_text`]); returns its
    /// session key.
    pub fn load(&self, text: &str) -> Result<String, String> {
        let loaded = super::load::replay_from_text(text)?;
        if let Some(id) = loaded.game_id {
            let state = self.lock();
            if let Some(session) = state.sessions.iter().find(|s| s.game_id == Some(id)) {
                // the bot's own game (with its decisions), or one loaded before
                return Ok(session.key.clone());
            }
        }
        let (key, label) = match loaded.game_id {
            Some(id) => (format!("hanab-{id}"), format!("hanab.live game {id}")),
            None => {
                let n = {
                    let mut state = self.lock();
                    state.uploads += 1;
                    state.uploads
                };
                (format!("replay-{n}"), format!("replay {n}"))
            }
        };
        self.add_replay(&key, &label, loaded.replay, loaded.game_id)?;
        Ok(key)
    }

    /// Add a complete game under `key`.
    pub fn add_replay(
        &self,
        key: &str,
        label: &str,
        replay: Replay,
        game_id: Option<u64>,
    ) -> Result<(), String> {
        let label = format!(
            "{label} ({}, {})",
            replay.variant,
            replay.players.join(", ")
        );
        self.publish(Session::from_replay(key, label, replay, game_id)?);
        Ok(())
    }

    fn session(&self, key: Option<&str>) -> Result<Arc<Session>, (u16, String)> {
        let state = self.lock();
        match key.filter(|k| !k.is_empty()) {
            Some(key) => state.sessions.iter().find(|s| s.key == key),
            None => state.sessions.first(),
        }
        .cloned()
        .ok_or_else(|| (404, "no such game (none loaded yet?)".to_string()))
    }

    /// Answer one HTTP request.
    pub fn handle(&self, request: &Request) -> Response {
        let result = match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/" | "/index.html") => {
                return Response::new(200, "text/html; charset=utf-8", INDEX);
            }
            ("GET", "/api/sessions") => Ok(Response::json(&self.summaries())),
            ("GET", "/api/board") => self.board(request),
            ("GET", "/api/card") => self.card(request),
            ("POST", "/api/explain") => self.explain(request),
            ("POST", "/api/report") => self.report(request),
            ("POST", "/api/load") => self.load_request(request),
            (_, "/" | "/index.html" | "/api/sessions" | "/api/board" | "/api/card")
            | (_, "/api/explain" | "/api/report" | "/api/load") => {
                Err((405, "method not allowed".to_string()))
            }
            _ => Err((404, "not found".to_string())),
        };
        result.unwrap_or_else(|(status, message)| Response::error(status, &message))
    }

    fn summaries(&self) -> serde_json::Value {
        let state = self.lock();
        let sessions: Vec<_> = state
            .sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "key": s.key,
                    "label": s.label,
                    "table": s.table,
                    "game_id": s.game_id,
                    "live": s.live,
                    "complete": s.is_complete(),
                    "result": s.result(),
                    "version": s.version,
                })
            })
            .collect();
        serde_json::json!({
            "version": state.version,
            "bot_name": state.bot_name,
            "sessions": sessions,
        })
    }

    /// The configuration of a recomputation: the bot's, with the requested level and
    /// search mode, on every core and without a time limit (reproducible).
    fn analysis_config(
        &self,
        level: Option<u8>,
        search: Option<&str>,
    ) -> Result<Config, (u16, String)> {
        let mut config = self.config.clone();
        if let Some(level) = level {
            config.max_level = level;
        }
        if let Some(mode) = search.filter(|m| !m.is_empty()) {
            let mode: SearchMode = mode.parse().map_err(|e: String| (400, e))?;
            if mode != config.search.mode {
                config.search = SearchConfig::new(mode);
            }
        }
        config.search.threads = cores();
        config.search.time_limit = None;
        Ok(config)
    }

    fn board(&self, request: &Request) -> Result<Response, (u16, String)> {
        let session = self.session(request.param("session"))?;
        let turn = parse_param::<usize>(request, "turn")?;
        let viewer = parse_param::<u8>(request, "viewer")?.map(Seat);
        let level = parse_param::<u8>(request, "level")?;
        let config = self.analysis_config(level, None)?;
        let board = board(&session, &config, turn, viewer).map_err(|e| (422, e))?;
        let bot_name = self.lock().bot_name.clone();
        Ok(Response::json(&serde_json::json!({
            "session": session_info(&session, &config, bot_name),
            "board": board,
        })))
    }

    fn card(&self, request: &Request) -> Result<Response, (u16, String)> {
        let session = self.session(request.param("session"))?;
        let record = session.record();
        let turn = parse_param::<usize>(request, "turn")?.unwrap_or(record.actions().len() + 1);
        let viewer = parse_param::<u8>(request, "viewer")?
            .map(Seat)
            .or(session.observer)
            .ok_or((400, "viewer missing".to_string()))?;
        let card = parse_param::<u8>(request, "card")?.ok_or((400, "card missing".to_string()))?;
        let level = parse_param::<u8>(request, "level")?;
        let config = self.analysis_config(level, None)?;
        let history = explain::card_history(&record, &config, viewer, CardId(card), turn)
            .map_err(|e| (422, e))?;
        Ok(Response::json(&history))
    }

    fn explain(&self, request: &Request) -> Result<Response, (u16, String)> {
        #[derive(Deserialize)]
        struct Body {
            session: Option<String>,
            turn: usize,
            #[serde(default)]
            candidates: Vec<String>,
            #[serde(default)]
            all: bool,
            search: Option<String>,
            level: Option<u8>,
            world: Option<usize>,
        }
        let body: Body = parse_body(request)?;
        let session = self.session(body.session.as_deref())?;
        let config = self.analysis_config(body.level, body.search.as_deref())?;
        let mut explain_request = explain::Request::new(body.turn);
        explain_request.candidates = body.candidates;
        explain_request.all = body.all;
        explain_request.world = body.world;
        explain_request.compare.threads = cores();
        let explanation = explain::explain_record(&session.record(), &config, &explain_request)
            .map_err(|e| (422, e))?;
        Ok(Response::json(&explanation))
    }

    fn report(&self, request: &Request) -> Result<Response, (u16, String)> {
        #[derive(Deserialize)]
        struct Body {
            session: Option<String>,
            turn: usize,
            expected: Option<String>,
            rejected: Option<String>,
            rule: Option<String>,
            #[serde(default)]
            comment: String,
            search: Option<String>,
            level: Option<u8>,
        }
        let body: Body = parse_body(request)?;
        let session = self.session(body.session.as_deref())?;
        let replay = session.replay.as_ref().ok_or((
            422,
            "bug reports need every card: available once the game is over".to_string(),
        ))?;
        let config = self.analysis_config(body.level, body.search.as_deref())?;
        let report = report::scenario(
            replay,
            &config,
            &ReportRequest {
                turn: body.turn,
                expected: body.expected,
                rejected: body.rejected,
                rule: body.rule,
                comment: body.comment,
                source: Some(match session.game_id {
                    Some(id) => format!("hanab.live game {id}"),
                    None => session.label.clone(),
                }),
                game_id: session.game_id,
            },
        )
        .map_err(|e| (422, e))?;
        Ok(Response::json(&report))
    }

    fn load_request(&self, request: &Request) -> Result<Response, (u16, String)> {
        #[derive(Deserialize)]
        struct Body {
            replay: String,
        }
        let body: Body = parse_body(request)?;
        let key = self.load(&body.replay).map_err(|e| (422, e))?;
        Ok(Response::json(&serde_json::json!({ "key": key })))
    }
}

/// Serve the web UI of `hub` on `addr` (or one of the next ports when it is taken).
pub fn serve(hub: Arc<Hub>, addr: &str) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    http::serve(addr, Arc::new(move |request: &Request| hub.handle(request)))
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn parse_param<T: std::str::FromStr>(
    request: &Request,
    name: &str,
) -> Result<Option<T>, (u16, String)> {
    match request.param(name).filter(|v| !v.is_empty()) {
        Some(value) => value
            .parse()
            .map(Some)
            .map_err(|_| (400, format!("invalid {name}: {value}"))),
        None => Ok(None),
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(request: &Request) -> Result<T, (u16, String)> {
    serde_json::from_slice(&request.body).map_err(|e| (400, format!("invalid request: {e}")))
}

#[derive(Serialize)]
struct SessionInfo {
    key: String,
    label: String,
    live: bool,
    complete: bool,
    observer: Option<u8>,
    /// The bot's seat: the one it played, or the player with its user name.
    bot: Option<u8>,
    bot_name: Option<String>,
    game_id: Option<u64>,
    table: Option<String>,
    result: String,
    variant: VariantInfo,
    players: Vec<String>,
    /// The last turn (the position after the last action).
    turns: usize,
    replay_url: Option<String>,
    level: u8,
    search: &'static str,
    version: u64,
    /// Turns with a recorded live decision.
    decisions: Vec<usize>,
}

#[derive(Serialize)]
struct VariantInfo {
    name: String,
    suits: Vec<SuitView>,
    identities: Vec<IdentityView>,
    ranks: Vec<u8>,
}

#[derive(Serialize)]
struct SuitView {
    name: String,
    abbreviation: String,
}

#[derive(Serialize)]
struct IdentityView {
    suit: u8,
    rank: u8,
    name: String,
}

fn mode_name(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Off => "off",
        SearchMode::Fast => "fast",
        SearchMode::Full => "full",
    }
}

fn session_info(session: &Session, config: &Config, bot_name: Option<String>) -> SessionInfo {
    let variant = &session.variant;
    let bot = session.bot.map(|s| s.0).or_else(|| {
        let name = bot_name.as_deref()?;
        let seat = session
            .players
            .iter()
            .position(|p| p.eq_ignore_ascii_case(name))?;
        Some(seat as u8)
    });
    let bot_name = bot
        .and_then(|s| session.players.get(s as usize).cloned())
        .or(bot_name);
    SessionInfo {
        key: session.key.clone(),
        label: session.label.clone(),
        live: session.live,
        complete: session.is_complete(),
        observer: session.observer.map(|s| s.0),
        bot,
        bot_name,
        game_id: session.game_id,
        table: session.table.clone(),
        result: session.result(),
        variant: VariantInfo {
            name: variant.name.clone(),
            suits: variant
                .suits
                .iter()
                .map(|s| SuitView {
                    name: s.name.clone(),
                    abbreviation: s.abbreviation.clone(),
                })
                .collect(),
            identities: variant
                .all_identities()
                .iter()
                .map(|i| IdentityView {
                    suit: variant.suit_of(i) as u8,
                    rank: variant.rank_of(i),
                    name: variant.identity_name(i),
                })
                .collect(),
            ranks: variant.ranks().collect(),
        },
        players: session.players.clone(),
        turns: session.record().actions().len() + 1,
        replay_url: session.replay.as_ref().map(Replay::to_url),
        level: config.max_level,
        search: mode_name(config.search.mode),
        version: session.version,
        decisions: session.decisions.iter().map(|d| d.turn).collect(),
    }
}

#[derive(Serialize)]
struct Board {
    turn: usize,
    viewer: u8,
    to_move: u8,
    over: bool,
    score: u8,
    max_score: u8,
    clues: u8,
    strikes: u8,
    deck: u8,
    pace: i16,
    stacks: Vec<StackView>,
    /// `(identity, count)`.
    discards: Vec<(u8, u8)>,
    hands: Vec<HandBoard>,
    log: Vec<LogEntry>,
    /// The legal actions of the seat to move (when its view is known).
    legal: Vec<ActionOption>,
    /// The action taken at this turn, if the game went on.
    played: Option<ActionOption>,
    /// The bot's live decision at this turn.
    decision: Option<LiveDecision>,
}

#[derive(Serialize)]
struct StackView {
    suit: u8,
    /// Rank of the top card (0 = empty, 7 = START).
    top: u8,
    direction: &'static str,
}

#[derive(Serialize)]
struct HandBoard {
    seat: u8,
    player: String,
    cards: Vec<CardBoard>,
}

#[derive(Serialize)]
struct CardBoard {
    card: u8,
    /// 1 = newest.
    slot: usize,
    /// The true identity, when known.
    identity: Option<u8>,
    /// Whether the viewer sees the card (not in its own hand).
    visible: bool,
    /// The card is the holder's chop in the viewer's notes.
    chop: bool,
    /// Clues that touched it, oldest first.
    clues: Vec<String>,
    /// The viewer's note.
    note: NoteView,
    /// What the viewer believes the holder knows (its kept view of the holder).
    holder: Option<NoteView>,
    /// What the holder believes, from its own view (complete games only).
    actual: Option<NoteView>,
}

#[derive(Serialize)]
struct NoteView {
    possible: Vec<u8>,
    literal: Vec<u8>,
    flags: Vec<String>,
    text: String,
}

#[derive(Serialize)]
struct LogEntry {
    turn: usize,
    seat: u8,
    text: String,
    /// How the viewer read a clue (rule, focus and readings), for clues before the turn.
    reading: Option<String>,
}

#[derive(Serialize)]
struct ActionOption {
    /// As `text::parse_action` reads it.
    spec: String,
    text: String,
}

fn note_view(variant: &Variant, note: &CardNote) -> NoteView {
    let mut flags: Vec<String> = [
        (note.clued, "clued"),
        (note.play, "play"),
        (note.trash, "trash"),
        (note.chop_moved, "chop-moved"),
        (note.locked, "locked"),
        (note.fixed, "fixed"),
        (note.stopped, "stopped"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_string())
    .collect();
    if note.promised.is_some() {
        flags.push("promised".to_string());
    }
    if note.delayed > 0 {
        flags.push(format!("after {}", note.delayed));
    }
    NoteView {
        possible: note.belief.soft.iter().map(|i| i.0).collect(),
        literal: note.belief.hard.iter().map(|i| i.0).collect(),
        flags,
        text: text::note(variant, note),
    }
}

fn log_text(variant: &Variant, names: &Names, state: &GameState, event: &Event) -> String {
    let slot = |card: CardId| {
        text::slot(state, card).map_or_else(|| format!("#{}", card.0), |(_, s)| format!("slot {s}"))
    };
    let id = |identity: Option<Identity>| {
        identity.map_or_else(|| "?".to_string(), |i| variant.identity_name(i))
    };
    match *event {
        Event::Play {
            seat,
            card,
            identity,
            success,
        } => format!(
            "{} {} {} from {}",
            names.seat(seat),
            if success { "plays" } else { "misplays" },
            id(identity),
            slot(card)
        ),
        Event::Discard {
            seat,
            card,
            identity,
        } => format!(
            "{} discards {} from {}",
            names.seat(seat),
            id(identity),
            slot(card)
        ),
        Event::Clue {
            giver,
            target,
            clue,
            touched,
        } => {
            let hand = state.hand(target);
            let slots: Vec<String> = (1..=hand.len())
                .filter(|&s| touched.contains(hand[hand.len() - s]))
                .map(|s| s.to_string())
                .collect();
            format!(
                "{} clues {} {} (slot{} {})",
                names.seat(giver),
                names.seat(target),
                variant.clue_name(clue),
                if slots.len() == 1 { "" } else { "s" },
                slots.join(",")
            )
        }
        _ => text::event(variant, names, event),
    }
}

/// The position before `turn` (default: the latest) as `viewer` (default: the seat to
/// move; the observer in a live game) sees it.
fn board(
    session: &Session,
    config: &Config,
    turn: Option<usize>,
    viewer: Option<Seat>,
) -> Result<Board, String> {
    let record = session.record();
    let variant = &session.variant;
    let names = Names::new(&session.players);
    let num_seats = record.num_seats();
    let actions = record.actions();
    let last = actions.len() + 1;
    let turn = turn.unwrap_or(last).clamp(1, last);
    let state = record.state_before(turn);
    let viewer = match session.observer {
        Some(observer) => observer,
        None => viewer.unwrap_or(state.current),
    };
    if viewer.0 >= num_seats {
        return Err(format!("there is no seat {}", viewer.0));
    }
    record.check_viewer(viewer)?;

    // The viewer's view (with its interpretations) and, in complete games, every seat's.
    let profile = Arc::new(
        Profile::new(config.max_level, variant, config.require_complete)
            .map_err(|e| format!("{e:?}"))?,
    );
    let shared = Arc::new(variant.clone());
    let mut views: Vec<Option<View>> = (0..num_seats)
        .map(|s| {
            (s == viewer.0 || session.is_complete()).then(|| {
                View::with_seat_views(
                    Arc::clone(&shared),
                    Arc::clone(&profile),
                    num_seats,
                    Seat(s),
                )
            })
        })
        .collect();
    if let Some(view) = &mut views[viewer.index()] {
        view.enable_trace();
    }
    let mut clue_marks: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    let mut log = Vec::new();
    let mut before = GameState::new(variant, num_seats);
    for (t, event) in record.events {
        let past = *t < turn;
        let mut reading = None;
        if past {
            for (s, view) in views.iter_mut().enumerate() {
                if let Some(view) = view {
                    view.observe(&event.observed_by(Seat(s as u8)));
                }
            }
            let traces = views[viewer.index()]
                .as_mut()
                .map(|v| v.trace.take())
                .unwrap_or_default();
            if let Event::Clue { clue, touched, .. } = *event {
                reading = traces.last().map(|t| reading_text(&before, t));
                for card in touched.iter() {
                    clue_marks
                        .entry(card.0)
                        .or_default()
                        .push(variant.clue_name(clue));
                }
            }
        }
        if event.is_action() {
            log.push(LogEntry {
                turn: *t,
                seat: before.current.0,
                text: log_text(variant, &names, &before, event),
                reading,
            });
        }
        engine::apply_event(variant, &mut before, event);
    }
    let view = views[viewer.index()].as_ref().expect("the viewer's view");
    let world = record.world;

    let hands = state
        .seats()
        .map(|holder| {
            let hand = state.hand(holder);
            let chop = view.chop(holder);
            HandBoard {
                seat: holder.0,
                player: names.seat(holder),
                cards: hand
                    .iter()
                    .rev()
                    .enumerate()
                    .map(|(i, &c)| {
                        let other = holder != viewer;
                        CardBoard {
                            card: c.0,
                            slot: i + 1,
                            identity: world
                                .map(|w| w.identity(c))
                                .or_else(|| view.identity(c))
                                .map(|i| i.0),
                            visible: other,
                            chop: chop == Some(c),
                            clues: clue_marks.get(&c.0).cloned().unwrap_or_default(),
                            note: note_view(variant, view.note(c)),
                            holder: other
                                .then(|| view.seat_view(holder))
                                .flatten()
                                .map(|v| note_view(variant, v.note(c))),
                            actual: other
                                .then(|| views[holder.index()].as_ref())
                                .flatten()
                                .map(|v| note_view(variant, v.note(c))),
                        }
                    })
                    .collect(),
            }
        })
        .collect();

    // Legal actions of the seat to move, where its view is known.
    let mover = state.current;
    let legal = match (&views[mover.index()], state.is_over()) {
        (Some(mover_view), false) => {
            let sight = explain::sight_world(mover_view);
            let world_for_rules = world.unwrap_or(&sight);
            engine::legal_commands(variant, &state, world_for_rules)
                .into_iter()
                .map(|a| action_option(variant, &names, &state, world, &sight, a))
                .collect()
        }
        _ => Vec::new(),
    };
    let played = actions.get(turn - 1).map(|&a| {
        let sight = explain::sight_world(view);
        action_option(variant, &names, &state, world, &sight, a)
    });

    let max_score = variant.max_reachable_score(&state.discarded);
    let score = state.score(variant);
    Ok(Board {
        turn,
        viewer: viewer.0,
        to_move: mover.0,
        over: state.is_over(),
        score,
        max_score,
        clues: state.clue_tokens(),
        strikes: state.strikes,
        deck: state.cards_left(),
        pace: score as i16 + state.cards_left() as i16 + num_seats as i16 - max_score as i16,
        stacks: (0..variant.num_suits())
            .map(|s| {
                let stack = state.stacks[s];
                StackView {
                    suit: s as u8,
                    top: stack.top,
                    direction: match stack.direction {
                        Direction::Up => "up",
                        Direction::Down => "down",
                        Direction::Undecided => "undecided",
                    },
                }
            })
            .collect(),
        discards: variant
            .all_identities()
            .iter()
            .filter(|i| state.discarded[i.index()] > 0)
            .map(|i| (i.0, state.discarded[i.index()]))
            .collect(),
        hands,
        log,
        legal,
        played,
        decision: session.decisions.iter().find(|d| d.turn == turn).cloned(),
    })
}

/// An action with its spec; clues name their slots (the target's hand is in sight).
fn action_option(
    variant: &Variant,
    names: &Names,
    state: &GameState,
    world: Option<&World>,
    sight: &World,
    action: Action,
) -> ActionOption {
    let shown = match action {
        Action::Clue { .. } => Some(world.unwrap_or(sight)),
        _ => world,
    };
    ActionOption {
        spec: text::spec(variant, state, action),
        text: text::action(variant, names, state, shown, action),
    }
}
