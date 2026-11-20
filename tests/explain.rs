//! `raikan explain` (docs/architecture.md §8.2): a recomputed decision matches the game,
//! comparisons use the planner's worlds, rollouts and card histories are complete.

use raikan::adapters::explain::{Request, explain};
use raikan::adapters::text::{self, Names};
use raikan::agent::{Agent, BotAgent, Config, run_game};
use raikan::engine::{Replay, shuffled_world};
use raikan::search::{SearchConfig, SearchMode};
use raikan::variants::Variant;

fn game(seats: u8, seed: u64) -> Replay {
    let variant = Variant::no_variant();
    let world = shuffled_world(&variant, seed);
    let mut bots: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
    let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
    run_game(&variant, &world, &mut refs).replay
}

fn small() -> SearchConfig {
    let mut config = SearchConfig::new(SearchMode::Fast);
    config.worlds = 6;
    config.simulations = 60;
    config
}

fn request(turn: usize) -> Request {
    let mut request = Request::new(turn);
    request.compare = small();
    request
}

/// A blueprint game explained with the blueprint: the recomputed choice is the one played,
/// and it is the reference row of the comparison.
#[test]
fn recomputed_decision_is_the_played_one() {
    let replay = game(3, 5);
    for turn in [1, 7, 12, 20] {
        let e = explain(&replay, &Config::default(), &request(turn)).expect("explains");
        assert_eq!(
            e.played.as_deref(),
            Some(e.blueprint.action.as_str()),
            "turn {turn}"
        );
        assert_eq!(e.chosen.action, e.blueprint.action);
        let first = &e.comparison.rows[0];
        assert_eq!(first.action, e.blueprint.action);
        assert!(first.sources.contains(&"blueprint") && first.sources.contains(&"played"));
        assert!(first.difference.is_none());
        assert!(
            e.comparison.rows[1..]
                .iter()
                .all(|r| r.difference.is_some())
        );
        assert_eq!(e.position.hands.len(), 3);
        serde_json::to_string(&e).expect("serialises");
        assert!(e.render().contains(&format!("Turn {turn}")));
    }
}

/// With search on, the comparison runs on the planner's worlds: a candidate the planner
/// evaluated on every world has the same mean in both tables.
#[test]
fn comparison_uses_the_planner_worlds() {
    let replay = game(3, 8);
    let config = Config {
        search: small(),
        ..Config::default()
    };
    let e = explain(&replay, &config, &request(9)).expect("explains");
    let plan = e.plan.as_ref().expect("search is on");
    assert_eq!(plan.seed, e.comparison.seed);
    let mut matched = 0;
    for row in plan.rows.iter().filter(|r| r.visits == plan.worlds) {
        let other = e
            .comparison
            .rows
            .iter()
            .find(|c| c.action == row.action)
            .expect("planner candidates are compared");
        assert!((other.mean - row.mean).abs() < 1e-9, "{}", row.action);
        matched += 1;
    }
    assert!(plan.worlds == 0 || matched > 0);
}

#[test]
fn requested_actions_rollouts_and_card_history() {
    let replay = game(4, 3);
    let mut r = request(10);
    r.candidates = vec!["discard 1".into(), "play 3".into()];
    r.world = Some(2);
    r.card = Some("Bob:1".into());
    r.viewer = Some("Alice".into());
    let e = explain(&replay, &Config::default(), &r).expect("explains");
    let requested: Vec<_> = e
        .comparison
        .rows
        .iter()
        .filter(|row| row.sources.contains(&"requested"))
        .collect();
    assert_eq!(requested.len(), 2);
    assert_eq!(e.rollouts.len(), 2);
    for rollout in &e.rollouts {
        assert_eq!(rollout.world, 2);
        assert!(rollout.steps[0].contains(&rollout.action));
        assert!(
            rollout.steps.len() >= 4,
            "one rotation: {:?}",
            rollout.steps
        );
    }
    let card = e.card.as_ref().expect("card history");
    assert_eq!(card.viewer, "Alice");
    assert!(card.entries[0].event.contains("draws"));

    // errors are reported, not panics
    assert!(explain(&replay, &Config::default(), &request(0)).is_err());
    assert!(explain(&replay, &Config::default(), &request(10_000)).is_err());
    let mut bad = request(10);
    bad.candidates = vec!["play 9".into()];
    assert!(explain(&replay, &Config::default(), &bad).is_err());
}

#[test]
fn action_text_round_trips() {
    let replay = game(3, 2);
    let variant = Variant::no_variant();
    let names = Names::new(&replay.players);
    let (state, _) = raikan::engine::deal(&variant, 3, &replay.deck);
    for spec in ["play 1", "discard 3", "clue Bob red", "clue 2 4", "play #2"] {
        let action = text::parse_action(&variant, &names, &state, state.current, spec)
            .unwrap_or_else(|e| panic!("{spec}: {e}"));
        let shown = text::action(&variant, &names, &state, None, action);
        let again = text::parse_action(
            &variant,
            &names,
            &state,
            state.current,
            shown.replace("slot ", "").split(" (").next().unwrap(),
        )
        .unwrap();
        assert_eq!(action, again, "{spec} -> {shown}");
    }
}
