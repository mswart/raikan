use std::collections::{BTreeMap, VecDeque};

use raikan::game;
use raikan::{PositionSet, Variant, game::PlayerStrategy, hyphenated};
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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WelcomeMessage {
    #[serde(alias = "userID")]
    user_id: usize,
    username: String,
    muted: bool,
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
    Pregame { table_id: usize },
    Playing { table_id: usize },
    Spectating { table_id: usize },
    SharedReplay { table_id: usize },
    Replay,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TableState {
    table_id: usize,
    joined: bool,
    phase: String,
    player_names: Vec<String>,
}

#[derive(Deserialize)]
struct TableListMessage {
    tables: Vec<TableState>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    text: String,
    source: String,
    username: Option<String>,
    table_id: Option<usize>,
    recipient: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InitMessage {
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
    variant_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameActionListMessage {
    table_id: u64,
    events: Vec<GameAction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameActionMessage {
    table_id: u64,
    event: GameAction,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClueMessage {
    Color { value: u8 },
    Rank { value: u8 },
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
                        let (command, json) = text
                            .split_once(' ')
                            .expect("Websocket message must be 'command JSON'");
                        match self.process_message(command, json) {
                            Ok(_) => {}
                            Err(error) => {
                                println!(
                                    "Failed to process {command} message: {error} from {json}"
                                );
                            }
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
            "userLeft" => self.on_user_left(json),

            "tableList" => self.on_table_list(json),
            "table" => self.on_table(json),
            "tableGone" => self.on_table_gone(json),

            "tableJoined" => self.on_joined(json),
            "tableLeft" | "tableBoot" => self.on_left(json),
            "tableStarted" => self.on_table_start(json),
            "init" => self.on_init(json),
            "gameEventList" => self.on_game_action_list(json),
            "gameEvent" => self.on_action(json),

            "replayAction" => self.on_replay_segment(json),
            "clock" | "connected" | "noteList" | "noteListPlayer" | "spectators"
            | "cardIdentities" | "gameFinished" | "hypothetical" => Ok(()),

            _ => {
                println!("Recived message: {command} / {json}");
                Ok(())
            }
        }
    }

    fn on_welcome(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let welcome: WelcomeMessage = serde_json::from_str(json)?;
        println!("Welcome {:?}", welcome);
        self.user_id = welcome.user_id;
        self.username = welcome.username;
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
        // Prefer an active membership over a replay when reconnecting.
        let selected = tables.tables.iter().find(|table| table.joined).or_else(|| {
            tables.tables.iter().find(|table| {
                table.phase == "sharedReplay" && table.player_names.contains(&self.username)
            })
        });
        if self.table_id == 0 {
            if let Some(table) = selected {
                self.table_id = table.table_id;
                let command = if table.joined {
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
        for table in tables.tables {
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
        if chat.source != "user" {
            return Ok(());
        }
        if chat.recipient.as_deref() == Some(self.username.as_str()) {
            if let "/join" = chat.text.as_str() {
                if let Some((_user, user)) = self.user_states.iter().find(|(_user_id, user)| {
                    Some(user.username.as_str()) == chat.username.as_deref()
                }) {
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
                                .is_some_and(|table| table.joined) =>
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
                        UserActivity::Lobby | UserActivity::Replay => {}
                    }
                }
            } else {
                eprintln!("Unknown message content {}", chat.text);
            }
        } else if chat.table_id == Some(self.table_id) {
            if let "dump" = chat.text.as_str() {
                if let Some(game) = &mut self.game {
                    if let Some(segment) = self.replay_segment {
                        game.reset();
                        for action in self.game_actions.iter() {
                            game.process_action(action);
                            if game.status.turn >= segment {
                                break;
                            }
                        }
                        println!("player: {:?}", game.player);
                        println!("line: {:?}", game.player.line());
                    }
                }
            }
        }
        Ok(())
    }

    fn on_joined(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.table_id = table.table_id;
        Ok(())
    }

    fn on_left(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        if table.table_id == self.table_id {
            self.table_id = 0;
            self.game = None;
            self.game_actions.clear();
            self.replay_segment = None;
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
        self.table_id = init.table_id as usize;
        let table_id = TableIdMessage {
            table_id: init.table_id as usize,
        };
        if init.options.variant_name != "No Variant" {
            self.game = None;
            eprintln!("Unsupported game variant: {}", init.options.variant_name);
            return Ok(());
        }
        self.game = Some(HanabGame::from_message(&init));
        self.game_actions = Vec::with_capacity(256);
        self.replay_segment = None;
        self.send("gameStateLoad", &serde_json::to_string(&table_id)?);
        Ok(())
    }

    fn on_game_action_list(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let action_list: GameActionListMessage = serde_json::from_str(json)?;
        if action_list.table_id != self.table_id as u64 {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            game.reset();
            self.game_actions.clear();
            for action in action_list.events.iter() {
                game.process_action(action);
                self.game_actions.push(action.clone());
            }
            let table_id = TableIdMessage {
                table_id: action_list.table_id as usize,
            };
            self.send("loaded", &serde_json::to_string(&table_id)?);
            self.act()?;
        } else {
            eprintln!("Retrieved game action list but I aren't in a game");
        }
        Ok(())
    }

    fn on_action(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let action: GameActionMessage = serde_json::from_str(json)?;
        if action.table_id != self.table_id as u64 {
            return Ok(());
        }
        if let Some(game) = &mut self.game {
            game.process_action(&action.event);
            self.game_actions.push(action.event);
            self.act()?;
        } else {
            println!("Retrieved game action but I aren't in a game");
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
        println!("==> {command} {body}");
        let message = Message::Text(format!("{} {}", command, body).into());
        match self.client.send(message) {
            Ok(()) => {}
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

struct HanabGame {
    player_names: Vec<String>,
    own_player: u8,
    hands: Vec<VecDeque<Slot>>,
    player: hyphenated::HyphenatedPlayer,
    variant: Variant,
    current_player_index: Option<u8>,
    can_act: bool,
    discarded_copies: [[u8; 5]; 5],
    status: game::GameStatus,
}

impl HanabGame {
    fn from_message(init: &InitMessage) -> Self {
        let mut hands = Vec::new();
        for _ in 0..init.player_names.len() {
            hands.push(VecDeque::new());
        }

        let mut player = hyphenated::HyphenatedPlayer::new(true);
        let own_player = init.mode.our_player_index.unwrap_or(0);
        player.init(init.player_names.len() as u8, own_player);

        Self {
            player_names: init.player_names.clone(),
            hands,
            own_player,
            player,
            variant: Variant {},
            current_player_index: Some(0),
            can_act: init.mode.kind == "playing" && init.mode.our_player_index.is_some(),
            discarded_copies: [[0; 5]; 5],
            status: game::GameStatus {
                turn: 0,
                score: 0,
                max_score: 25,
                num_strikes: 0,
                clues: 8,
                blind_plays: 0,
            },
        }
    }

    fn reset(&mut self) {
        self.hands = Vec::new();
        for _ in 0..self.player_names.len() {
            self.hands.push(VecDeque::new());
        }

        self.player = hyphenated::HyphenatedPlayer::new(true);
        self.player
            .init(self.player_names.len() as u8, self.own_player);
        // New games contain only draw events: player zero must act before the
        // server emits its first turn event. Later history events override this
        // opening turn, and the client waits for the full history before acting.
        self.current_player_index = Some(0);
        self.discarded_copies = [[0; 5]; 5];
        self.status = game::GameStatus {
            turn: 0,
            score: 0,
            max_score: 25,
            num_strikes: 0,
            clues: 8,
            blind_plays: 0,
        };
    }

    fn resolve_index(&self, player: u8) -> u8 {
        let num_player = self.player_names.len() as u8;
        (player + num_player - self.own_player) % num_player
    }

    fn act(&mut self, table_id: u64) -> Option<ActionMessage> {
        if !self.can_act || self.current_player_index != Some(self.own_player) {
            return None;
        }
        self.current_player_index = None;
        let action = self.player.act(&self.status);
        println!("Decision: {:?}", action);
        Some(self.action_message(table_id, action))
    }

    fn action_message(&self, table_id: u64, action: game::Move) -> ActionMessage {
        let action = match action {
            game::Move::Play(pos) => PlayerAction::Play {
                order: self.hands[self.own_player as usize][pos as usize].index,
            },
            game::Move::Discard(pos) => PlayerAction::Discard {
                order: self.hands[self.own_player as usize][pos as usize].index,
            },
            game::Move::Clue(player, clue) => PlayerAction::Clue {
                target: (self.own_player + player) % self.player_names.len() as u8,
                clue: match clue {
                    game::Clue::Color(color) => ClueMessage::Color {
                        value: self
                            .variant
                            .suits()
                            .iter()
                            .position(|&s| s == color.suit())
                            .unwrap() as u8,
                    },
                    game::Clue::Rank(value) => ClueMessage::Rank { value },
                },
            },
        };
        ActionMessage { table_id, action }
    }

    fn process_action(&mut self, action: &GameAction) {
        println!("Action: {:?}", action);
        match action {
            GameAction::Draw {
                player_index,
                order,
                identity,
            } => {
                if *player_index == self.own_player {
                    self.hands[*player_index as usize].push_front(Slot { index: *order });
                    self.player.own_drawn();
                } else if let Some(identity) = identity {
                    let card = game::Card {
                        suit: self.variant.suits()[identity.suit_index as usize],
                        rank: identity.rank,
                    };
                    self.hands[*player_index as usize].push_front(Slot { index: *order });
                    self.player
                        .drawn(self.resolve_index(*player_index) as usize, card);
                } else {
                    eprintln!("Missing identity for another player's drawn card");
                }
            }
            GameAction::Play {
                order,
                player_index,
                identity,
            } => {
                if let Some(slot_pos) = self.hands[*player_index as usize]
                    .iter()
                    .position(|slot| slot.index == *order)
                {
                    let card = game::Card {
                        suit: self.variant.suits()[identity.suit_index as usize],
                        rank: identity.rank,
                    };
                    self.player.played(
                        self.resolve_index(*player_index) as usize,
                        slot_pos,
                        card,
                        true,
                    );
                    self.hands[*player_index as usize].remove(slot_pos);
                    self.status.score += 1;
                    if identity.rank == 5 {
                        self.status.clues = (self.status.clues + 1).min(8);
                    }
                    println!(
                        "Player {} succesfully played {card:?} from slot {slot_pos}",
                        self.player_names[*player_index as usize]
                    )
                } else {
                    eprintln!("Did not find card that should be played");
                }
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
                if let Some(slot_pos) = self.hands[*player_index as usize]
                    .iter()
                    .position(|slot| slot.index == *order)
                {
                    let card = game::Card {
                        suit: self.variant.suits()[identity.suit_index as usize],
                        rank: identity.rank,
                    };
                    if failed {
                        self.status.num_strikes += 1;
                        self.player.played(
                            self.resolve_index(*player_index) as usize,
                            slot_pos,
                            card,
                            false,
                        );
                        println!(
                            "Player {} failed to played {card:?} from slot {slot_pos}",
                            self.player_names[*player_index as usize]
                        )
                    } else {
                        self.status.clues = (self.status.clues + 1).min(8);
                        self.player.discarded(
                            self.resolve_index(*player_index) as usize,
                            slot_pos,
                            card,
                        );
                        println!(
                            "Player {} discard {card:?} from slot {slot_pos}",
                            self.player_names[*player_index as usize]
                        )
                    }
                    self.hands[*player_index as usize].remove(slot_pos);
                    self.discarded_copies[identity.suit_index as usize]
                        [identity.rank as usize - 1] += 1;
                    // The new protocol has no status or strike events. Derive
                    // counters and the achievable score from public events.
                    self.status.max_score = self
                        .discarded_copies
                        .iter()
                        .map(|copies| {
                            copies
                                .iter()
                                .enumerate()
                                .find_map(|(rank, &count)| {
                                    let total = card.suit.card_count(rank as u8 + 1);
                                    (count >= total).then_some(rank as u8)
                                })
                                .unwrap_or(5)
                        })
                        .sum();
                } else {
                    eprintln!("Did not find card that should be discarded");
                }
            }
            GameAction::Clue {
                clue: clue_message,
                player_index: giver,
                orders,
                target,
                ..
            } => {
                let mut touched = PositionSet::new(self.hands[*target as usize].len() as u8);
                for (pos, slot) in self.hands[*target as usize].iter().enumerate() {
                    if orders.contains(&slot.index) {
                        touched.add(pos as u8);
                    }
                }
                let clue = match clue_message {
                    ClueMessage::Rank { value } => game::Clue::Rank(*value),
                    ClueMessage::Color { value } => {
                        game::Clue::Color(self.variant.suits()[*value as usize].clue_color())
                    }
                };
                self.status.clues = self.status.clues.saturating_sub(1);
                self.player.clued(
                    self.resolve_index(*giver) as usize,
                    self.resolve_index(*target) as usize,
                    clue,
                    touched,
                );
            }
            GameAction::Turn {
                current_player_index,
                turn,
            } => {
                self.status.turn = *turn;
                self.current_player_index = *current_player_index;
            }
            GameAction::GameOver { end_condition } => {
                self.current_player_index = None;
                self.can_act = false;
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
            },
            WebSocket::from_raw_socket(server, Role::Server, None),
        )
    }

    fn receive(server: &mut WebSocket<TcpStream>, expected_command: &str) -> Value {
        let message = server.read().unwrap().into_text().unwrap();
        let (command, body) = message.split_once(' ').unwrap();
        assert_eq!(command, expected_command);
        serde_json::from_str(body).unwrap()
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
            "mode":mode, "options":{"variantName":"No Variant"},
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
        let mut game = HanabGame::from_message(&init(json!({"type":"playing","ourPlayerIndex":0})));
        for event in initial_events() {
            game_event(&mut game, event);
        }
        game
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
                "settings":{"name":"Test table","options":{"variantName":"No Variant"}},
                "progress":50,"numPlayers":2}]}),
            ),
            ("lobbyStatus", json!({"type":"loading"})),
            (
                "chat",
                json!({"text":"Welcome!","source":"server","sentAt":1234}),
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
        assert_eq!(bot.table_states[&42].phase, "pregame");
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
            r#"{"text":"/join","source":"server","username":"Alice","recipient":"test bot"}"#,
        )
        .unwrap();
        assert_eq!(bot.table_id, 0);
        bot.process_message(
            "chat",
            r#"{"text":"/join","source":"user","username":"Alice","recipient":"test bot"}"#,
        )
        .unwrap();
        assert_eq!(
            receive(&mut server, "tableJoin"),
            json!({"tableId":42,"password":"secret"})
        );
        bot.process_message("tableJoined", r#"{"tableId":42}"#)
            .unwrap();
        assert_eq!(bot.table_id, 42);

        bot.process_message("user", r#"{"userId":7,"username":"Alice","activity":{"type":"spectating","tableId":43},"inactive":false}"#).unwrap();
        bot.process_message(
            "chat",
            r#"{"text":"/join","source":"user","username":"Alice","recipient":"test bot"}"#,
        )
        .unwrap();
        assert_eq!(receive(&mut server, "tableSpectate"), json!({"tableId":43}));
    }

    #[test]
    fn reconnect_prefers_joined_game_over_old_replays() {
        let (mut bot, mut server) = connection();
        bot.process_message("tableList", &json!({"tables":[
            {"tableId":41,"joined":false,"phase":"sharedReplay","playerNames":["test bot","Alice"]},
            {"tableId":42,"joined":true,"phase":"playing","playerNames":["test bot","Alice"]}
        ]}).to_string()).unwrap();
        assert_eq!(receive(&mut server, "tableReattend"), json!({"tableId":42}));
        assert_eq!(bot.table_id, 42);
        assert_eq!(bot.table_states.len(), 2);
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
            assert_eq!(receive(&mut server, "loaded"), json!({"tableId":42}));
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
            bot.process_message("gameEventList", &history).unwrap();
            assert_eq!(receive(&mut server, "loaded"), json!({"tableId":42}));
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
        let mut game = dealt_game();
        for (action, expected) in [
            (game::Move::Play(0), json!({"type":"play","order":4})),
            (game::Move::Discard(4), json!({"type":"discard","order":0})),
            (
                game::Move::Clue(1, game::Clue::Rank(1)),
                json!({"type":"clue","receiverIndex":1,"clue":{"type":"rank","value":1}}),
            ),
            (
                game::Move::Clue(1, game::Clue::Color(game::Suit::Green().clue_color())),
                json!({"type":"clue","receiverIndex":1,"clue":{"type":"color","value":2}}),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(game.action_message(42, action)).unwrap(),
                json!({"tableId":42,"action":expected})
            );
        }
        game.own_player = 1;
        assert_eq!(
            serde_json::to_value(game.action_message(42, game::Move::Clue(1, game::Clue::Rank(2))))
                .unwrap()["action"]["receiverIndex"],
            0
        );
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
        bot.process_message("gameEventList", history).unwrap();
        assert_eq!(receive(&mut server, "loaded"), json!({"tableId":42}));
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
    fn current_and_legacy_events_update_the_same_game_state() {
        for (receiver_field, failed_event) in [("receiverIndex", "misplay"), ("target", "discard")]
        {
            let mut game = dealt_game();
            let clue = json!({"type":"clue","playerIndex":0,receiver_field:1,"orders":[5],"clue":{"type":"rank","value":1}});
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
        assert_eq!(game.discarded_copies, [[0; 5]; 5]);
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
            json!({"type":"sharedReplay","ourPlayerIndex":0,"sharedReplay":{"turn":0}}),
        ] {
            let mut game = HanabGame::from_message(&init(mode));
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
