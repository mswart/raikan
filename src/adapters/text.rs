//! Text forms of actions, cards and notes for people: seat names and hand slots
//! (slot 1 = newest card), as hanab.live shows them. The layers below only use absolute
//! seats and card ids.

use crate::conventions::CardNote;
use crate::domain::{CardId, ClueId, IdentitySet, Seat};
use crate::engine::{Action, GameState, World};
use crate::variants::{ClueValue, Stack, Variant};

/// Names of the seats of one table, for printing and parsing.
#[derive(Clone, Debug)]
pub struct Names {
    pub players: Vec<String>,
}

impl Names {
    pub fn new(players: &[String]) -> Self {
        Self {
            players: players.to_vec(),
        }
    }

    pub fn seat(&self, seat: Seat) -> String {
        self.players
            .get(seat.index())
            .cloned()
            .unwrap_or_else(|| format!("seat {}", seat.0))
    }

    /// A seat by number (`0`-based) or by name (case-insensitive prefix).
    pub fn parse_seat(&self, text: &str) -> Option<Seat> {
        if let Ok(n) = text.parse::<u8>() {
            return ((n as usize) < self.players.len()).then_some(Seat(n));
        }
        let lower = text.to_lowercase();
        let matches: Vec<usize> = (0..self.players.len())
            .filter(|&i| self.players[i].to_lowercase().starts_with(&lower))
            .collect();
        match matches[..] {
            [i] => Some(Seat(i as u8)),
            _ => None,
        }
    }
}

/// Slot of `card` in its holder's hand (1 = newest), if it is in a hand.
pub fn slot(state: &GameState, card: CardId) -> Option<(Seat, usize)> {
    let seat = state.holder(card)?;
    let hand = state.hand(seat);
    let index = hand.iter().position(|&c| c == card)?;
    Some((seat, hand.len() - index))
}

/// `#17 r3` with the true identity when known.
pub fn card(variant: &Variant, world: Option<&World>, card: CardId) -> String {
    match world {
        Some(world) => format!(
            "#{} {}",
            card.0,
            variant.identity_name(world.identity(card))
        ),
        None => format!("#{}", card.0),
    }
}

/// An action as `play slot 2 (#17 r3)` or `clue Bob red (slots 1,3)`, from the state
/// before it. `world` adds true identities.
pub fn action(
    variant: &Variant,
    names: &Names,
    state: &GameState,
    world: Option<&World>,
    action: Action,
) -> String {
    let at = |c: CardId| match slot(state, c) {
        Some((_, s)) => format!("slot {s} ({})", card(variant, world, c)),
        None => card(variant, world, c),
    };
    match action {
        Action::Play(c) => format!("play {}", at(c)),
        Action::Discard(c) => format!("discard {}", at(c)),
        Action::Clue { target, clue } => {
            let name = format!("clue {} {}", names.seat(target), variant.clue_name(clue));
            match world {
                Some(world) => {
                    let touched = crate::engine::touched_cards(variant, state, world, target, clue);
                    let hand = state.hand(target);
                    let slots: Vec<String> = hand
                        .iter()
                        .enumerate()
                        .rev()
                        .filter(|(_, c)| touched.contains(**c))
                        .map(|(i, _)| (hand.len() - i).to_string())
                        .collect();
                    format!("{name} (slots {})", slots.join(","))
                }
                None => name,
            }
        }
    }
}

/// Parse `play 2`, `discard 4`, `play #17`, `clue Bob red`, `clue 1 3` for the seat `actor`
/// (slots count from the newest card, 1-based).
pub fn parse_action(
    variant: &Variant,
    names: &Names,
    state: &GameState,
    actor: Seat,
    text: &str,
) -> Result<Action, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let own_card = |word: &str| -> Result<CardId, String> {
        let hand = state.hand(actor);
        if let Some(id) = word.strip_prefix('#') {
            let id: u8 = id.parse().map_err(|_| format!("bad card {word}"))?;
            return hand
                .iter()
                .copied()
                .find(|c| c.0 == id)
                .ok_or_else(|| format!("card #{id} is not in {}'s hand", names.seat(actor)));
        }
        let slot: usize = word.parse().map_err(|_| format!("bad slot {word}"))?;
        if !(1..=hand.len()).contains(&slot) {
            return Err(format!("{} has no slot {slot}", names.seat(actor)));
        }
        Ok(hand[hand.len() - slot])
    };
    match words[..] {
        ["play", c] => Ok(Action::Play(own_card(c)?)),
        ["discard", c] => Ok(Action::Discard(own_card(c)?)),
        ["clue", target, value] => {
            let target = names
                .parse_seat(target)
                .ok_or_else(|| format!("unknown seat {target}"))?;
            Ok(Action::Clue {
                target,
                clue: parse_clue(variant, value)?,
            })
        }
        _ => Err(format!(
            "cannot read action {text:?} (play N, discard N, clue SEAT VALUE)"
        )),
    }
}

/// A rank (`3`) or a clue colour name (`red`).
pub fn parse_clue(variant: &Variant, text: &str) -> Result<ClueId, String> {
    let value = match text.parse::<u8>() {
        Ok(rank) => ClueValue::Rank(rank),
        Err(_) => {
            let index = variant
                .clue_colors
                .iter()
                .position(|c| c.eq_ignore_ascii_case(text))
                .ok_or_else(|| format!("unknown clue {text}"))?;
            ClueValue::Color(index as u8)
        }
    };
    variant
        .clue_id(value)
        .ok_or_else(|| format!("no clue {text} in this variant"))
}

/// A card as `#17`, `17` or `SEAT:SLOT` (e.g. `Bob:2`, `1:2`).
pub fn parse_card(names: &Names, state: &GameState, text: &str) -> Result<CardId, String> {
    if let Some((seat, slot)) = text.split_once(':') {
        let seat = names
            .parse_seat(seat)
            .ok_or_else(|| format!("unknown seat {seat}"))?;
        let slot: usize = slot.parse().map_err(|_| format!("bad slot {slot}"))?;
        let hand = state.hand(seat);
        if !(1..=hand.len()).contains(&slot) {
            return Err(format!("{} has no slot {slot}", names.seat(seat)));
        }
        return Ok(hand[hand.len() - slot]);
    }
    let id: u8 = text
        .trim_start_matches('#')
        .parse()
        .map_err(|_| format!("bad card {text} (#ID or SEAT:SLOT)"))?;
    Ok(CardId(id))
}

/// Identity set as `r3,y3` (or `any` / `N identities` when large).
pub fn identities(variant: &Variant, set: IdentitySet) -> String {
    if set == variant.all_identities() {
        return "any".to_string();
    }
    if set.is_empty() {
        return "none".to_string();
    }
    if set.len() > 8 {
        return format!("{} identities", set.len());
    }
    set.iter()
        .map(|i| variant.identity_name(i))
        .collect::<Vec<_>>()
        .join(",")
}

/// A note as `r3,y3 (literal r3,y3,g3) clued play`.
pub fn note(variant: &Variant, note: &CardNote) -> String {
    let mut text = identities(variant, note.belief.soft);
    if note.belief.hard != note.belief.soft {
        text.push_str(&format!(
            " (literal {})",
            identities(variant, note.belief.hard)
        ));
    }
    for (flag, name) in [
        (note.clued, "clued"),
        (note.play, "play"),
        (note.trash, "trash"),
        (note.locked, "locked"),
        (note.fixed, "fixed"),
        (note.stopped, "stopped"),
        (note.one_chop_focus, "chop-1"),
    ] {
        if flag {
            text.push(' ');
            text.push_str(name);
        }
    }
    if note.delayed > 0 {
        text.push_str(&format!(" after {}", note.delayed));
    }
    if let Some(turn) = note.promised {
        text.push_str(&format!(" promised@{turn}"));
    }
    text
}

/// Stacks as `r2 y0 g1 b0 p3`.
pub fn stacks(variant: &Variant, state: &GameState) -> String {
    (0..variant.num_suits())
        .map(|s| {
            let Stack::Up(top) = state.stacks[s];
            format!("{}{top}", variant.suits[s].abbreviation)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An event as `Alice clues Bob red (#12,#14)` or `Bob plays #12 r2`.
pub fn event(variant: &Variant, names: &Names, event: &crate::engine::Event) -> String {
    use crate::engine::Event;
    let id = |identity: Option<crate::domain::Identity>| {
        identity.map_or_else(|| "?".to_string(), |i| variant.identity_name(i))
    };
    match *event {
        Event::Draw {
            seat,
            card,
            identity,
        } => format!("{} draws #{} {}", names.seat(seat), card.0, id(identity)),
        Event::Play {
            seat,
            card,
            identity,
            success,
        } => format!(
            "{} {} #{} {}",
            names.seat(seat),
            if success { "plays" } else { "misplays" },
            card.0,
            id(identity)
        ),
        Event::Discard {
            seat,
            card,
            identity,
        } => format!("{} discards #{} {}", names.seat(seat), card.0, id(identity)),
        Event::Clue {
            giver,
            target,
            clue,
            touched,
        } => format!(
            "{} clues {} {} ({})",
            names.seat(giver),
            names.seat(target),
            variant.clue_name(clue),
            touched
                .iter()
                .map(|c| format!("#{}", c.0))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Event::GameOver { reason } => format!("game over ({reason:?})"),
    }
}
