//! Replay import and export in hanab.live formats (JSON and `replay-json` URL).

use super::{Action, EndReason, World};
use crate::domain::{CardId, Seat};
use crate::variants::{ClueValue, Variant};
use serde::{Deserialize, Serialize};

const BASE62: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const URL_PREFIX: &str = "https://hanab.live/replay-json/";

/// One action of a replay as hanab.live stores it (absolute seats, card orders).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayAction {
    Action(Action),
    GameOver { seat: Seat, reason: EndReason },
}

/// A complete game record: variant, seats, deck and actions.
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub variant: String,
    pub players: Vec<String>,
    pub deck: World,
    pub actions: Vec<ReplayAction>,
}

#[derive(Debug)]
pub enum ReplayError {
    Json(serde_json::Error),
    Variant(crate::variants::VariantError),
    Format(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(err) => write!(f, "invalid replay JSON: {err}"),
            Self::Variant(err) => write!(f, "{err}"),
            Self::Format(msg) => write!(f, "invalid replay: {msg}"),
        }
    }
}

impl std::error::Error for ReplayError {}

#[derive(Serialize, Deserialize)]
struct JsonCard {
    #[serde(rename = "suitIndex")]
    suit_index: u8,
    rank: u8,
}

#[derive(Serialize, Deserialize)]
struct JsonAction {
    #[serde(rename = "type")]
    kind: u8,
    target: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    value: Option<u8>,
}

#[derive(Serialize, Deserialize)]
struct JsonOptions {
    variant: String,
}

#[derive(Serialize, Deserialize)]
struct JsonGame {
    players: Vec<String>,
    deck: Vec<JsonCard>,
    actions: Vec<JsonAction>,
    options: JsonOptions,
}

const PLAYER_NAMES: [&str; 6] = ["Alice", "Bob", "Cathy", "Donald", "Emily", "Frank"];

impl Replay {
    pub fn new(variant: &Variant, num_seats: u8, deck: World) -> Self {
        Self {
            variant: variant.name.clone(),
            players: PLAYER_NAMES[..num_seats as usize]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            deck,
            actions: Vec::new(),
        }
    }

    pub fn num_seats(&self) -> u8 {
        self.players.len() as u8
    }

    /// The game actions (without the game-over marker).
    pub fn game_actions(&self) -> impl Iterator<Item = Action> + '_ {
        self.actions.iter().filter_map(|a| match a {
            ReplayAction::Action(action) => Some(*action),
            ReplayAction::GameOver { .. } => None,
        })
    }

    fn encode_action(&self, variant: &Variant, action: &ReplayAction) -> JsonAction {
        match *action {
            ReplayAction::Action(Action::Play(card)) => JsonAction {
                kind: 0,
                target: card.0,
                value: None,
            },
            ReplayAction::Action(Action::Discard(card)) => JsonAction {
                kind: 1,
                target: card.0,
                value: None,
            },
            ReplayAction::Action(Action::Clue { target, clue }) => match variant.clue_value(clue) {
                ClueValue::Color(c) => JsonAction {
                    kind: 2,
                    target: target.0,
                    value: Some(c),
                },
                ClueValue::Rank(r) => JsonAction {
                    kind: 3,
                    target: target.0,
                    value: Some(r),
                },
            },
            ReplayAction::GameOver { seat, reason } => JsonAction {
                kind: 4,
                target: seat.0,
                value: Some(match reason {
                    EndReason::Normal => 1,
                    EndReason::Strikeout => 2,
                    EndReason::Illegal => 3,
                }),
            },
        }
    }

    fn decode_action(
        variant: &Variant,
        kind: u8,
        target: u8,
        value: Option<u8>,
    ) -> Result<ReplayAction, ReplayError> {
        let clue = |value: ClueValue| {
            variant
                .clue_id(value)
                .ok_or_else(|| ReplayError::Format(format!("clue {value:?} does not exist")))
        };
        let value_or = || value.ok_or_else(|| ReplayError::Format("clue without value".into()));
        Ok(match kind {
            0 => ReplayAction::Action(Action::Play(CardId(target))),
            1 => ReplayAction::Action(Action::Discard(CardId(target))),
            2 => ReplayAction::Action(Action::Clue {
                target: Seat(target),
                clue: clue(ClueValue::Color(value_or()?))?,
            }),
            3 => ReplayAction::Action(Action::Clue {
                target: Seat(target),
                clue: clue(ClueValue::Rank(value_or()?))?,
            }),
            4 => ReplayAction::GameOver {
                seat: Seat(target),
                reason: match value {
                    Some(2) => EndReason::Strikeout,
                    Some(1) | None => EndReason::Normal,
                    Some(_) => EndReason::Illegal,
                },
            },
            other => return Err(ReplayError::Format(format!("unknown action type {other}"))),
        })
    }

    pub fn to_json(&self) -> String {
        let variant = Variant::by_name(&self.variant).expect("replay variant must be supported");
        let game = JsonGame {
            players: self.players.clone(),
            deck: self
                .deck
                .as_slice()
                .iter()
                .map(|&i| JsonCard {
                    suit_index: variant.suit_of(i) as u8,
                    rank: variant.rank_of(i),
                })
                .collect(),
            actions: self
                .actions
                .iter()
                .map(|a| self.encode_action(&variant, a))
                .collect(),
            options: JsonOptions {
                variant: self.variant.clone(),
            },
        };
        serde_json::to_string(&game).expect("replay serialises")
    }

    pub fn from_json(json: &str) -> Result<Self, ReplayError> {
        let game: JsonGame = serde_json::from_str(json).map_err(ReplayError::Json)?;
        let variant = Variant::by_name(&game.options.variant).map_err(ReplayError::Variant)?;
        let mut deck = Vec::with_capacity(game.deck.len());
        for card in &game.deck {
            if card.suit_index as usize >= variant.num_suits()
                || !variant.ranks().any(|r| r == card.rank)
            {
                return Err(ReplayError::Format(format!(
                    "card {}/{} out of range",
                    card.suit_index, card.rank
                )));
            }
            deck.push(variant.identity(card.suit_index as usize, card.rank));
        }
        let actions = game
            .actions
            .iter()
            .map(|a| Self::decode_action(&variant, a.kind, a.target, a.value))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            variant: variant.name.clone(),
            players: game.players,
            deck: World::new(&deck),
            actions,
        })
    }

    /// The `https://hanab.live/replay-json/…` URL of this replay.
    pub fn to_url(&self) -> String {
        let variant = Variant::by_name(&self.variant).expect("replay variant must be supported");
        let mut url =
            String::with_capacity(URL_PREFIX.len() + 16 + self.deck.len() + 2 * self.actions.len());
        url.push_str(URL_PREFIX);
        url.push_str(&self.num_seats().to_string());
        url.push('1');
        // the rank range: 1..=stack size, or 1..=7 with START cards (rank 7)
        let max_rank = if variant.has_start() {
            crate::variants::START_RANK
        } else {
            variant.num_ranks
        };
        url.push(char::from_digit(max_rank as u32, 10).unwrap());
        let ranks = max_rank as usize;
        for &identity in self.deck.as_slice() {
            let index = variant.suit_of(identity) * ranks + variant.rank_of(identity) as usize - 1;
            url.push(BASE62[index] as char);
        }
        url.push(',');
        url.push_str("05");
        for action in &self.actions {
            let json = self.encode_action(&variant, action);
            let v = json.value.map_or(0, |v| v as usize + 1);
            url.push(BASE62[v * 6 + json.kind as usize] as char);
            url.push(BASE62[json.target as usize] as char);
        }
        url.push(',');
        url.push_str(&variant.id.to_string());
        url
    }

    /// Parse the data part of a replay URL: `deck`, `actions` and `variant id` as they appear
    /// after the prefix (also accepts the full URL).
    pub fn from_url(url: &str) -> Result<Self, ReplayError> {
        let data = url.strip_prefix(URL_PREFIX).unwrap_or(url);
        let mut parts = data.split(',');
        let (deck, actions) = match (parts.next(), parts.next()) {
            (Some(deck), Some(actions)) => (deck, actions),
            _ => {
                return Err(ReplayError::Format(
                    "expected deck,actions[,variant]".into(),
                ));
            }
        };
        let variant_id: u32 = parts
            .next()
            .map(|v| {
                v.parse()
                    .map_err(|_| ReplayError::Format(format!("bad variant id {v}")))
            })
            .transpose()?
            .unwrap_or(0);
        Self::from_parts(deck, actions, variant_id, None)
    }

    /// Build a replay from the URL parts, optionally keeping only the first `turns` actions.
    pub fn from_parts(
        deck: &str,
        actions: &str,
        variant_id: u32,
        turns: Option<usize>,
    ) -> Result<Self, ReplayError> {
        let variant = Variant::by_id(variant_id).map_err(ReplayError::Variant)?;
        let decode = |c: u8| {
            BASE62.iter().position(|&b| b == c).ok_or_else(|| {
                ReplayError::Format(format!("invalid base62 character {:?}", c as char))
            })
        };
        let deck = deck.as_bytes();
        if deck.len() < 3 {
            return Err(ReplayError::Format("deck too short".into()));
        }
        let num_seats = (deck[0] as char)
            .to_digit(10)
            .ok_or_else(|| ReplayError::Format("player count must be a digit".into()))?
            as usize;
        let min_rank = (deck[1] as char).to_digit(10).unwrap_or(1) as usize;
        let max_rank = (deck[2] as char).to_digit(10).unwrap_or(5) as usize;
        let ranks = max_rank - min_rank + 1;
        let mut cards = Vec::with_capacity(deck.len() - 3);
        for &c in &deck[3..] {
            let index = decode(c)?;
            let (suit, rank) = (index / ranks, (index % ranks + min_rank) as u8);
            if suit >= variant.num_suits() || !variant.ranks().any(|r| r == rank) {
                return Err(ReplayError::Format(format!(
                    "card index {index} out of range"
                )));
            }
            cards.push(variant.identity(suit, rank));
        }
        let actions = actions.as_bytes();
        if actions.len() < 2 || actions.len() % 2 != 0 {
            return Err(ReplayError::Format("malformed action list".into()));
        }
        let min_type = (actions[0] as char).to_digit(10).unwrap_or(0) as usize;
        let max_type = (actions[1] as char).to_digit(10).unwrap_or(5) as usize;
        let types = max_type - min_type + 1;
        let mut decoded = Vec::new();
        for pair in actions[2..].chunks(2) {
            if turns.is_some_and(|t| decoded.len() >= t) {
                break;
            }
            let code = decode(pair[0])?;
            let kind = (code % types + min_type) as u8;
            let value = (code / types).checked_sub(1).map(|v| v as u8);
            let target = decode(pair[1])? as u8;
            decoded.push(Self::decode_action(&variant, kind, target, value)?);
        }
        Ok(Self {
            variant: variant.name.clone(),
            players: PLAYER_NAMES[..num_seats.min(6)]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            deck: World::new(&cards),
            actions: decoded,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{deal, shuffled_world, step};

    fn sample() -> Replay {
        let variant = Variant::no_variant();
        let world = shuffled_world(&variant, 7);
        let (mut state, _) = deal(&variant, 3, &world);
        let mut replay = Replay::new(&variant, 3, world);
        for _ in 0..3 {
            let target = state.current.after(1, 3);
            let clue = variant
                .clue_ids()
                .find(|&c| {
                    !crate::engine::touched_cards(&variant, &state, &world, target, c).is_empty()
                })
                .unwrap();
            let action = Action::Clue { target, clue };
            step(&variant, &mut state, &world, action);
            replay.actions.push(ReplayAction::Action(action));
        }
        let action = Action::Play(state.hand(state.current)[0]);
        step(&variant, &mut state, &world, action);
        replay.actions.push(ReplayAction::Action(action));
        replay.actions.push(ReplayAction::GameOver {
            seat: Seat(1),
            reason: EndReason::Illegal,
        });
        replay
    }

    #[test]
    fn json_round_trip() {
        let replay = sample();
        let json = replay.to_json();
        assert!(json.contains("\"variant\":\"No Variant\""));
        assert_eq!(Replay::from_json(&json).unwrap(), replay);
    }

    #[test]
    fn url_round_trip() {
        let replay = sample();
        let url = replay.to_url();
        assert!(url.starts_with("https://hanab.live/replay-json/315"));
        assert!(url.ends_with(",0"));
        let mut parsed = Replay::from_url(&url).unwrap();
        parsed.players = replay.players.clone();
        assert_eq!(parsed, replay);
    }
}
