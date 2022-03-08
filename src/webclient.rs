use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use websocket::sync::stream::NetworkStream;

use crate::game;
use crate::{card_quantum, game::PlayerStrategy, hyphenated, PositionSet};

pub fn run() {
    let session_id_env = std::env::var("HANABI_SID");
    let mut headers = websocket::header::Headers::new();
    match session_id_env {
        Ok(message) => headers.set(websocket::header::Cookie(vec![message])),
        Err(_error) => {
            let username =
                std::env::var("HANABI_USERNAME").expect("HANABI_USERNAME environment is required");
            let password =
                std::env::var("HANABI_PASSWORD").expect("HANABI_PASSWORD environment is required");

            let params = [
                ("username", username),
                ("password", password),
                ("version", "bot".to_string()),
            ];
            let client = reqwest::blocking::Client::new();
            let res = client
                .post("https://hanab.live/login")
                .form(&params)
                .send()
                .expect("Failed to login into hanab.live");
            assert_eq!(
                res.status(),
                reqwest::StatusCode::OK,
                "Login request was unsuccessful"
            );
            for (name, value) in res.headers() {
                if name != reqwest::header::SET_COOKIE {
                    continue;
                }
                headers.set(websocket::header::Cookie(vec![value
                    .to_str()
                    .expect("cookie data should be ascii")
                    .to_owned()]));
            }
        }
    }

    let mut client = HanabClient {
        client: websocket::ClientBuilder::new("wss://hanab.live/ws")
            .unwrap()
            .custom_headers(&headers)
            .connect(None)
            .unwrap(),
        user_id: 0,
        username: "".to_string(),
        table_id: 0,
        user_states: BTreeMap::new(),
        table_states: BTreeMap::new(),
        game: None,
    };
    client.run();
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WelcomeMessage {
    #[serde(rename = "userID")]
    user_id: usize,
    username: String,
    total_games: usize,
    muted: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct UserState {
    #[serde(rename = "userID")]
    user_id: usize,
    name: String,
    status: u8,
    #[serde(rename = "tableID")]
    table_id: usize,
    hyphenated: bool,
    inactive: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UserInactiveMessage {
    #[serde(rename = "userID")]
    user_id: usize,
    inactive: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UserLeftMessage {
    #[serde(rename = "userID")]
    user_id: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct TableIdMessage {
    #[serde(rename = "tableID")]
    table_id: usize,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SpectateMessage {
    #[serde(rename = "tableID")]
    table_id: usize,
    shadowing_player_index: i8,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TableJoinMessage {
    #[serde(rename = "tableID")]
    table_id: usize,
    password: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TableState {
    id: usize,
    name: String,
    password_protected: bool,
    joined: bool,
    num_players: u8,
    owned: bool,
    running: bool,
    variant: String,
    timed: bool,
    time_base: i16,
    time_per_turn: i16,
    shared_replay: bool,
    progress: i8,
    players: Vec<String>,
    spectators: Vec<String>,
    max_players: u8,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    msg: String,
    who: String,
    discord: bool,
    server: bool,
    room: String,
    recipient: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InitMessage {
    #[serde(rename = "tableID")]
    table_id: u64,
    player_names: Vec<String>,
    our_player_index: u8,
    spectating: bool,
    replay: bool,
    shared_replay: bool,
    shared_replay_leader: String,
    paused: bool,
    pause_player_index: i8,
    pause_queued: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameActionListMessage {
    #[serde(rename = "tableID")]
    table_id: u64,
    list: Vec<GameAction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GameActionMessage {
    #[serde(rename = "tableID")]
    table_id: u64,
    action: GameAction,
}

#[derive(Serialize, Deserialize, Debug)]
struct ClueMessage {
    #[serde(rename = "type")]
    kind: u8,
    value: u8,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum GameAction {
    #[serde(rename = "draw")]
    #[serde(rename_all = "camelCase")]
    Draw {
        player_index: u8,
        order: u8,
        suit_index: i8,
        rank: i8,
    },
    #[serde(rename = "clue")]
    Clue {
        giver: u8,
        turn: u8,
        target: u8,
        list: Vec<u8>,
        clue: ClueMessage,
    },
    #[serde(rename = "status")]
    #[serde(rename_all = "camelCase")]
    Status { clues: u8, max_score: u8, score: u8 },
    #[serde(rename_all = "camelCase")]
    #[serde(rename = "turn")]
    Turn { num: u8, current_player_index: i8 },
    #[serde(rename = "play")]
    #[serde(rename_all = "camelCase")]
    Play {
        order: u8,
        player_index: u8,
        rank: u8,
        suit_index: u8,
    },
    #[serde(rename = "discard")]
    #[serde(rename_all = "camelCase")]
    Discard {
        failed: bool,
        order: u8,
        player_index: u8,
        rank: u8,
        suit_index: u8,
    },
    #[serde(rename = "strike")]
    Strike { num: u8 },
    #[serde(rename = "gameOver")]
    #[serde(rename_all = "camelCase")]
    GameOver { end_condition: u8, player_index: i8 },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ActionMessage {
    #[serde(rename = "tableID")]
    table_id: u64,
    #[serde(rename = "type")]
    action: u8,
    target: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u8>,
}

struct HanabClient {
    client: websocket::sync::Client<Box<dyn NetworkStream + Send>>,
    user_id: usize,
    username: String,
    user_states: std::collections::BTreeMap<usize, UserState>,
    table_states: std::collections::BTreeMap<usize, TableState>,
    table_id: usize,
    game: Option<HanabGame>,
}

impl HanabClient {
    fn run(&mut self) {
        loop {
            let message = self.client.recv_message();
            match message {
                Ok(message) => match message {
                    websocket::OwnedMessage::Ping(id) => {
                        match self
                            .client
                            .send_message(&websocket::OwnedMessage::Pong(id.clone()))
                        {
                            Ok(_) => {}
                            Err(error) => {
                                println!("Failed to send pong websocket message: {}", error)
                            }
                        }
                    }
                    websocket::OwnedMessage::Text(text) => {
                        let (command, json) = text
                            .split_once(" ")
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
                        println!("Received unknown websocket message (implemented are text and ping): {message:?}");
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

            "userList" => self.on_user_list(json),
            "user" => self.on_user(json),
            "userInactive" => self.on_user_inactive(json),
            "userLeft" => self.on_user_left(json),

            "tableList" => self.on_table_list(json),
            "table" => self.on_table(json),
            "tableGone" => self.on_table_gone(json),

            "joined" => self.on_joined(json),
            "tableStart" => self.on_table_start(json),
            "init" => self.on_init(json),
            "gameActionList" => self.on_game_action_list(json),
            "gameAction" => self.on_action(json),
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
        let warning = serde_json::from_str(json)?;
        println!("Warning {:?}", warning);
        Ok(())
    }

    fn on_warning(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let warning = serde_json::from_str(json)?;
        println!("Warning {:?}", warning);
        Ok(())
    }

    fn on_user_list(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let users: Vec<UserState> = serde_json::from_str(json)?;
        for user in users.iter() {
            self.user_states.insert(user.user_id, user.clone());
        }
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
        let tables: Vec<TableState> = serde_json::from_str(json)?;
        for table in tables.iter() {
            self.table_states.insert(table.id, table.clone());
            if table.joined {
                println!("Found existing table: {:?}", table);
                self.table_id = table.id;
                let table_ref = TableIdMessage { table_id: table.id };
                self.send("tableReattend", &serde_json::to_string(&table_ref)?);
            }
        }
        Ok(())
    }

    fn on_table(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableState = serde_json::from_str(json)?;
        self.table_states.insert(table.id, table);
        Ok(())
    }

    fn on_table_gone(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.table_states.remove(&table.table_id);
        Ok(())
    }

    fn on_chat(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let chat: ChatMessage = serde_json::from_str(json)?;
        if chat.recipient != self.username {
            return Ok(());
        }
        if let "/join" = chat.msg.as_str() {
            if let Some((_user, user)) = self
                .user_states
                .iter()
                .find(|(_user_id, user)| user.name == chat.who)
            {
                println!("Join request by {}: {:?}", chat.who, user);
                if user.table_id > 0 {
                    self.table_id = user.table_id;
                    if user.status == 5 {
                        let table_ref = SpectateMessage {
                            table_id: user.table_id,
                            shadowing_player_index: -1,
                        };
                        self.send("tableSpectate", &serde_json::to_string(&table_ref)?);
                    } else if user.status == 2 {
                        let table_ref = TableIdMessage {
                            table_id: user.table_id,
                        };
                        self.send("tableReattend", &serde_json::to_string(&table_ref)?);
                    } else {
                        let table_ref = TableJoinMessage {
                            table_id: user.table_id,
                            password: "bot".to_string(),
                        };
                        self.send("tableJoin", &serde_json::to_string(&table_ref)?);
                    }
                }
            }
        } else {
            eprintln!("Unknown message content {}", chat.msg);
        }
        Ok(())
    }

    // todo chat
    // {"msg":"test","who":"msw-debug1","discord":false,"server":false,"datetime":"2022-03-08T16:49:55.796705296Z","room":"table17397","recipient":""}
    // tableProgress
    fn on_joined(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.table_id = table.table_id;
        Ok(())
    }

    fn on_table_start(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let table: TableIdMessage = serde_json::from_str(json)?;
        self.send("getGameInfo1", &serde_json::to_string(&table)?);
        Ok(())
    }

    fn on_init(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let init: InitMessage = serde_json::from_str(json)?;
        self.table_id = init.table_id as usize;
        let table_id = TableIdMessage {
            table_id: init.table_id as usize,
        };
        self.game = Some(HanabGame::from_message(&init));
        self.send("getGameInfo2", &serde_json::to_string(&table_id)?);
        Ok(())
    }

    fn on_game_action_list(&mut self, json: &str) -> Result<(), serde_json::Error> {
        if let Some(game) = &mut self.game {
            let action_list: GameActionListMessage = serde_json::from_str(json)?;
            for action in action_list.list.iter() {
                game.process_action(action);
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
        if let Some(game) = &mut self.game {
            game.process_action(&action.action);
            self.act()?;
        } else {
            println!("Retrieved game action but I aren't in a game");
        }
        Ok(())
    }

    fn act(&mut self) -> Result<(), serde_json::Error> {
        if let Some(game) = &mut self.game {
            if let Some(action) = game.act(self.table_id as u64) {
                // wait a bit to simulate thinking ...
                let duration = std::time::Duration::from_secs(1);
                std::thread::sleep(duration);
                self.send("action", &serde_json::to_string(&action)?);
            }
        }
        Ok(())
    }

    fn send(&mut self, command: &str, body: &str) {
        println!("==> {command} {body}");
        let message = websocket::OwnedMessage::Text(format!("{} {}", command, body));
        match self.client.send_message(&message) {
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
    clued: bool,
}

struct HanabGame {
    player_names: Vec<String>,
    own_player: u8,
    hands: Vec<VecDeque<Slot>>,
    player: hyphenated::HyphenatedPlayer,
    variant: card_quantum::Variant,
    current_player_index: Option<u8>,
    status: game::GameStatus,
}

impl HanabGame {
    fn from_message(init: &InitMessage) -> Self {
        let mut hands = Vec::new();
        for _ in 0..init.player_names.len() {
            hands.push(VecDeque::new());
        }

        let mut player = hyphenated::HyphenatedPlayer::new(true);
        player.init(init.player_names.len() as u8);

        Self {
            player_names: init.player_names.clone(),
            hands: hands,
            own_player: init.our_player_index,
            player,
            variant: card_quantum::Variant {},
            current_player_index: Some(0),
            status: game::GameStatus {
                turn: 0,
                score: 0,
                max_score: 25,
                num_strikes: 0,
                clues: 8,
            },
        }
    }

    fn resolve_index(&self, player: u8) -> u8 {
        let num_player = self.player_names.len() as u8;
        (player + num_player - self.own_player) % num_player
    }

    fn act(&mut self, table_id: u64) -> Option<ActionMessage> {
        if self.current_player_index != Some(self.own_player) {
            return None;
        }
        self.current_player_index = None;
        let action = self.player.act(&self.status);
        println!("Decision: {:?}", action);
        Some(match action {
            game::Move::Play(pos) => ActionMessage {
                table_id,
                action: 0,
                target: self.hands[self.own_player as usize][pos as usize].index,
                value: None,
            },
            game::Move::Discard(pos) => ActionMessage {
                table_id,
                action: 1,
                target: self.hands[self.own_player as usize][pos as usize].index,
                value: None,
            },
            game::Move::Clue(player, clue) => match clue {
                game::Clue::Color(color) => ActionMessage {
                    table_id,
                    action: 2,
                    target: (self.own_player + player) % self.player_names.len() as u8,
                    value: Some(
                        self.variant
                            .suits()
                            .iter()
                            .position(|&s| s == color.suit())
                            .unwrap() as u8,
                    ),
                },
                game::Clue::Rank(rank) => ActionMessage {
                    table_id,
                    action: 3,
                    target: (self.own_player + player) % self.player_names.len() as u8,
                    value: Some(rank),
                },
            },
        })
    }

    fn process_action(&mut self, action: &GameAction) {
        println!("Action: {:?}", action);
        match action {
            GameAction::Draw {
                player_index,
                order,
                suit_index,
                rank,
            } => {
                if *player_index == self.own_player {
                    self.hands[*player_index as usize].push_front(Slot {
                        index: *order,
                        clued: false,
                    });
                    self.player.own_drawn();
                } else {
                    if *suit_index < 0 || *rank < 0 {
                        eprintln!(
                            "Drawn card of other player {:?} does has an identity",
                            self.player_names[*player_index as usize],
                        );
                    } else {
                        let card = game::Card {
                            suit: self.variant.suits()[*suit_index as usize],
                            rank: *rank as u8,
                        };
                        self.hands[*player_index as usize].push_front(Slot {
                            index: *order,
                            clued: false,
                        });
                        self.player
                            .drawn(self.resolve_index(*player_index) as usize, card);
                    }
                }
            }
            GameAction::Status {
                clues,
                score,
                max_score,
            } => {
                self.status.clues = *clues;
                self.status.score = *score;
                self.status.max_score = *max_score;
                println!("Game status {clues} clues; score {score}/{max_score}");
            }
            GameAction::Play {
                order,
                player_index,
                rank,
                suit_index,
            } => {
                if let Some(slot_pos) = self.hands[*player_index as usize]
                    .iter()
                    .position(|slot| slot.index == *order)
                {
                    let card = game::Card {
                        suit: self.variant.suits()[*suit_index as usize],
                        rank: *rank,
                    };
                    self.player.played(
                        self.resolve_index(*player_index) as usize,
                        slot_pos,
                        card,
                        true,
                        !self.hands[*player_index as usize][slot_pos].clued,
                    );
                    self.hands[*player_index as usize].remove(slot_pos);
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
                rank,
                suit_index,
                failed,
            } => {
                if let Some(slot_pos) = self.hands[*player_index as usize]
                    .iter()
                    .position(|slot| slot.index == *order)
                {
                    let card = game::Card {
                        suit: self.variant.suits()[*suit_index as usize],
                        rank: *rank,
                    };
                    if *failed {
                        self.player.played(
                            self.resolve_index(*player_index) as usize,
                            slot_pos,
                            card,
                            false,
                            !self.hands[*player_index as usize][slot_pos].clued,
                        );
                        println!(
                            "Player {} failed to played {card:?} from slot {slot_pos}",
                            self.player_names[*player_index as usize]
                        )
                    } else {
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
                } else {
                    eprintln!("Did not find card that should be discarded");
                }
            }
            GameAction::Clue {
                clue: clue_message,
                giver,
                list,
                target,
                turn: _turn,
            } => {
                let mut touched = PositionSet::new(self.hands[*target as usize].len() as u8);
                let mut previously_clued =
                    PositionSet::new(self.hands[*target as usize].len() as u8);
                for (pos, slot) in self.hands[*target as usize].iter().enumerate() {
                    if slot.clued {
                        previously_clued.add(pos as u8);
                    }
                    if list.contains(&slot.index) {
                        touched.add(pos as u8);
                    }
                }
                let clue = if clue_message.kind == 1 {
                    println!(
                        "Player {} clued to {} {} touching {} cards",
                        self.player_names[*giver as usize],
                        self.player_names[*target as usize],
                        clue_message.value,
                        touched.len(),
                    );
                    game::Clue::Rank(clue_message.value)
                } else {
                    let clue_color = self.variant.suits()[clue_message.value as usize].clue_color();
                    println!(
                        "Player {} clued to {} {:?} touching {} cards",
                        self.player_names[*giver as usize],
                        self.player_names[*target as usize],
                        clue_color,
                        touched.len(),
                    );
                    game::Clue::Color(
                        self.variant.suits()[clue_message.value as usize].clue_color(),
                    )
                };
                self.player.clued(
                    self.resolve_index(*giver) as usize,
                    self.resolve_index(*target) as usize,
                    clue,
                    touched,
                    previously_clued,
                );
            }
            GameAction::Turn {
                current_player_index,
                num,
            } => {
                self.status.turn = *num;
                if *current_player_index >= 0 {
                    self.current_player_index = Some(*current_player_index as u8);
                } else {
                    self.current_player_index = None;
                }
            }
            GameAction::Strike { num } => {
                self.status.num_strikes += 1;
                println!("Strike {num}");
            }
            &GameAction::GameOver {
                end_condition,
                player_index: _player_index,
            } => {
                println!("Game finished: {end_condition}");
            }
        }
    }
}
