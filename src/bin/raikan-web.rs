use std::collections::{BTreeMap, VecDeque};

use raikan::agent::{Agent, BotAgent, Config};
use raikan::domain::{CardId, CardSet, Seat};
use raikan::engine::{self, Action, EndReason, Event, GameState};
use raikan::variants::{ClueValue, Variant};
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::Duration;
use tungstenite::{
    ClientRequestBuilder, client::connect, http::Uri, protocol::Message, protocol::WebSocket,
    stream::MaybeTlsStream,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("raikan-web: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cookie = match std::env::var("HANABI_SID") {
        Ok(cookie) if !cookie.trim().is_empty() => cookie,
        Ok(_) => return Err("HANABI_SID must contain a session cookie".into()),
        Err(std::env::VarError::NotPresent) => {
            let username = std::env::var("HANABI_USERNAME")
                .map_err(|_| "HANABI_USERNAME environment variable is required")?;
            let password = std::env::var("HANABI_PASSWORD")
                .map_err(|_| "HANABI_PASSWORD environment variable is required")?;
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            login(&client, "https://hanab.live/login", &username, &password)?
        }
        Err(_) => return Err("HANABI_SID must be valid UTF-8".into()),
    };

    let mut client: HanabClient = HanabClient {
        client: connect(websocket_request(&cookie))
            .map_err(|error| format!("WebSocket connection failed: {error}"))?
            .0,
        user_id: 0,
        username: "".to_string(),
        table_id: 0,
        user_states: BTreeMap::new(),
        table_states: BTreeMap::new(),
        game: None,
        game_actions: Vec::with_capacity(256),
        replay_segment: None,
        next_request_id: 1,
        pending_requests: BTreeMap::new(),
    };
    client.run();
    Ok(())
}

fn websocket_request(cookie: &str) -> ClientRequestBuilder {
    // The current server uses client=bot on BOTH /login and /ws. version is
    // reserved for browser releases; the old version=bot is rejected with 401.
    let uri: Uri = "wss://hanab.live/ws?client=bot"
        .parse()
        .expect("valid WebSocket URL");
    ClientRequestBuilder::new(uri).with_header("cookie", cookie)
}

fn login(
    client: &reqwest::blocking::Client,
    url: &str,
    username: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if username.is_empty() || password.is_empty() {
        return Err("HANABI_USERNAME and HANABI_PASSWORD must not be empty".into());
    }
    let response = client
        .post(url)
        .form(&[
            ("username", username),
            ("password", password),
            ("client", "bot"),
            ("createAccount", "false"),
        ])
        .send()
        .map_err(|error| format!("Login request failed: {error}"))?;
    let status = response.status();
    if status != reqwest::StatusCode::OK {
        let body = response.text()?;
        let detail = serde_json::from_str::<ServerNotice>(&body)
            .map(|notice| notice.message)
            .unwrap_or(body);
        // A server diagnostic should not repeat a submitted password.
        let detail = detail.replace(password, "[redacted]");
        let detail = if detail.trim().is_empty() {
            "The server returned no explanation."
        } else {
            detail.trim()
        };
        return Err(format!("Login failed (HTTP {status}): {detail}").into());
    }
    // Set-Cookie attributes (Path, HttpOnly, SameSite, etc.) do not belong in
    // a Cookie request header. Only the session pair is needed for /ws.
    response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .map(str::trim)
        .find(|pair| {
            pair.strip_prefix("hanabi.sid=")
                .is_some_and(|value| !value.is_empty())
        })
        .map(str::to_owned)
        .ok_or_else(|| "Login succeeded but no hanabi.sid session cookie was returned".into())
}

#[derive(Deserialize)]
struct ServerNotice {
    message: String,
}

// Each WebSocket frame is a JSON object. Replies identify the request, not
// its command or table; retain that context until the reply arrives.
#[derive(Deserialize)]
#[serde(untagged)]
enum ServerFrame {
    Event {
        event: String,
        data: serde_json::Value,
    },
    Reply {
        reply: u64,
        data: Option<serde_json::Value>,
        error: Option<ServerNotice>,
    },
}

struct PendingRequest {
    command: String,
    table_id: Option<usize>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WelcomeMessage {
    #[serde(alias = "userID")]
    user_id: usize,
    username: String,
    /// Ongoing games we are a player in (current protocol).
    #[serde(default)]
    playing_at_tables: Vec<usize>,
    /// The current protocol folds the lobby's user and table lists into
    /// `welcome` instead of sending `userList` and `tableList` events.
    #[serde(default)]
    users: Option<Vec<UserState>>,
    #[serde(default)]
    tables: Option<Vec<TableState>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct UserState {
    user_id: usize,
    username: String,
    activity: UserActivity,
    inactive: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum UserActivity {
    Lobby,
    Pregame {
        table_id: usize,
    },
    Playing {
        table_id: usize,
    },
    Spectating {
        table_id: usize,
    },
    SharedReplay {
        table_id: usize,
    },
    #[serde(alias = "replay")]
    SoloReplay,
}

#[derive(Deserialize)]
struct UserListMessage {
    users: Vec<UserState>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UserInactiveMessage {
    user_id: usize,
    inactive: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UserLeftMessage {
    user_id: usize,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TableIdMessage {
    table_id: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TableJoinedMessage {
    table_id: usize,
    // Older notifications did not include a stage.
    stage: Option<TableStage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum TableStage {
    Pregame,
    JustStarted,
    Playing,
    Replay,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SpectateMessage {
    table_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadowing_player_index: Option<u8>,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TableJoinMessage {
    table_id: usize,
    password: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TableState {
    table_id: usize,
    #[serde(deserialize_with = "deserialize_table_phase")]
    phase: TablePhase,
    player_names: Vec<String>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TablePhase {
    Pregame,
    Playing,
    SharedReplay,
}

fn deserialize_table_phase<'de, D>(deserializer: D) -> Result<TablePhase, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Payload {
        // Shared replays may also carry a gameId, which the bot does not need.
        Current {
            #[serde(rename = "type")]
            phase: TablePhase,
        },
        Legacy(TablePhase),
    }

    Ok(match Payload::deserialize(deserializer)? {
        Payload::Current { phase } | Payload::Legacy(phase) => phase,
    })
}

impl TableState {
    fn has_active_player(&self, username: &str) -> bool {
        matches!(self.phase, TablePhase::Pregame | TablePhase::Playing)
            && self.player_names.iter().any(|name| name == username)
    }
}

#[derive(Deserialize)]
struct TableListMessage {
    tables: Vec<TableState>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ChatAuthor {
    User {
        username: String,
    },
    /// The Discord server, whose messages the lobby mirrors.
    Discord,
    Server,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ChatKind {
    /// A message to everyone in the lobby or at the table.
    Public { author: ChatAuthor },
    /// A message to one user.
    Private {
        author: ChatAuthor,
        recipient: String,
    },
    /// Server-provided table settings; the bot ignores the payload.
    Suggestion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    text: String,
    table_id: Option<usize>,
    kind: ChatKind,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InitMessage {
    // Legacy gameInit events included the table; current replies get it from
    // the matching gameInitLoad request instead.
    #[serde(default)]
    table_id: u64,
    player_names: Vec<String>,
    mode: ViewMode,
    options: GameOptions,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ViewMode {
    #[serde(rename = "type")]
    kind: String,
    our_player_index: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameOptions {
    variant_id: usize,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameStateMessage {
    table_id: u64,
    events: Vec<GameAction>,
    phase: Option<GamePhase>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameActionMessage {
    table_id: u64,
    #[serde(flatten)]
    batch: GameEventBatch,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum GameEventBatch {
    Current { events: Vec<GameAction> },
    Legacy { event: GameAction },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum GamePhase {
    Playing {
        pause: PauseState,
    },
    Replay {
        shared_replay: Option<SharedReplayState>,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct SharedReplayState {
    turn: u8,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PauseState {
    pausing_player_index: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PauseMessage {
    table_id: u64,
    #[serde(flatten)]
    pause: PauseState,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameFinishedMessage {
    table_id: u64,
    // The current server sends the entire unhidden history on completion.
    // Older servers sent only replay metadata.
    events: Option<Vec<GameAction>>,
    shared_replay: SharedReplayState,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClueMessage {
    // The server uses variant-specific fields; accept `value` in older histories.
    Color {
        #[serde(alias = "value")]
        color: u8,
    },
    Rank {
        #[serde(alias = "value")]
        rank: u8,
    },
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "camelCase")]
struct CardIdentity {
    suit_index: u8,
    rank: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum GameAction {
    Draw {
        player_index: u8,
        order: u8,
        identity: Option<CardIdentity>,
    },
    Clue {
        player_index: u8,
        #[serde(rename = "receiverIndex", alias = "target")]
        target: u8,
        orders: Vec<u8>,
        clue: ClueMessage,
    },
    Turn {
        turn: u8,
        current_player_index: Option<u8>,
    },
    Play {
        order: u8,
        player_index: u8,
        identity: CardIdentity,
    },
    Discard {
        #[serde(default)]
        failed: bool,
        order: u8,
        player_index: u8,
        identity: CardIdentity,
    },
    Misplay {
        order: u8,
        player_index: u8,
        identity: CardIdentity,
    },
    GameOver {
        end_condition: String,
    },
    CardIdentity,
    PlayerTimes,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ActionMessage {
    table_id: u64,
    action: PlayerAction,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
enum PlayerAction {
    Play {
        order: u8,
    },
    Discard {
        order: u8,
    },
    Clue {
        #[serde(rename = "receiverIndex", alias = "target")]
        target: u8,
        clue: ClueMessage,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ReplaySegmentMessage {
    table_id: u64,
    action: ReplayAction,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ReplayAction {
    Turn {
        turn: u8,
    },
    #[serde(other)]
    Other,
}

struct HanabClient {
    client: WebSocket<MaybeTlsStream<TcpStream>>,
    user_id: usize,
    username: String,
    user_states: std::collections::BTreeMap<usize, UserState>,
    table_states: std::collections::BTreeMap<usize, TableState>,
    table_id: usize,
    game: Option<HanabGame>,
    game_actions: Vec<GameAction>,
    replay_segment: Option<u8>,
    next_request_id: u64,
    pending_requests: BTreeMap<u64, PendingRequest>,
}

impl HanabClient {
    fn run(&mut self) {
        loop {
            let message = self.client.read();
            match message {
                Ok(message) => match message {
                    Message::Ping(id) => match self.client.send(Message::Pong(id.clone())) {
                        Ok(_) => {}
                        Err(error) => {
                            println!("Failed to send pong websocket message: {}", error)
                        }
                    },
                    Message::Text(text) => {
                        if let Err(error) = self.process_frame(&text) {
                            eprintln!("Failed to process WebSocket frame: {error}");
                        }
                    }
                    _ => {
                        println!(
                            "Received unknown websocket message (implemented are text and ping): {message:?}"
                        );
                    }
                },
                Err(error) => {
                    println!("Error recving message {}", error);
                    return;
                }
            }
        }
    }

    fn process_frame(&mut self, text: &str) -> Result<(), serde_json::Error> {
        match serde_json::from_str::<ServerFrame>(text)? {
            ServerFrame::Event { event, data } => self.process_message(&event, &data.to_string()),
            ServerFrame::Reply { reply, data, error } => {
                let Some(request) = self.pending_requests.remove(&reply) else {
                    // A reply can outlive the table it initialized.
                    return Ok(());
                };
                if let Some(error) = error {
                    eprintln!("Request {} failed: {}", request.command, error.message);
                    if request.table_id == Some(self.table_id)
                        && self.game.is_none()
                        && matches!(
                            request.command.as_str(),
                            "tableJoin" | "tableReattend" | "tableSpectate"
                        )
                    {
                        self.table_id = 0;
                    }
                    return Ok(());
                }
                if request.command == "gameInitLoad" && request.table_id == Some(self.table_id) {
                    let mut init: InitMessage =
                        serde_json::from_value(data.unwrap_or(serde_json::Value::Null))?;
                    init.table_id = self.table_id as u64;
                    self.initialize_game(init)?;
                }
                Ok(())
            }
        }
    }

    fn process_message(&mut self, command: &str, json: &str) -> Result<(), serde_json::Error> {
        match command {
            "welcome" => self.on_welcome(json),
            "error" => self.on_error(json),
            "warning" => self.on_warning(json),

            "chat" => self.on_chat(json),
            "chatList" => Ok(()),
            "chatTyping" => Ok(()),
            "lobbyStatus" | "synced" | "pregame" => Ok(()),

            "userList" => self.on_user_list(json),
            "user" => self.on_user(json),
            "userInactive" => self.on_user_inactive(json),
            "userRemoved" | "userLeft" => self.on_user_left(json),

            "tableList" => self.on_table_list(json),
            "table" => self.on_table(json),
            "tableRemoved" | "tableGone" => self.on_table_gone(json),

            "tableJoined" => self.on_joined(json),
            "tableLeft" | "tableBooted" | "tableBoot" => self.on_left(json),
            "tableStarted" => self.on_table_start(json),
            "gameInit" | "init" => self.on_init(json),
            "gameState" | "gameEventList" => self.on_game_state(json),
            "gameEvent" => self.on_action(json),
            "gameFinished" => self.on_game_finished(json),
            "pause" => self.on_pause(json),

            "replayAction" => self.on_replay_segment(json),
            "clock"
            | "connections"
            | "connected"
            | "noteList"
            | "noteListPlayer"
            | "spectators"
            | "cardIdentities"
            | "hypothetical"
            | "note"
            | "voteChanged"
            | "voteChange"
            | "votes"
            | "tableOwner"
            | "tableRestartDecliners"
            | "tableRestartDeclined"
            | "tableSuggestion"
            | "tableSettingsSuggestion"
            | "chatDeleted"
            | "gameHistoryAdded"
            | "friends"
            | "blocked"
            | "idleWarning"
            | "idleReset"
            | "serverShutdown" => Ok(()),

            _ => {
                println!("Received unhandled message: {command} / {json}");
                Ok(())
            }
        }
    }

    fn on_welcome(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let welcome: WelcomeMessage = serde_json::from_str(json)?;
        println!("Welcome {:?}", welcome);
        self.user_id = welcome.user_id;
        self.username = welcome.username;
        if let Some(users) = welcome.users {
            self.user_states = users.into_iter().map(|user| (user.user_id, user)).collect();
        }
        if let Some(tables) = welcome.tables {
            self.adopt_tables(tables, &welcome.playing_at_tables)?;
        } else if self.table_id == 0 {
            if let Some(&table_id) = welcome.playing_at_tables.first() {
                self.table_id = table_id;
                self.send(
                    "tableReattend",
                    &serde_json::to_string(&TableIdMessage { table_id })?,
                );
            }
        }
        Ok(())
    }

    fn on_error(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let notice: ServerNotice = serde_json::from_str(json)?;
        eprintln!("Server error: {}", notice.message);
        Ok(())
    }

    fn on_warning(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let notice: ServerNotice = serde_json::from_str(json)?;
        eprintln!("Server warning: {}", notice.message);
        Ok(())
    }

    fn on_user_list(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let users: UserListMessage = serde_json::from_str(json)?;
        self.user_states = users
            .users
            .into_iter()
            .map(|user| (user.user_id, user))
            .collect();
        Ok(())
    }

    fn on_user(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let user: UserState = serde_json::from_str(json)?;
        self.user_states.insert(user.user_id, user);
        Ok(())
    }

    fn on_user_inactive(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let user: UserInactiveMessage = serde_json::from_str(json)?;
        self.user_states
            .entry(user.user_id)
            .and_modify(|e| e.inactive = user.inactive);
        Ok(())
    }

    fn on_user_left(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let user: UserLeftMessage = serde_json::from_str(json)?;
        self.user_states.remove(&user.user_id);
        Ok(())
    }

    fn on_table_list(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let tables: TableListMessage = serde_json::from_str(json)?;
        self.adopt_tables(tables.tables, &[])
    }

    /// Replace the known tables and, when not seated anywhere yet, return to
    /// the table we belong to: a game the server says we are playing, then
    /// any table listing us as an active player, then an old shared replay.
    fn adopt_tables(
        &mut self,
        tables: Vec<TableState>,
        playing_at_tables: &[usize],
    ) -> Result<(), serde_json::Error> {
        let selected = playing_at_tables
            .iter()
            .find_map(|id| tables.iter().find(|table| table.table_id == *id))
            .or_else(|| {
                tables
                    .iter()
                    .find(|table| table.has_active_player(&self.username))
            })
            .or_else(|| {
                tables.iter().find(|table| {
                    table.phase == TablePhase::SharedReplay
                        && table.player_names.contains(&self.username)
                })
            });
        if self.table_id == 0 {
            if let Some(table) = selected {
                self.table_id = table.table_id;
                let command = if playing_at_tables.contains(&table.table_id)
                    || table.has_active_player(&self.username)
                {
                    "tableReattend"
                } else {
                    "tableSpectate"
                };
                self.send(
                    command,
                    &serde_json::to_string(&TableIdMessage {
                        table_id: table.table_id,
                    })?,
                );
            }
        }
        self.table_states.clear();
        for table in tables {
            self.table_states.insert(table.table_id, table);
        }
        Ok(())
    }

    fn on_table(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableState = serde_json::from_str(json)?;
        self.table_states.insert(table.table_id, table);
        Ok(())
    }

    fn on_table_gone(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.table_states.remove(&table.table_id);
        Ok(())
    }

    fn on_chat(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let chat: ChatMessage = serde_json::from_str(json)?;
        match chat.kind {
            ChatKind::Private {
                author: ChatAuthor::User { username },
                recipient,
            } if recipient == self.username => {
                if chat.text != "/join" {
                    eprintln!("Unknown message content {}", chat.text);
                    return Ok(());
                }

                let Some(user) = self
                    .user_states
                    .values()
                    .find(|user| user.username == username)
                else {
                    return Ok(());
                };
                println!("Join request by {}", user.username);
                match user.activity {
                    UserActivity::Pregame { table_id } => {
                        self.table_id = table_id;
                        let table_ref = TableJoinMessage {
                            table_id,
                            password: "secret".to_string(),
                        };
                        self.send("tableJoin", &serde_json::to_string(&table_ref)?);
                    }
                    UserActivity::Playing { table_id }
                        if self
                            .table_states
                            .get(&table_id)
                            .is_some_and(|table| table.has_active_player(&self.username)) =>
                    {
                        self.table_id = table_id;
                        self.send(
                            "tableReattend",
                            &serde_json::to_string(&TableIdMessage { table_id })?,
                        );
                    }
                    UserActivity::Playing { table_id }
                    | UserActivity::Spectating { table_id }
                    | UserActivity::SharedReplay { table_id } => {
                        self.table_id = table_id;
                        let table_ref = SpectateMessage {
                            table_id,
                            shadowing_player_index: None,
                        };
                        self.send("tableSpectate", &serde_json::to_string(&table_ref)?);
                    }
                    UserActivity::Lobby | UserActivity::SoloReplay => {}
                }
            }
            ChatKind::Public {
                author: ChatAuthor::User { .. },
            } if chat.table_id == Some(self.table_id) && chat.text == "dump" => {
                if let (Some(game), Some(segment)) = (&mut self.game, self.replay_segment) {
                    game.reset();
                    for action in &self.game_actions {
                        game.process_action(action);
                        if game.status.turn >= segment {
                            break;
                        }
                    }
                    println!("bot: {}", game.agent.describe());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_joined(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableJoinedMessage = serde_json::from_str(json)?;
        self.clear_game();
        self.table_id = table.table_id;
        if matches!(
            table.stage,
            Some(TableStage::JustStarted | TableStage::Playing | TableStage::Replay)
        ) {
            self.send(
                "gameInitLoad",
                &serde_json::to_string(&TableIdMessage {
                    table_id: table.table_id,
                })?,
            );
        }
        Ok(())
    }

    fn clear_game(&mut self) {
        self.game = None;
        self.game_actions.clear();
        self.replay_segment = None;
        self.pending_requests
            .retain(|_, request| request.command != "gameInitLoad");
    }

    fn on_left(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        if table.table_id == self.table_id {
            self.table_id = 0;
            self.clear_game();
        }
        Ok(())
    }

    fn on_table_start(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.table_id = table.table_id;
        self.send("gameInitLoad", &serde_json::to_string(&table)?);
        Ok(())
    }

    fn on_init(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let init: InitMessage = serde_json::from_str(json)?;
        self.initialize_game(init)
    }

    fn initialize_game(&mut self, init: InitMessage) -> Result<(), serde_json::Error> {
        self.table_id = init.table_id as usize;
        let table_id = TableIdMessage {
            table_id: init.table_id as usize,
        };
        let game = match HanabGame::from_message(&init) {
            Ok(game) => game,
            Err(reason) => {
                self.clear_game();
                eprintln!(
                    "Unsupported game variant {}: {reason}",
                    init.options.variant_id
                );
                return Ok(());
            }
        };
        self.game = Some(game);
        self.game_actions = Vec::with_capacity(256);
        self.replay_segment = None;
        self.send("gameStateLoad", &serde_json::to_string(&table_id)?);
        Ok(())
    }

    fn on_game_state(&mut self, json: &str) -> Result<(), serde_json::Error> {
        // gameState bundles the event history with notes, votes, clocks, and
        // presence. Public events provide all the state our strategy needs.
        let state: GameStateMessage = serde_json::from_str(json)?;
        if state.table_id != self.table_id as u64 {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            game.reset();
            self.game_actions.clear();
            for action in state.events.iter() {
                game.process_action(action);
                self.game_actions.push(action.clone());
            }
            match state.phase {
                Some(GamePhase::Playing { pause }) => {
                    game.paused = pause.pausing_player_index.is_some();
                }
                Some(GamePhase::Replay { shared_replay }) => {
                    game.can_act = false;
                    self.replay_segment = shared_replay.map(|replay| replay.turn);
                }
                None => {}
            }
            let table_id = TableIdMessage {
                table_id: state.table_id as usize,
            };
            // The server sends live events after this snapshot. gameLoaded
            // marks us connected and must still precede our opening move.
            self.send("gameLoaded", &serde_json::to_string(&table_id)?);
            self.act()?;
        } else {
            eprintln!("Received game state before game initialization");
        }
        Ok(())
    }

    fn on_action(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let action: GameActionMessage = serde_json::from_str(json)?;
        if action.table_id != self.table_id as u64 {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            let events = match action.batch {
                GameEventBatch::Current { events } => events,
                GameEventBatch::Legacy { event } => vec![event],
            };
            // A move includes its draw and turn change. Never act on a partial
            // batch, especially one that ends the game.
            for event in events {
                game.process_action(&event);
                self.game_actions.push(event);
            }
            self.act()?;
        } else {
            println!("Retrieved game action but I aren't in a game");
        }
        Ok(())
    }

    fn on_game_finished(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let finished: GameFinishedMessage = serde_json::from_str(json)?;
        if finished.table_id != self.table_id as u64 {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            game.can_act = false;
            if let Some(events) = finished.events {
                game.reset();
                for event in &events {
                    game.process_action(event);
                }
                self.game_actions = events;
            }
            game.current_player_index = None;
        }
        self.replay_segment = Some(finished.shared_replay.turn);
        if let Some(table) = self.table_states.get_mut(&self.table_id) {
            table.phase = TablePhase::SharedReplay;
        }
        Ok(())
    }

    fn on_pause(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let pause: PauseMessage = serde_json::from_str(json)?;
        if pause.table_id == self.table_id as u64 {
            if let Some(game) = &mut self.game {
                game.paused = pause.pause.pausing_player_index.is_some();
                self.act()?;
            }
        }
        Ok(())
    }

    fn on_replay_segment(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let segment: ReplaySegmentMessage = serde_json::from_str(json)?;
        if segment.table_id == self.table_id as u64 {
            if let ReplayAction::Turn { turn } = segment.action {
                self.replay_segment = Some(turn);
            }
        }
        Ok(())
    }

    fn act(&mut self) -> Result<(), serde_json::Error> {
        if let Some(game) = &mut self.game {
            if let Some(action) = game.act(self.table_id as u64) {
                // wait a bit to simulate thinking ...
                let duration = std::time::Duration::from_secs(1);
                std::thread::sleep(duration);
                self.send("playerAction", &serde_json::to_string(&action)?);
            }
        }
        Ok(())
    }

    fn send(&mut self, command: &str, body: &str) {
        let data: serde_json::Value = serde_json::from_str(body).expect("request data is JSON");
        let id = self.next_request_id;
        self.next_request_id += 1;
        let request = PendingRequest {
            command: command.to_owned(),
            table_id: data
                .get("tableId")
                .and_then(serde_json::Value::as_u64)
                .map(|id| id as usize),
        };
        let frame = serde_json::json!({"id":id,"request":command,"data":data});
        println!("==> {frame}");
        let message = Message::Text(frame.to_string().into());
        match self.client.send(message) {
            Ok(()) => {
                self.pending_requests.insert(id, request);
            }
            Err(error) => {
                eprintln!("Could not send message to server: {error}");
            }
        }
    }
}

#[derive(Debug)]
struct Slot {
    index: u8,
}

/// The bot: H-Group conventions capped at `RAIKAN_LEVEL=<n>` (default 9), with the root
/// planner when `RAIKAN_SEARCH=fast|full` (default off; all cores, at most
/// `RAIKAN_TIME_LIMIT_MS`, default 3000, per decision).
fn new_agent() -> Box<dyn Agent> {
    let mut config = Config::default();
    if let Some(level) = std::env::var("RAIKAN_LEVEL")
        .ok()
        .and_then(|l| l.parse().ok())
    {
        config.max_level = level;
    }
    if let Ok(mode) = std::env::var("RAIKAN_SEARCH") {
        match mode.parse() {
            Ok(mode) => config.search = raikan::search::SearchConfig::new(mode),
            Err(err) => eprintln!("ignoring RAIKAN_SEARCH: {err}"),
        }
        // A live table has one decision at a time: use the cores, and keep a turn short.
        config.search.threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        config.search.time_limit = Some(std::time::Duration::from_millis(
            std::env::var("RAIKAN_TIME_LIMIT_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
        ));
    }
    Box::new(BotAgent::new(config))
}

/// Public counters derived from the events (the server sends no status events).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Status {
    turn: u8,
    score: u8,
    max_score: u8,
    num_strikes: u8,
    clues: u8,
}

struct HanabGame {
    player_names: Vec<String>,
    own_player: u8,
    /// Hands newest first, as hanab.live shows them.
    hands: Vec<VecDeque<Slot>>,
    agent: Box<dyn Agent>,
    variant: Variant,
    /// Public state derived from the events (the server sends no status events).
    state: GameState,
    current_player_index: Option<u8>,
    can_act: bool,
    paused: bool,
    status: Status,
}

impl HanabGame {
    fn from_message(init: &InitMessage) -> Result<Self, String> {
        let variant = Variant::by_id(init.options.variant_id as u32).map_err(|e| e.to_string())?;
        // Only variants whose suits the conventions cover (plain and rainbow) unless opted in.
        if let Err(missing) = variant.convention_support()
            && std::env::var_os("RAIKAN_EXPERIMENTAL_VARIANTS").is_none()
        {
            return Err(format!(
                "{} needs conventions for {missing} (set RAIKAN_EXPERIMENTAL_VARIANTS=1 to play anyway)",
                variant.name
            ));
        }
        let own_player = init.mode.our_player_index.unwrap_or(0);
        let mut game = Self {
            player_names: init.player_names.clone(),
            hands: Vec::new(),
            own_player,
            agent: new_agent(),
            state: GameState::new(&variant, init.player_names.len() as u8),
            variant,
            current_player_index: Some(0),
            can_act: init.mode.kind == "playing" && init.mode.our_player_index.is_some(),
            paused: false,
            status: Status::default(),
        };
        game.reset();
        Ok(game)
    }

    fn reset(&mut self) {
        let seats = self.player_names.len() as u8;
        self.hands = (0..seats).map(|_| VecDeque::new()).collect();
        self.agent = new_agent();
        self.agent
            .start(&self.variant, seats, Seat(self.own_player));
        self.state = GameState::new(&self.variant, seats);
        // New games contain only draw events: player zero must act before the server emits
        // its first turn event. Later history events override this opening turn, and the
        // client waits for the full history before acting.
        self.current_player_index = Some(0);
        self.paused = false;
        self.status.turn = 0;
        self.update_status();
    }

    fn update_status(&mut self) {
        self.status.score = self.state.score(&self.variant);
        self.status.num_strikes = self.state.strikes;
        self.status.clues = self.state.clue_tokens();
        self.status.max_score = self.variant.max_reachable_score(&self.state.discarded);
    }

    fn act(&mut self, table_id: u64) -> Option<ActionMessage> {
        if !self.can_act || self.paused || self.current_player_index != Some(self.own_player) {
            return None;
        }
        self.current_player_index = None;
        self.state.current = Seat(self.own_player);
        let action = self.agent.choose_action(&self.state);
        println!("Decision: {:?}", action);
        Some(self.action_message(table_id, action))
    }

    fn action_message(&self, table_id: u64, action: Action) -> ActionMessage {
        let action = match action {
            Action::Play(card) => PlayerAction::Play { order: card.0 },
            Action::Discard(card) => PlayerAction::Discard { order: card.0 },
            Action::Clue { target, clue } => PlayerAction::Clue {
                target: target.0,
                clue: match self.variant.clue_value(clue) {
                    ClueValue::Color(color) => ClueMessage::Color { color },
                    ClueValue::Rank(rank) => ClueMessage::Rank { rank },
                },
            },
        };
        ActionMessage { table_id, action }
    }

    fn identity(&self, identity: &CardIdentity) -> Option<raikan::domain::Identity> {
        ((identity.suit_index as usize) < self.variant.num_suits()
            && self.variant.ranks().any(|r| r == identity.rank))
        .then(|| {
            self.variant
                .identity(identity.suit_index as usize, identity.rank)
        })
    }

    fn observe(&mut self, event: Event) {
        engine::apply_event(&self.variant, &mut self.state, &event);
        self.agent.observe(&event);
        self.update_status();
    }

    fn remove_from_hand(&mut self, player_index: u8, order: u8) -> bool {
        let hand = &mut self.hands[player_index as usize];
        match hand.iter().position(|slot| slot.index == order) {
            Some(pos) => {
                hand.remove(pos);
                true
            }
            None => false,
        }
    }

    fn process_action(&mut self, action: &GameAction) {
        println!("Action: {:?}", action);
        match action {
            GameAction::Draw {
                player_index,
                order,
                identity,
            } => {
                let identity = if *player_index == self.own_player {
                    None
                } else {
                    match identity.as_ref().and_then(|i| self.identity(i)) {
                        Some(identity) => Some(identity),
                        None => {
                            eprintln!("Missing identity for another player's drawn card");
                            return;
                        }
                    }
                };
                self.hands[*player_index as usize].push_front(Slot { index: *order });
                self.observe(Event::Draw {
                    seat: Seat(*player_index),
                    card: CardId(*order),
                    identity,
                });
            }
            GameAction::Play {
                order,
                player_index,
                identity,
            } => {
                if !self.remove_from_hand(*player_index, *order) {
                    eprintln!("Did not find card that should be played");
                    return;
                }
                let identity = self.identity(identity);
                self.observe(Event::Play {
                    seat: Seat(*player_index),
                    card: CardId(*order),
                    identity,
                    success: true,
                });
            }
            GameAction::Discard {
                order,
                player_index,
                identity,
                ..
            }
            | GameAction::Misplay {
                order,
                player_index,
                identity,
            } => {
                let failed = matches!(
                    action,
                    GameAction::Misplay { .. } | GameAction::Discard { failed: true, .. }
                );
                if !self.remove_from_hand(*player_index, *order) {
                    eprintln!("Did not find card that should be discarded");
                    return;
                }
                let (seat, card, identity) =
                    (Seat(*player_index), CardId(*order), self.identity(identity));
                self.observe(if failed {
                    Event::Play {
                        seat,
                        card,
                        identity,
                        success: false,
                    }
                } else {
                    Event::Discard {
                        seat,
                        card,
                        identity,
                    }
                });
            }
            GameAction::Clue {
                clue: clue_message,
                player_index: giver,
                orders,
                target,
            } => {
                let value = match clue_message {
                    ClueMessage::Rank { rank } => ClueValue::Rank(*rank),
                    ClueMessage::Color { color } => ClueValue::Color(*color),
                };
                let Some(clue) = self.variant.clue_id(value) else {
                    eprintln!("Clue {value:?} does not exist in {}", self.variant.name);
                    return;
                };
                let touched: CardSet = orders.iter().map(|&o| CardId(o)).collect();
                self.observe(Event::Clue {
                    giver: Seat(*giver),
                    target: Seat(*target),
                    clue,
                    touched,
                });
            }
            GameAction::Turn {
                current_player_index,
                turn,
            } => {
                self.status.turn = *turn;
                self.current_player_index = *current_player_index;
                if let Some(seat) = current_player_index {
                    self.state.current = Seat(*seat);
                }
            }
            GameAction::GameOver { end_condition } => {
                self.current_player_index = None;
                self.can_act = false;
                let reason = if end_condition.contains("strike") {
                    EndReason::Strikeout
                } else {
                    EndReason::Normal
                };
                self.observe(Event::GameOver { reason });
                println!("Game finished: {end_condition}");
            }
            GameAction::CardIdentity | GameAction::PlayerTimes => {}
        }
    }
}

#[cfg(test)]
mod login_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use tungstenite::client::IntoClientRequest;

    fn http_response(
        status: &str,
        headers: &str,
        body: &str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/login", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut request = String::new();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
                if let Some(length) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = length.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).unwrap();
            request.push_str(std::str::from_utf8(&body).unwrap());
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        (url, server)
    }

    fn client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    #[test]
    fn bot_login_uses_current_protocol_and_passes_only_the_session_cookie_to_websocket() {
        let (url, server) = http_response(
            "200 OK",
            concat!(
                "Set-Cookie: theme=dark; Path=/\r\n",
                "Set-Cookie: hanabi.sid=123.fake.signature; Path=/; Max-Age=31536000; HttpOnly; Secure; SameSite=Strict\r\n",
                "Set-Cookie: hanabi.sid.123=123.fake.signature; Path=/; HttpOnly\r\n",
            ),
            r#"{"userId":123}"#,
        );
        let cookie = login(&client(), &url, "test bot", "p&=+ ä").unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /login HTTP/1.1\r\n"));
        assert!(request.contains("content-type: application/x-www-form-urlencoded\r\n"));
        let body = request.split_once("\r\n\r\n").unwrap().1;
        let form = reqwest::Url::parse(&format!("http://localhost/?{body}")).unwrap();
        let fields: BTreeMap<_, _> = form.query_pairs().into_owned().collect();
        assert_eq!(
            fields,
            BTreeMap::from([
                ("username".to_owned(), "test bot".to_owned()),
                ("password".to_owned(), "p&=+ ä".to_owned()),
                ("client".to_owned(), "bot".to_owned()),
                ("createAccount".to_owned(), "false".to_owned()),
            ])
        );
        let websocket = websocket_request(&cookie).into_client_request().unwrap();
        assert_eq!(websocket.uri(), "wss://hanab.live/ws?client=bot");
        assert_eq!(
            websocket.headers()["cookie"],
            "hanabi.sid=123.fake.signature"
        );
        assert_eq!(websocket.headers().get_all("cookie").iter().count(), 1);
    }

    #[test]
    fn unauthorized_login_reports_the_server_explanation() {
        let (url, server) = http_response(
            "401 Unauthorized",
            "Content-Type: application/json\r\n",
            r#"{"message":"This account requires a password reset.","code":"example"}"#,
        );
        let error = login(&client(), &url, "test bot", "do-not-print").unwrap_err();
        server.join().unwrap();
        assert_eq!(
            error.to_string(),
            "Login failed (HTTP 401 Unauthorized): This account requires a password reset."
        );
    }

    #[test]
    fn login_errors_handle_plain_text_empty_bodies_and_redact_the_password() {
        for (body, expected) in [
            ("Too many attempts.", "Too many attempts."),
            ("", "The server returned no explanation."),
            ("Rejected do-not-print", "Rejected [redacted]"),
        ] {
            let (url, server) = http_response("429 Too Many Requests", "", body);
            let error = login(&client(), &url, "test bot", "do-not-print").unwrap_err();
            server.join().unwrap();
            assert_eq!(
                error.to_string(),
                format!("Login failed (HTTP 429 Too Many Requests): {expected}")
            );
        }
    }

    #[test]
    fn successful_http_response_requires_a_nonempty_session_cookie() {
        for headers in [
            "",
            "Set-Cookie: theme=dark; Path=/\r\n",
            "Set-Cookie: hanabi.sid=; Max-Age=0; Path=/\r\n",
        ] {
            let (url, server) = http_response("200 OK", headers, r#"{"userId":123}"#);
            let error = login(&client(), &url, "test bot", "do-not-print").unwrap_err();
            server.join().unwrap();
            assert_eq!(
                error.to_string(),
                "Login succeeded but no hanabi.sid session cookie was returned"
            );
        }
    }

    #[test]
    fn welcome_accepts_current_and_legacy_account_fields() {
        for payload in [
            r#"{"userId":123,"username":"test bot","muted":false,"firstTimeUser":false,"settings":{},"friends":[],"blocked":[],"playingAtTables":[]}"#,
            r#"{"userID":123,"username":"test bot","muted":false,"totalGames":42}"#,
        ] {
            let welcome: WelcomeMessage = serde_json::from_str(payload).unwrap();
            assert_eq!(welcome.user_id, 123);
            assert_eq!(welcome.username, "test bot");
        }
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use serde_json::{Value, json};
    use std::net::TcpListener;
    use tungstenite::protocol::Role;

    // Exercise actual outgoing frames without contacting Hanabi Live.
    fn connection() -> (HanabClient, WebSocket<TcpStream>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        (
            HanabClient {
                client: WebSocket::from_raw_socket(
                    MaybeTlsStream::Plain(client),
                    Role::Client,
                    None,
                ),
                user_id: 1,
                username: "test bot".into(),
                user_states: BTreeMap::new(),
                table_states: BTreeMap::new(),
                table_id: 0,
                game: None,
                game_actions: Vec::new(),
                replay_segment: None,
                next_request_id: 1,
                pending_requests: BTreeMap::new(),
            },
            WebSocket::from_raw_socket(server, Role::Server, None),
        )
    }

    fn receive(server: &mut WebSocket<TcpStream>, expected_command: &str) -> Value {
        receive_frame(server, expected_command)["data"].clone()
    }

    fn receive_frame(server: &mut WebSocket<TcpStream>, expected_command: &str) -> Value {
        let message = server.read().unwrap().into_text().unwrap();
        let frame: Value = serde_json::from_str(&message).unwrap();
        assert_eq!(frame["request"], expected_command);
        assert!(frame["id"].as_u64().is_some_and(|id| id > 0));
        assert_eq!(frame.as_object().unwrap().len(), 3);
        frame
    }

    fn assert_no_message(server: &mut WebSocket<TcpStream>) {
        server.get_mut().set_nonblocking(true).unwrap();
        let result = server.read();
        server.get_mut().set_nonblocking(false).unwrap();
        assert!(matches!(result, Err(tungstenite::Error::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock));
    }

    fn init(mode: Value) -> InitMessage {
        serde_json::from_value(json!({
            "tableId":42, "playerNames":["test bot","Alice"],
            "mode":mode, "options":{"variantId":0},
            "hasCustomSeed":false, "startedAt":1234, "characters":[], "pauseQueued":false
        }))
        .unwrap()
    }

    fn initial_events() -> Vec<Value> {
        let mut events = Vec::new();
        for order in 0..5 {
            events.push(json!({"type":"draw","playerIndex":0,"order":order}));
        }
        for rank in 1..=5 {
            events.push(json!({"type":"draw","playerIndex":1,"order":rank+4,
                "identity":{"suitIndex":0,"rank":rank}}));
        }
        events
    }

    fn game_event(game: &mut HanabGame, event: Value) {
        game.process_action(&serde_json::from_value(event).unwrap());
    }

    fn dealt_game() -> HanabGame {
        let mut game =
            HanabGame::from_message(&init(json!({"type":"playing","ourPlayerIndex":0}))).unwrap();
        for event in initial_events() {
            game_event(&mut game, event);
        }
        game
    }

    #[test]
    fn json_frames_reconnect_play_and_rebuild_the_finished_game() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hanabi-live-json-frames.json"
        ))
        .unwrap();
        let (mut bot, mut server) = connection();
        let mut expected_id = 1;
        for frame in fixture["frames"].as_array().unwrap() {
            bot.process_frame(&frame.to_string()).unwrap();
            let request = match (frame["event"].as_str(), frame["reply"].as_u64()) {
                (Some("welcome"), _) => Some("tableReattend"),
                (Some("tableJoined"), _) => Some("gameInitLoad"),
                (_, Some(2)) => Some("gameStateLoad"),
                (Some("gameState"), _) => Some("gameLoaded"),
                (Some("gameEvent"), _) if frame["data"]["events"][2]["turn"] == 2 => {
                    let game = bot.game.as_ref().unwrap();
                    assert_eq!((game.status.score, game.status.turn), (1, 2));
                    assert_eq!(game.hands[1].len(), 5);
                    assert_eq!(game.hands[1][0].index, 10);
                    Some("playerAction")
                }
                (Some("gameFinished"), _) => {
                    assert_eq!(bot.game_actions.len(), 18);
                    // The finished history replaces hidden draws with identities.
                    assert_eq!(
                        serde_json::to_value(&bot.game_actions[0]).unwrap(),
                        frame["data"]["events"][0]
                    );
                    assert_eq!(bot.replay_segment, Some(3));
                    assert!(!bot.game.as_ref().unwrap().can_act);
                    None
                }
                _ => None,
            };
            if let Some(command) = request {
                let outgoing = receive_frame(&mut server, command);
                assert_eq!(outgoing["id"], expected_id);
                assert_eq!(outgoing["data"]["tableId"], 42);
                if command == "playerAction" {
                    serde_json::from_value::<ActionMessage>(outgoing["data"].clone()).unwrap();
                }
                expected_id += 1;
            }
            assert_no_message(&mut server);
        }
        assert_eq!(expected_id, 6);
        assert!(bot.pending_requests.is_empty());
        assert_eq!(bot.table_states[&42].phase, TablePhase::SharedReplay);
        assert_eq!(bot.replay_segment, Some(1));
        let game = bot.game.as_ref().unwrap();
        assert_eq!(
            (game.status.turn, game.status.score, game.status.clues),
            (3, 1, 7)
        );
        assert!(game.hands.iter().all(|hand| hand.len() == 5));
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn table_joined_loads_started_games_and_replays_but_waits_in_pregame() {
        for stage in ["pregame", "justStarted", "playing", "replay"] {
            let (mut bot, mut server) = connection();
            bot.game = Some(dealt_game());
            bot.replay_segment = Some(3);
            bot.process_frame(
                &json!({"event":"tableJoined","data":{"tableId":42,"stage":stage}}).to_string(),
            )
            .unwrap();
            assert_eq!(bot.table_id, 42);
            assert!(bot.game.is_none());
            assert!(bot.replay_segment.is_none());
            if stage != "pregame" {
                assert_eq!(receive(&mut server, "gameInitLoad"), json!({"tableId":42}));
            }
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn setup_replies_are_correlated_and_cannot_restore_a_departed_table() {
        let (mut bot, mut server) = connection();
        for table in [42, 99] {
            bot.process_frame(
                &json!({"event":"tableJoined","data":{"tableId":table,"stage":"playing"}})
                    .to_string(),
            )
            .unwrap();
            assert_eq!(
                receive(&mut server, "gameInitLoad"),
                json!({"tableId":table})
            );
        }
        let mut setup =
            serde_json::to_value(init(json!({"type":"playing","ourPlayerIndex":0}))).unwrap();
        setup.as_object_mut().unwrap().remove("tableId");
        // The old reply has no table ID, and must not initialize the new table.
        bot.process_frame(&json!({"reply":1,"data":setup}).to_string())
            .unwrap();
        assert!(bot.game.is_none());
        assert_no_message(&mut server);
        bot.process_frame(&json!({"reply":2,"data":setup}).to_string())
            .unwrap();
        assert!(bot.game.is_some());
        assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":99}));
        // A duplicate reply and an unrelated acknowledgement have no effect.
        for frame in [json!({"reply":2,"data":setup}), json!({"reply":3})] {
            bot.process_frame(&frame.to_string()).unwrap();
            assert_no_message(&mut server);
        }
        bot.process_frame(r#"{"event":"tableJoined","data":{"tableId":99,"stage":"justStarted"}}"#)
            .unwrap();
        assert_eq!(receive(&mut server, "gameInitLoad"), json!({"tableId":99}));
        bot.process_frame(r#"{"event":"tableLeft","data":{"tableId":99}}"#)
            .unwrap();
        bot.process_frame(&json!({"reply":4,"data":setup}).to_string())
            .unwrap();
        assert_eq!(bot.table_id, 0);
        assert!(bot.game.is_none());
        assert!(bot.pending_requests.is_empty());
        assert_no_message(&mut server);
    }

    #[test]
    fn refused_requests_and_malformed_frames_do_not_initialize_a_game() {
        let (mut bot, mut server) = connection();
        for malformed in [
            "welcome {}",
            "{",
            "null",
            "{}",
            r#"{"reply":"1"}"#,
            r#"{"event":"welcome","data":{}}"#,
        ] {
            assert!(bot.process_frame(malformed).is_err());
        }
        bot.process_frame(r#"{"event":"tableJoined","data":{"tableId":42,"stage":"playing"}}"#)
            .unwrap();
        receive(&mut server, "gameInitLoad");
        bot.process_frame(r#"{"reply":1,"error":{"message":"You are not at this table."}}"#)
            .unwrap();
        assert!(bot.game.is_none());
        assert!(bot.pending_requests.is_empty());
        bot.send("tableJoin", r#"{"tableId":42,"password":"secret"}"#);
        receive(&mut server, "tableJoin");
        bot.process_frame(r#"{"reply":2,"error":{"message":"Incorrect password."}}"#)
            .unwrap();
        assert_eq!(bot.table_id, 0);
        assert!(bot.pending_requests.is_empty());
        bot.process_frame(
            r#"{"event":"welcome","data":{"userId":7,"username":"bot","muted":false}}"#,
        )
        .unwrap();
        assert_eq!(bot.user_id, 7);
        assert_no_message(&mut server);
    }

    #[test]
    fn batched_events_apply_completely_before_acting_and_ignore_other_tables() {
        let (mut bot, mut server) = connection();
        bot.table_id = 42;
        bot.game = Some(dealt_game());
        bot.process_frame(r#"{"event":"gameEvent","data":{"tableId":99,"events":[{"type":"turn","turn":2,"currentPlayerIndex":0}]}}"#).unwrap();
        assert_eq!(bot.game.as_ref().unwrap().status.turn, 0);
        assert!(bot.game_actions.is_empty());
        bot.process_frame(r#"{"event":"gameEvent","data":{"tableId":42,"events":[{"type":"turn","turn":2,"currentPlayerIndex":0},{"type":"gameOver","endCondition":"terminatedByVote","voters":[0,1]}]}}"#).unwrap();
        assert_eq!(bot.game.as_ref().unwrap().status.turn, 2);
        assert_eq!(bot.game_actions.len(), 2);
        assert!(!bot.game.as_ref().unwrap().can_act);
        assert_no_message(&mut server);
    }

    #[test]
    fn replay_snapshots_restore_the_cursor_and_never_send_moves() {
        let (mut bot, mut server) = connection();
        bot.table_id = 42;
        bot.game = Some(dealt_game());
        bot.process_frame(
            &json!({"event":"gameState","data":{
                "tableId":42,"events":initial_events(),
                "phase":{"type":"replay","deck":[],"sharedReplay":{"turn":4}}
            }})
            .to_string(),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
        assert_eq!(bot.replay_segment, Some(4));
        assert!(!bot.game.as_ref().unwrap().can_act);
        assert_no_message(&mut server);
    }

    #[test]
    fn paused_snapshots_wait_for_resume_before_sending_a_move() {
        let (mut bot, mut server) = connection();
        bot.table_id = 42;
        bot.game = Some(dealt_game());
        bot.process_frame(
            &json!({"event":"gameState","data":{
                "tableId":42,"events":initial_events(),
                "phase":{"type":"playing","pause":{"pausingPlayerIndex":1,"queued":false}}
            }})
            .to_string(),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
        assert_no_message(&mut server);
        bot.process_frame(r#"{"event":"pause","data":{"tableId":99,"queued":false}}"#)
            .unwrap();
        assert!(bot.game.as_ref().unwrap().paused);
        assert_no_message(&mut server);
        bot.process_frame(r#"{"event":"pause","data":{"tableId":42,"queued":false}}"#)
            .unwrap();
        assert_eq!(receive(&mut server, "playerAction")["tableId"], 42);
        bot.process_frame(r#"{"event":"pause","data":{"tableId":42,"queued":false}}"#)
            .unwrap();
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn current_protocol_reconnects_plays_once_and_finishes_in_shared_replay() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hanabi-live-current-protocol.json"
        ))
        .unwrap();
        let (mut bot, mut server) = connection();
        for message in fixture["messages"].as_array().unwrap() {
            let command = message["command"].as_str().unwrap();
            let payload = &message["payload"];
            bot.process_message(command, &payload.to_string())
                .unwrap_or_else(|error| panic!("{command}: {error}"));
            match command {
                "welcome" => {
                    assert_eq!(bot.user_states.len(), 7);
                    assert!(matches!(
                        bot.user_states[&3].activity,
                        UserActivity::SoloReplay
                    ));
                    assert_eq!(bot.table_states.len(), 4);
                    assert_eq!(bot.table_id, 42);
                    assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":42}));
                }
                "tableStarted" => {
                    assert_eq!(receive(&mut server, "gameInitLoad"), json!({"tableId":42}));
                }
                "gameInit" => {
                    assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
                }
                "gameState" => {
                    assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
                    assert_eq!(bot.game_actions.len(), 12);
                    assert_eq!(bot.game.as_ref().unwrap().current_player_index, Some(1));
                }
                "gameEvent" if payload["event"]["turn"] == 2 => {
                    let action: ActionMessage =
                        serde_json::from_value(receive(&mut server, "playerAction")).unwrap();
                    assert_eq!(action.table_id, 42);
                    bot.act().unwrap();
                }
                _ => {}
            }
            assert_no_message(&mut server);
        }
        assert_eq!(bot.table_states[&42].phase, TablePhase::SharedReplay);
        assert!(!bot.table_states[&42].has_active_player("test bot"));
        let game = bot.game.as_ref().unwrap();
        assert_eq!(
            (game.status.turn, game.status.score, game.status.clues),
            (3, 1, 7)
        );
        assert!(game.hands.iter().all(|hand| hand.len() == 5));
        assert_eq!(bot.game_actions.len(), 17);
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn lobby_payloads_from_log_update_state_and_accept_server_chat() {
        let (mut bot, _server) = connection();
        // Minimized, anonymized shapes from bot.log, including fields we ignore.
        for (command, payload) in [
            (
                "userList",
                json!({"users":[{"userId":7,"username":"Alice",
                "activity":{"type":"sharedReplay","tableId":42},"inactive":false,
                "bot":false,"hGroupConventions":true,"hGroupLevel":7}]}),
            ),
            (
                "tableList",
                json!({"tables":[{"tableId":42,"joined":false,
                "phase":"sharedReplay","playerNames":["Alice","Bob"],
                "settings":{"name":"Test table","options":{"variantId":0}},
                "progress":50,"numPlayers":2}]}),
            ),
            ("lobbyStatus", json!({"type":"loading"})),
            (
                "chat",
                json!({"text":"Welcome!","kind":{"type":"public","author":{"type":"server"}},"sentAt":1234}),
            ),
            ("lobbyStatus", json!({"type":"ready","totalGames":42})),
            (
                "user",
                json!({"userId":7,"username":"Alice","activity":{"type":"lobby"},"inactive":false}),
            ),
            ("userInactive", json!({"userId":7,"inactive":true})),
            (
                "table",
                json!({"tableId":42,"joined":false,"phase":"pregame","playerNames":["Alice"],"settings":{"name":"Next game"}}),
            ),
        ] {
            bot.process_message(command, &payload.to_string()).unwrap();
        }
        assert!(bot.user_states[&7].inactive);
        assert!(matches!(bot.user_states[&7].activity, UserActivity::Lobby));
        assert_eq!(bot.table_states[&42].phase, TablePhase::Pregame);
        assert_eq!(bot.table_id, 0);
        bot.process_message("userLeft", r#"{"userId":7}"#).unwrap();
        bot.process_message("tableGone", r#"{"tableId":42}"#)
            .unwrap();
        assert!(bot.user_states.is_empty());
        assert!(bot.table_states.is_empty());
    }

    #[test]
    fn private_join_commands_use_activity_and_current_table_requests() {
        let (mut bot, mut server) = connection();
        bot.process_message("user", r#"{"userId":7,"username":"Alice","activity":{"type":"pregame","tableId":42},"inactive":false}"#).unwrap();
        bot.process_message(
            "chat",
            r#"{"text":"/join","kind":{"type":"private","author":{"type":"server"},"recipient":"test bot"}}"#,
        )
        .unwrap();
        assert_eq!(bot.table_id, 0);
        bot.process_message(
            "chat",
            r#"{"text":"/join","kind":{"type":"private","author":{"type":"user","username":"Alice"},"recipient":"test bot"}}"#,
        )
        .unwrap();
        assert_eq!(
            receive(&mut server, "tableJoin"),
            json!({"tableId":42,"password":"secret"})
        );
        bot.process_message("tableJoined", r#"{"tableId":42}"#)
            .unwrap();
        assert_eq!(bot.table_id, 42);

        for activity in ["playing", "spectating", "sharedReplay"] {
            bot.process_message("user", &json!({
                "userId":7,"username":"Alice","activity":{"type":activity,"tableId":43},"inactive":false
            }).to_string()).unwrap();
            bot.process_message(
                "chat",
                r#"{"text":"/join","kind":{"type":"private","author":{"type":"user","username":"Alice"},"recipient":"test bot"}}"#,
            )
            .unwrap();
            assert_eq!(receive(&mut server, "tableSpectate"), json!({"tableId":43}));
            assert_eq!(bot.table_id, 43);
        }

        bot.process_message("user", r#"{"userId":7,"username":"Alice","activity":{"type":"playing","tableId":44},"inactive":false}"#).unwrap();
        bot.process_message(
            "table",
            r#"{"tableId":44,"phase":{"type":"playing"},"playerNames":["Alice","test bot"]}"#,
        )
        .unwrap();
        bot.process_message(
            "chat",
            r#"{"text":"/join","kind":{"type":"private","author":{"type":"user","username":"Alice"},"recipient":"test bot"}}"#,
        )
        .unwrap();
        assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":44}));
        assert_eq!(bot.table_id, 44);
        assert_no_message(&mut server);
    }

    #[test]
    fn solo_replay_activity_updates_do_not_join_a_private_table() {
        let (mut bot, mut server) = connection();
        bot.process_message("user", r#"{"userId":7,"username":"Alice","activity":{"type":"pregame","tableId":42},"inactive":false}"#).unwrap();
        for activity in ["soloReplay", "replay"] {
            bot.process_message(
                "user",
                &json!({
                    "userId":7,"username":"Alice","activity":{"type":activity},"inactive":false
                })
                .to_string(),
            )
            .unwrap();
            bot.process_message(
                "chat",
                r#"{"text":"/join","kind":{"type":"private","author":{"type":"user","username":"Alice"},"recipient":"test bot"}}"#,
            )
            .unwrap();
            assert!(matches!(
                bot.user_states[&7].activity,
                UserActivity::SoloReplay
            ));
            assert_eq!(bot.table_id, 0);
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn chat_ignores_join_requests_without_a_known_user_addressing_the_bot() {
        let (mut bot, mut server) = connection();
        bot.process_message("user", r#"{"userId":7,"username":"Alice","activity":{"type":"pregame","tableId":42},"inactive":false}"#).unwrap();
        for kind in [
            json!({"type":"private","author":{"type":"user","username":"Alice"},"recipient":"Bob"}),
            json!({"type":"private","author":{"type":"user","username":"Unknown"},"recipient":"test bot"}),
            json!({"type":"private","author":{"type":"server"},"recipient":"test bot"}),
            json!({"type":"private","author":{"type":"discord","username":"Alice"},"recipient":"test bot"}),
            json!({"type":"public","author":{"type":"user","username":"Alice"}}),
            json!({"type":"public","author":{"type":"discord"}}),
            json!({"type":"suggestion","recipient":"test bot","settings":{"name":"Suggested table","options":{"variantId":0}}}),
        ] {
            bot.process_message("chat", &json!({"text":"/join","kind":kind}).to_string())
                .unwrap();
            assert_eq!(bot.table_id, 0);
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn public_dump_replays_only_user_messages_at_the_current_table() {
        let (mut bot, mut server) = connection();
        bot.table_id = 42;
        bot.game = Some(dealt_game());
        bot.game.as_mut().unwrap().status.turn = 9;
        bot.game_actions = initial_events()
            .into_iter()
            .map(|event| serde_json::from_value(event).unwrap())
            .collect();
        for turn in 1..=2 {
            bot.game_actions.push(GameAction::Turn {
                turn,
                current_player_index: Some(1),
            });
        }
        bot.replay_segment = Some(1);

        for (table_id, author) in [
            (None, json!({"type":"user","username":"Alice"})),
            (Some(43), json!({"type":"user","username":"Alice"})),
            (Some(42), json!({"type":"server"})),
            (Some(42), json!({"type":"discord","username":"Alice"})),
        ] {
            bot.process_message(
                "chat",
                &json!({
                    "text":"dump","tableId":table_id,"kind":{"type":"public","author":author}
                })
                .to_string(),
            )
            .unwrap();
            assert_eq!(bot.game.as_ref().unwrap().status.turn, 9);
        }

        bot.process_message(
            "chat",
            r#"{"text":"dump","tableId":42,"kind":{"type":"public","author":{"type":"user","username":"Alice"}}}"#,
        )
        .unwrap();
        let game = bot.game.as_ref().unwrap();
        assert_eq!(game.status.turn, 1);
        assert!(game.hands.iter().all(|hand| hand.len() == 5));
        assert_no_message(&mut server);
    }

    #[test]
    fn reconnect_without_joined_flag_prefers_active_membership_over_old_replays() {
        for (replay_phase, phase) in [
            (json!("sharedReplay"), json!("pregame")),
            (json!("sharedReplay"), json!("playing")),
            (
                json!({"type":"sharedReplay","gameId":123}),
                json!({"type":"pregame"}),
            ),
            (json!({"type":"sharedReplay"}), json!({"type":"playing"})),
        ] {
            let (mut bot, mut server) = connection();
            bot.process_message(
                "tableList",
                &json!({"tables":[
                    {"tableId":41,"phase":replay_phase,"playerNames":["test bot","Alice"]},
                    {"tableId":42,"phase":phase,"playerNames":["test bot","Alice"]}
                ]})
                .to_string(),
            )
            .unwrap();
            assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":42}));
            assert_eq!(bot.table_id, 42);
            assert_eq!(bot.table_states.len(), 2);
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn welcome_alone_rejoins_the_game_we_are_playing() {
        // The current server sends no tableList; welcome carries the tables.
        let (mut bot, mut server) = connection();
        bot.process_message(
            "welcome",
            &json!({
                "userId":1,"username":"test bot","muted":false,"firstTimeUser":false,
                "settings":{},"friends":[],"blocked":[],"playingAtTables":[42],
                "spectatingAtTables":[],"users":[],
                "tables":[
                    {"tableId":41,"phase":{"type":"sharedReplay","gameId":1,"progress":100},
                     "playerNames":["test bot","Alice"]},
                    {"tableId":42,"phase":{"type":"playing","progress":10},
                     "playerNames":["test bot","Alice"]}
                ]
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":42}));
        assert_eq!(bot.table_id, 42);
        assert_eq!(bot.table_states.len(), 2);
        assert_no_message(&mut server);

        // Without a table list, playingAtTables alone is enough.
        let (mut bot, mut server) = connection();
        bot.process_message(
            "welcome",
            r#"{"userId":1,"username":"test bot","muted":false,"playingAtTables":[7]}"#,
        )
        .unwrap();
        assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":7}));
        assert_no_message(&mut server);
    }

    #[test]
    fn reconnect_spectates_old_replay_when_no_active_membership_exists() {
        for phase in [
            json!("sharedReplay"),
            json!({"type":"sharedReplay","gameId":123}),
            json!({"type":"sharedReplay"}),
        ] {
            let (mut bot, mut server) = connection();
            bot.process_message(
                "tableList",
                &json!({"tables":[
                    {"tableId":41,"phase":{"type":"playing"},"playerNames":["Alice","Bob"]},
                    {"tableId":42,"phase":phase,"playerNames":["test bot","Alice"]}
                ]})
                .to_string(),
            )
            .unwrap();
            assert_eq!(receive(&mut server, "tableSpectate"), json!({"tableId":42}));
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn reported_game_state_acknowledges_loading_then_sends_the_opening_move() {
        let (mut bot, mut server) = connection();
        bot.process_message(
            "welcome",
            r#"{"userId":75582,"username":"hustenated1","muted":false}"#,
        )
        .unwrap();
        bot.process_message(
            "tableList",
            &json!({"tables":[
                {"tableId":9282,"phase":"playing","playerNames":["hustenated1","Alice","Bob"]}
            ]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            receive(&mut server, "tableReattend"),
            json!({"tableId":9282})
        );
        bot.process_message("tableStarted", r#"{"tableId":9282,"stage":"inProgress"}"#)
            .unwrap();
        assert_eq!(
            receive(&mut server, "gameInitLoad"),
            json!({"tableId":9282})
        );
        let mut game_init = init(json!({"type":"playing","ourPlayerIndex":0}));
        game_init.table_id = 9282;
        game_init.player_names = vec!["hustenated1".into(), "Alice".into(), "Bob".into()];
        bot.process_message("gameInit", &serde_json::to_string(&game_init).unwrap())
            .unwrap();
        assert_eq!(
            receive(&mut server, "gameStateLoad"),
            json!({"tableId":9282})
        );
        assert_no_message(&mut server);

        bot.process_message(
            "gameState",
            include_str!("../../tests/fixtures/hanabi-live-game-state.json"),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":9282}));
        let action = receive(&mut server, "playerAction");
        assert_eq!(action["tableId"], 9282);
        assert_eq!(action["action"]["type"], "clue");
        let game = bot.game.as_ref().unwrap();
        assert_eq!(game.status.turn, 0);
        assert!(game.hands.iter().all(|hand| hand.len() == 5));
        assert_eq!(bot.game_actions.len(), 15);

        bot.process_message(
            "connected",
            r#"{"tableId":9282,"connected":[true,false,true]}"#,
        )
        .unwrap();
        bot.process_message(
            "clock",
            r#"{"tableId":9282,"playerTimesMs":[0,0,0],"currentPlayerIndex":0,"turnElapsedMs":0}"#,
        )
        .unwrap();
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn reported_four_player_state_acknowledges_loading_and_waits_for_own_turn() {
        let (mut bot, mut server) = connection();
        bot.process_message(
            "gameInit",
            &json!({
                "tableId":9355,"playerNames":["Alice","test bot","Bob","Carol"],
                "mode":{"type":"playing","ourPlayerIndex":1},"options":{"variantId":0}
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            receive(&mut server, "gameStateLoad"),
            json!({"tableId":9355})
        );
        bot.process_message(
            "gameState",
            &json!({"tableId":9355,"events":[
                {"type":"draw","playerIndex":0,"order":0,"identity":{"suitIndex":3,"rank":3}},
                {"type":"draw","playerIndex":0,"order":1,"identity":{"suitIndex":3,"rank":2}},
                {"type":"draw","playerIndex":0,"order":2,"identity":{"suitIndex":1,"rank":1}},
                {"type":"draw","playerIndex":0,"order":3,"identity":{"suitIndex":0,"rank":4}},
                {"type":"draw","playerIndex":1,"order":4},
                {"type":"draw","playerIndex":1,"order":5},
                {"type":"draw","playerIndex":1,"order":6},
                {"type":"draw","playerIndex":1,"order":7},
                {"type":"draw","playerIndex":2,"order":8,"identity":{"suitIndex":4,"rank":1}},
                {"type":"draw","playerIndex":2,"order":9,"identity":{"suitIndex":1,"rank":1}},
                {"type":"draw","playerIndex":2,"order":10,"identity":{"suitIndex":2,"rank":2}},
                {"type":"draw","playerIndex":2,"order":11,"identity":{"suitIndex":0,"rank":4}},
                {"type":"draw","playerIndex":3,"order":12,"identity":{"suitIndex":4,"rank":3}},
                {"type":"draw","playerIndex":3,"order":13,"identity":{"suitIndex":1,"rank":2}},
                {"type":"draw","playerIndex":3,"order":14,"identity":{"suitIndex":4,"rank":5}},
                {"type":"draw","playerIndex":3,"order":15,"identity":{"suitIndex":3,"rank":3}}
            ]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":9355}));
        let game = bot.game.as_ref().unwrap();
        assert_eq!(game.current_player_index, Some(0));
        assert!(game.hands.iter().all(|hand| hand.len() == 4));
        assert_eq!(bot.game_actions.len(), 16);
        assert_no_message(&mut server);

        // Once the opening player clues, the next live turn lets the bot act.
        bot.process_message(
            "gameEvent",
            r#"{"tableId":9355,"event":{"type":"clue","playerIndex":0,"receiverIndex":2,"orders":[8,9],"clue":{"type":"rank","rank":1}}}"#,
        )
        .unwrap();
        assert_no_message(&mut server);
        bot.process_message(
            "gameEvent",
            r#"{"tableId":9355,"event":{"type":"turn","turn":1,"currentPlayerIndex":1}}"#,
        )
        .unwrap();
        let action: ActionMessage =
            serde_json::from_value(receive(&mut server, "playerAction")).unwrap();
        assert_eq!(action.table_id, 9355);
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn current_game_state_loads_observers_without_sending_moves() {
        for mode in [
            json!({"type":"spectating"}),
            json!({"type":"shadowing","ourPlayerIndex":0}),
            json!({"type":"replay","ourPlayerIndex":0}),
            json!({"type":"soloReplay","gameId":123,"ourPlayerIndex":0}),
            json!({"type":"soloReplay"}),
            json!({"type":"sharedReplay","ourPlayerIndex":0,"sharedReplay":{"turn":0}}),
        ] {
            let (mut bot, mut server) = connection();
            bot.process_message("gameInit", &serde_json::to_string(&init(mode)).unwrap())
                .unwrap();
            assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
            bot.process_message("gameState", &json!({
                "tableId":42,"events":initial_events(),"notes":[],"connected":[false,false],"spectators":[]
            }).to_string()).unwrap();
            assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
            assert_eq!(bot.game_actions.len(), 10);
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn renamed_removal_and_boot_messages_clear_only_matching_state() {
        for (user_removed, table_removed, table_booted) in [
            ("userRemoved", "tableRemoved", "tableBooted"),
            ("userLeft", "tableGone", "tableBoot"),
        ] {
            let (mut bot, mut server) = connection();
            bot.process_message(
                "user",
                r#"{"userId":7,"username":"Alice","activity":{"type":"lobby"},"inactive":false}"#,
            )
            .unwrap();
            bot.process_message(
                "table",
                r#"{"tableId":42,"phase":"playing","playerNames":["test bot","Alice"]}"#,
            )
            .unwrap();
            bot.process_message(
                "gameInit",
                &serde_json::to_string(&init(json!({"type":"playing","ourPlayerIndex":0})))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
            bot.game_actions.push(GameAction::Turn {
                turn: 1,
                current_player_index: Some(1),
            });
            bot.replay_segment = Some(1);

            for _ in 0..2 {
                bot.process_message(user_removed, r#"{"userId":7}"#)
                    .unwrap();
            }
            assert!(bot.user_states.is_empty());
            bot.process_message(table_removed, r#"{"tableId":42}"#)
                .unwrap();
            assert!(bot.table_states.is_empty());
            // Removing a lobby listing does not mean that we left the table.
            assert!(bot.game.is_some());
            bot.process_message(table_booted, r#"{"tableId":99}"#)
                .unwrap();
            assert!(bot.game.is_some());
            bot.process_message(table_booted, r#"{"tableId":42}"#)
                .unwrap();
            assert_eq!(bot.table_id, 0);
            assert!(bot.game.is_none());
            assert!(bot.game_actions.is_empty());
            assert!(bot.replay_segment.is_none());
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn draw_only_initial_history_starts_player_zero_and_waits_for_other_seats() {
        // The initial snapshot in bot.log has draws, but no turn event. The
        // server emits its first turn event only after player zero acts.
        let cards = [
            [(3, 1), (1, 1), (2, 5), (1, 1), (4, 3)],
            [(3, 2), (2, 2), (1, 2), (2, 1), (4, 4)],
            [(3, 1), (1, 2), (0, 1), (0, 2), (4, 1)],
        ];
        for own_player in 0..3 {
            let (mut bot, mut server) = connection();
            let mut game_init = init(json!({"type":"playing","ourPlayerIndex":own_player}));
            game_init.player_names = vec!["Alice".into(), "Bob".into(), "Carol".into()];
            bot.process_message("init", &serde_json::to_string(&game_init).unwrap())
                .unwrap();
            assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
            assert_no_message(&mut server);

            let mut events = Vec::new();
            for (player, hand) in cards.iter().enumerate() {
                for (position, &(suit, rank)) in hand.iter().enumerate() {
                    let mut event =
                        json!({"type":"draw","playerIndex":player,"order":5*player+position});
                    if player != own_player {
                        event["identity"] = json!({"suitIndex":suit,"rank":rank});
                    }
                    events.push(event);
                }
            }
            bot.process_message(
                "gameEventList",
                &json!({"tableId":42,"events":events}).to_string(),
            )
            .unwrap();
            // Readiness precedes the move, and only the opening player acts.
            assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
            if own_player == 0 {
                let action = receive(&mut server, "playerAction");
                assert_eq!(action["tableId"], 42);
                assert_eq!(action["action"]["type"], "clue");
            } else {
                assert_eq!(bot.game.as_ref().unwrap().current_player_index, Some(0));
            }
            bot.act().unwrap();
            assert_no_message(&mut server);
            assert_eq!(bot.game.as_ref().unwrap().status.turn, 0);
            assert!(
                bot.game
                    .as_ref()
                    .unwrap()
                    .hands
                    .iter()
                    .all(|hand| hand.len() == 5)
            );
        }
    }

    #[test]
    fn loads_game_history_and_sends_one_current_protocol_move_on_own_turn() {
        let (mut bot, mut server) = connection();
        bot.process_message("tableStarted", r#"{"tableId":42,"stage":"inProgress"}"#)
            .unwrap();
        assert_eq!(receive(&mut server, "gameInitLoad"), json!({"tableId":42}));
        bot.process_message(
            "init",
            &serde_json::to_string(&init(json!({"type":"playing","ourPlayerIndex":0}))).unwrap(),
        )
        .unwrap();
        assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
        let mut events = initial_events();
        events.push(json!({"type":"turn","turn":1,"currentPlayerIndex":1}));
        let history = json!({"tableId":42,"events":events}).to_string();
        // A repeated snapshot replaces, rather than duplicates, the state.
        for _ in 0..2 {
            bot.process_message("gameState", &history).unwrap();
            assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
            assert_eq!(bot.game_actions.len(), 11);
            assert_eq!(bot.game.as_ref().unwrap().hands[0].len(), 5);
            assert_no_message(&mut server);
        }
        bot.process_message(
            "gameEvent",
            r#"{"tableId":99,"event":{"type":"turn","turn":17,"currentPlayerIndex":0}}"#,
        )
        .unwrap();
        assert_eq!(bot.game.as_ref().unwrap().status.turn, 1);
        bot.process_message(
            "gameEvent",
            r#"{"tableId":42,"event":{"type":"turn","turn":2,"currentPlayerIndex":0}}"#,
        )
        .unwrap();
        let action = receive(&mut server, "playerAction");
        assert_eq!(action["tableId"], 42);
        assert_eq!(action["action"]["type"], "clue"); // Eight clues, and visible playable cards.
        assert_eq!(action["action"]["receiverIndex"], 1);
        assert!(serde_json::from_value::<ActionMessage>(action).is_ok());
        assert!(bot.game.as_mut().unwrap().act(42).is_none());
        bot.process_message(
            "gameEvent",
            r#"{"tableId":42,"event":{"type":"turn","turn":3}}"#,
        )
        .unwrap();
        assert_eq!(bot.game.as_ref().unwrap().current_player_index, None);
        bot.process_message("tableLeft", r#"{"tableId":99}"#)
            .unwrap();
        assert!(bot.game.is_some());
        bot.process_message("tableBoot", r#"{"tableId":42}"#)
            .unwrap();
        assert!(bot.game.is_none());
        assert!(bot.game_actions.is_empty());
    }

    #[test]
    fn moves_use_card_orders_absolute_seats_and_tagged_clues() {
        let game = dealt_game();
        let clue = |value| game.variant.clue_id(value).unwrap();
        for (action, expected) in [
            (Action::Play(CardId(4)), json!({"type":"play","order":4})),
            (
                Action::Discard(CardId(0)),
                json!({"type":"discard","order":0}),
            ),
            (
                Action::Clue {
                    target: Seat(1),
                    clue: clue(ClueValue::Rank(1)),
                },
                json!({"type":"clue","receiverIndex":1,"clue":{"type":"rank","rank":1}}),
            ),
            (
                Action::Clue {
                    target: Seat(1),
                    clue: clue(ClueValue::Color(2)),
                },
                json!({"type":"clue","receiverIndex":1,"clue":{"type":"color","color":2}}),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(game.action_message(42, action)).unwrap(),
                json!({"tableId":42,"action":expected})
            );
        }
    }

    #[test]
    fn reconnect_replays_current_history_and_resumes_playing() {
        let (mut bot, mut server) = connection();
        let mut game_init = init(json!({"type":"playing","ourPlayerIndex":1}));
        game_init.player_names = vec!["Alice".into(), "test bot".into(), "Bob".into()];
        bot.process_message("init", &serde_json::to_string(&game_init).unwrap())
            .unwrap();
        assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));
        // The reported reconnect payload, with only the table ID normalized.
        let history = include_str!("../../tests/fixtures/hanabi-live-reconnect.json");
        bot.process_message("gameState", history).unwrap();
        assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
        let action: ActionMessage =
            serde_json::from_value(receive(&mut server, "playerAction")).unwrap();
        assert!(
            !matches!(action.action, PlayerAction::Clue { .. }),
            "no tokens remain"
        );
        let game = bot.game.as_ref().unwrap();
        assert_eq!(
            (
                game.status.turn,
                game.status.score,
                game.status.clues,
                game.status.num_strikes
            ),
            (34, 12, 0, 2)
        );
        assert_eq!(game.status.max_score, 25);
        assert_eq!(
            game.hands[1]
                .iter()
                .map(|slot| slot.index)
                .collect::<Vec<_>>(),
            vec![31, 23, 18, 8, 6]
        );
        bot.act().unwrap();
        assert_no_message(&mut server);
    }

    #[test]
    fn current_and_legacy_clues_load_from_history_and_live_events() {
        for legacy in [false, true] {
            let (mut bot, mut server) = connection();
            bot.process_message(
                "gameInit",
                &serde_json::to_string(&init(json!({"type":"playing","ourPlayerIndex":0})))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(receive(&mut server, "gameStateLoad"), json!({"tableId":42}));

            let rank_field = if legacy { "value" } else { "rank" };
            let color_field = if legacy { "value" } else { "color" };
            let rank_clue = json!({"type":"clue","playerIndex":0,"receiverIndex":1,
                "orders":[5],"clue":{"type":"rank",rank_field:1}});
            let color_clue = json!({"type":"clue","playerIndex":0,"receiverIndex":1,
                "orders":[5,6,7,8,9],"clue":{"type":"color",color_field:0}});
            let mut events = initial_events();
            events.push(rank_clue.clone());
            events.push(color_clue.clone());
            events.push(json!({"type":"turn","turn":3,"currentPlayerIndex":1}));
            bot.process_message(
                "gameState",
                &json!({"tableId":42,"events":events}).to_string(),
            )
            .unwrap();
            assert_eq!(receive(&mut server, "gameLoaded"), json!({"tableId":42}));
            assert_eq!(bot.game.as_ref().unwrap().status.clues, 6);

            for event in [rank_clue, color_clue] {
                bot.process_message(
                    "gameEvent",
                    &json!({"tableId":42,"event":event}).to_string(),
                )
                .unwrap();
            }
            assert_eq!(bot.game.as_ref().unwrap().status.clues, 4);
            // Both formats produce the current wire format when serialized.
            for (history_index, live_index, clue) in [
                (10, 13, json!({"type":"rank","rank":1})),
                (11, 14, json!({"type":"color","color":0})),
            ] {
                for index in [history_index, live_index] {
                    assert_eq!(
                        serde_json::to_value(&bot.game_actions[index]).unwrap()["clue"],
                        clue
                    );
                }
            }
            assert_no_message(&mut server);
        }
    }

    #[test]
    fn current_and_legacy_events_update_the_same_game_state() {
        for (receiver_field, failed_event, rank_field) in [
            ("receiverIndex", "misplay", "rank"),
            ("target", "discard", "value"),
        ] {
            let mut game = dealt_game();
            let clue = json!({"type":"clue","playerIndex":0,receiver_field:1,"orders":[5],"clue":{"type":"rank",rank_field:1}});
            game_event(&mut game, clue);
            game_event(
                &mut game,
                json!({"type":failed_event,"failed":true,"playerIndex":1,"order":9,"identity":{"suitIndex":0,"rank":5}}),
            );
            assert_eq!(
                (
                    game.status.clues,
                    game.status.num_strikes,
                    game.status.max_score
                ),
                (7, 1, 24)
            );
            game_event(
                &mut game,
                json!({"type":"discard","playerIndex":1,"order":8,"identity":{"suitIndex":0,"rank":4}}),
            );
            assert_eq!((game.status.clues, game.status.num_strikes), (8, 1));
        }
    }

    #[test]
    fn events_maintain_clues_scores_strikes_and_achievable_score() {
        let mut game = dealt_game();
        game_event(
            &mut game,
            json!({"type":"clue","playerIndex":0,"target":1,"orders":[5],"clue":{"type":"rank","value":1}}),
        );
        assert_eq!(game.status.clues, 7);
        game_event(
            &mut game,
            json!({"type":"play","playerIndex":1,"order":5,"identity":{"suitIndex":0,"rank":1}}),
        );
        assert_eq!((game.status.score, game.status.clues), (1, 7));
        game_event(
            &mut game,
            json!({"type":"discard","playerIndex":1,"order":9,"identity":{"suitIndex":0,"rank":5},"failed":false}),
        );
        assert_eq!((game.status.clues, game.status.max_score), (8, 24));
        game_event(
            &mut game,
            json!({"type":"discard","playerIndex":1,"order":8,"identity":{"suitIndex":0,"rank":4},"failed":true}),
        );
        assert_eq!(
            (
                game.status.num_strikes,
                game.status.clues,
                game.status.max_score
            ),
            (1, 8, 24)
        );
        game_event(
            &mut game,
            json!({"type":"draw","playerIndex":1,"order":10,"identity":{"suitIndex":0,"rank":4}}),
        );
        game_event(
            &mut game,
            json!({"type":"discard","playerIndex":1,"order":10,"identity":{"suitIndex":0,"rank":4},"failed":false}),
        );
        assert_eq!((game.status.clues, game.status.max_score), (8, 23));
        game.reset();
        assert_eq!(
            (
                game.status.score,
                game.status.num_strikes,
                game.status.max_score,
                game.status.clues
            ),
            (0, 0, 25, 8)
        );
        assert_eq!(game.state.discarded, [0; raikan::domain::MAX_IDENTITIES]);
    }

    #[test]
    fn completing_a_stack_restores_a_clue() {
        let mut game = dealt_game();
        game_event(
            &mut game,
            json!({"type":"clue","playerIndex":0,"target":1,"orders":[9],"clue":{"type":"rank","value":5}}),
        );
        for rank in 1..=5 {
            game_event(
                &mut game,
                json!({"type":"play","playerIndex":1,"order":rank+4,"identity":{"suitIndex":0,"rank":rank}}),
            );
        }
        assert_eq!((game.status.score, game.status.clues), (5, 8));
    }

    #[test]
    fn spectators_replays_and_finished_games_do_not_act() {
        for mode in [
            json!({"type":"spectating"}),
            json!({"type":"shadowing","ourPlayerIndex":1}),
            json!({"type":"replay","ourPlayerIndex":0}),
            json!({"type":"soloReplay","gameId":123,"ourPlayerIndex":0}),
            json!({"type":"soloReplay"}),
            json!({"type":"sharedReplay","ourPlayerIndex":0,"sharedReplay":{"turn":0}}),
        ] {
            let mut game = HanabGame::from_message(&init(mode)).unwrap();
            game.current_player_index = Some(game.own_player);
            assert!(game.act(42).is_none());
        }
        let mut game = dealt_game();
        game_event(
            &mut game,
            json!({"type":"gameOver","endCondition":"normal"}),
        );
        game_event(
            &mut game,
            json!({"type":"cardIdentity","playerIndex":0,"order":0,"identity":{"suitIndex":1,"rank":1}}),
        );
        game_event(
            &mut game,
            json!({"type":"playerTimes","playerTimesMs":[100,100],"durationMs":200}),
        );
        game.current_player_index = Some(0);
        assert!(game.act(42).is_none());
        game.reset();
        game.current_player_index = Some(0);
        assert!(game.act(42).is_none());
    }

    #[test]
    fn replay_cursor_uses_nested_actions_and_ignores_other_tables() {
        let (mut bot, _server) = connection();
        bot.table_id = 42;
        bot.process_message(
            "replayAction",
            r#"{"tableId":42,"action":{"type":"turn","turn":7}}"#,
        )
        .unwrap();
        bot.process_message(
            "replayAction",
            r#"{"tableId":99,"action":{"type":"turn","turn":9}}"#,
        )
        .unwrap();
        bot.process_message(
            "replayAction",
            r#"{"tableId":42,"action":{"type":"arrow","order":0}}"#,
        )
        .unwrap();
        assert_eq!(bot.replay_segment, Some(7));
    }
}
