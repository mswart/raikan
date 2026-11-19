//! Agents: anything that observes events and chooses actions for one seat, plus the
//! self-play runner that drives a table of agents through the simulator.

pub mod bot;

pub use bot::{BotAgent, Config};

use crate::domain::Seat;
use crate::engine::{self, Action, Event, GameState, Replay, ReplayAction, World};
use crate::variants::Variant;

/// One seat's player. It only ever sees events redacted for its seat and the public state.
pub trait Agent {
    /// Called once before the deal.
    fn start(&mut self, variant: &Variant, num_seats: u8, seat: Seat);
    /// Called for every event, in order, redacted for this seat.
    fn observe(&mut self, event: &Event);
    /// Called when it is this seat's turn.
    fn choose_action(&mut self, state: &GameState) -> Action;
    /// Human-readable description of the agent's current knowledge, for debugging.
    fn describe(&self) -> String {
        String::new()
    }
}

/// Outcome of one simulated game.
#[derive(Debug)]
pub struct GameResult {
    pub state: GameState,
    pub replay: Replay,
    pub score: u8,
    /// Successful plays of cards that were never clued.
    pub blind_plays: u8,
}

/// Play one game with the given agents (one per seat) on a fixed deck.
pub fn run_game(variant: &Variant, world: &World, agents: &mut [&mut dyn Agent]) -> GameResult {
    let num_seats = agents.len() as u8;
    for (seat, agent) in agents.iter_mut().enumerate() {
        agent.start(variant, num_seats, Seat(seat as u8));
    }
    let (mut state, deal_events) = engine::deal(variant, num_seats, world);
    let broadcast = |agents: &mut [&mut dyn Agent], event: &Event| {
        for (seat, agent) in agents.iter_mut().enumerate() {
            agent.observe(&event.observed_by(Seat(seat as u8)));
        }
    };
    for event in &deal_events {
        broadcast(agents, event);
    }

    let mut replay = Replay::new(variant, num_seats, *world);
    let mut blind_plays = 0;
    while !state.is_over() {
        let seat = state.current;
        let action = agents[seat.index()].choose_action(&state);
        let blind = matches!(action, Action::Play(card) if !state.clued.contains(card));
        let events = engine::step(variant, &mut state, world, action);
        if blind
            && events
                .iter()
                .any(|e| matches!(e, Event::Play { success: true, .. }))
        {
            blind_plays += 1;
        }
        finish_step(&mut replay, seat, action, &events);
        for event in &events {
            broadcast(agents, event);
        }
    }
    GameResult {
        score: state.score(variant),
        state,
        replay,
        blind_plays,
    }
}

fn finish_step(replay: &mut Replay, seat: Seat, action: Action, events: &[Event]) {
    if events.iter().any(Event::is_action) {
        replay.actions.push(ReplayAction::Action(action));
    }
    for event in events {
        if let Event::GameOver { reason } = event {
            replay.actions.push(ReplayAction::GameOver {
                seat,
                reason: *reason,
            });
        }
    }
}

/// Feed the first `turns` actions of a replay to the agents (they only observe; their own
/// choices are not asked). Returns the state after those actions.
pub fn replay_prefix(
    replay: &Replay,
    turns: usize,
    agents: &mut [&mut dyn Agent],
) -> Result<GameState, String> {
    let variant = Variant::by_name(&replay.variant).map_err(|e| e.to_string())?;
    let num_seats = replay.num_seats();
    for (seat, agent) in agents.iter_mut().enumerate() {
        agent.start(&variant, num_seats, Seat(seat as u8));
    }
    let (mut state, deal_events) = engine::deal(&variant, num_seats, &replay.deck);
    for event in &deal_events {
        for (seat, agent) in agents.iter_mut().enumerate() {
            agent.observe(&event.observed_by(Seat(seat as u8)));
        }
    }
    for action in replay.game_actions().take(turns) {
        if let Err(err) = engine::check_legal(&variant, &state, action) {
            return Err(format!(
                "turn {}: {action:?} is illegal: {err:?}",
                state.turn
            ));
        }
        for event in engine::step(&variant, &mut state, &replay.deck, action) {
            for (seat, agent) in agents.iter_mut().enumerate() {
                agent.observe(&event.observed_by(Seat(seat as u8)));
            }
        }
    }
    Ok(state)
}
