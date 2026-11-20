//! Root planner (docs/architecture.md §7.4): legal games, reproducible decisions, decisions
//! independent of hidden cards, and sampled worlds consistent with the bot's sight.

use raikan::agent::{Agent, BotAgent, Config, replay_prefix, run_game};
use raikan::domain::{CardId, Seat};
use raikan::engine::{EndReason, ReplayAction, World, deal, shuffled_world};
use raikan::knowledge::{ExactSampler, WorldSampler};
use raikan::search::{SearchConfig, SearchMode};
use raikan::variants::Variant;

/// A small budget: debug builds are slow.
fn small_search() -> Config {
    let mut search = SearchConfig::new(SearchMode::Fast);
    search.simulations = 40;
    search.worlds = 6;
    Config {
        search,
        ..Config::default()
    }
}

#[test]
fn search_games_finish_legally() {
    let variant = Variant::no_variant();
    // the smallest budget that still compares candidates: whole games are slow in debug
    let mut config = small_search();
    config.search.simulations = 12;
    config.search.worlds = 3;
    let world = shuffled_world(&variant, 7);
    let mut bots: Vec<BotAgent> = (0..3).map(|_| BotAgent::new(config.clone())).collect();
    let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
    let result = run_game(&variant, &world, &mut refs);
    let end = result.replay.actions.iter().find_map(|a| match a {
        ReplayAction::GameOver { reason, .. } => Some(*reason),
        _ => None,
    });
    assert_ne!(end, Some(EndReason::Illegal));
    assert!(result.score > 5, "score {}", result.score);
}

/// Bots after the first `turns` actions of a blueprint game.
fn bots_at(variant: &Variant, seats: u8, seed: u64, turns: usize) -> Vec<BotAgent> {
    let world = shuffled_world(variant, seed);
    let mut players: Vec<BotAgent> = (0..seats).map(|_| BotAgent::default()).collect();
    let mut refs: Vec<&mut dyn Agent> = players.iter_mut().map(|b| b as &mut dyn Agent).collect();
    let replay = run_game(variant, &world, &mut refs).replay;
    let mut bots: Vec<BotAgent> = (0..seats).map(|_| BotAgent::new(small_search())).collect();
    let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
    replay_prefix(&replay, turns, &mut refs).expect("legal prefix");
    bots
}

#[test]
fn same_position_gives_same_plan() {
    let variant = Variant::no_variant();
    let bots = bots_at(&variant, 3, 3, 9);
    let view = bots[0].view();
    let blueprint = raikan::policy::choose_action(view);
    let config = small_search().search;
    let a = raikan::search::plan(view, &blueprint, &config);
    let b = raikan::search::plan(view, &blueprint, &config);
    assert_eq!(a.action, b.action);
    assert_eq!(a.seed, b.seed);
    let means = |p: &raikan::search::PlanResult| -> Vec<f64> {
        p.candidates.iter().map(|c| c.mean).collect()
    };
    assert_eq!(means(&a), means(&b));
}

/// Swapping our own starting hand with later deck cards must not change a searched
/// decision (seeds are derived from public state, worlds from our view only).
#[test]
fn searched_decisions_do_not_depend_on_hidden_cards() {
    let variant = Variant::no_variant();
    for seed in 0..4u64 {
        let seats = 3 + (seed % 2) as u8;
        let world = shuffled_world(&variant, seed);
        let hand = raikan::engine::hand_size(seats) as usize;
        let mut swapped: Vec<_> = world.as_slice().to_vec();
        let deck_start = hand * seats as usize;
        for i in 0..hand {
            swapped.swap(i, deck_start + i);
        }
        let other = World::new(&swapped);
        let mut decisions = Vec::new();
        for world in [&world, &other] {
            let mut bots: Vec<BotAgent> =
                (0..seats).map(|_| BotAgent::new(small_search())).collect();
            for (s, bot) in bots.iter_mut().enumerate() {
                bot.start(&variant, seats, Seat(s as u8));
            }
            let (state, events) = deal(&variant, seats, world);
            for event in &events {
                for (s, bot) in bots.iter_mut().enumerate() {
                    bot.observe(&event.observed_by(Seat(s as u8)));
                }
            }
            decisions.push(bots[0].choose_action(&state));
        }
        assert_eq!(decisions[0], decisions[1], "seed {seed}");
    }
}

/// Worlds sampled from a view agree with every card the view can see, never use more
/// copies than exist, and respect the literal information of our own cards.
#[test]
fn sampled_worlds_respect_sight_and_counts() {
    let variant = Variant::no_variant();
    for (seed, turns) in [(1u64, 6usize), (2, 14), (5, 25)] {
        let truth = shuffled_world(&variant, seed);
        let bots = bots_at(&variant, 4, seed, turns);
        let view = bots[0].view();
        let store = view.constraint_store();
        let samples = ExactSampler::new(&store, 0.02).sample(30, seed);
        assert_eq!(samples.worlds.len(), 30);
        for world in &samples.worlds {
            let mut counts = [0u8; 64];
            for c in 0..variant.num_cards() {
                let card = CardId(c);
                let identity = world.identity(card);
                counts[identity.index()] += 1;
                if let Some(seen) = view.identity(card) {
                    assert_eq!(seen, identity, "seed {seed}: card #{c} contradicts sight");
                    assert_eq!(seen, truth.identity(card));
                }
            }
            for identity in variant.all_identities().iter() {
                assert_eq!(counts[identity.index()], variant.copies(identity));
            }
            for &card in view.state.hand(view.me) {
                assert!(
                    view.note(card).belief.contains_hard(world.identity(card)),
                    "seed {seed}: own card #{} outside literal information",
                    card.0
                );
            }
        }
    }
}

/// No searched action is one the legibility guard rejects, except the blueprint's own
/// choice, which stays the reference (docs/architecture.md §10).
#[test]
fn searched_actions_pass_the_legibility_guard() {
    let variant = Variant::no_variant();
    let config = small_search().search;
    let mut checked = 0;
    for (seed, turns) in [(11u64, 4usize), (12, 9), (13, 15), (14, 21)] {
        let bots = bots_at(&variant, 3, seed, turns);
        let seat = turns % 3;
        let view = bots[seat].view();
        let blueprint = raikan::policy::choose_action(view);
        let plan = raikan::search::plan(view, &blueprint, &config);
        let baseline = view.worth(0);
        for candidate in &plan.candidates[1..] {
            assert!(
                raikan::policy::permitted(view, &baseline, candidate.action),
                "seed {seed}: candidate {:?} is not legible",
                candidate.action
            );
        }
        assert!(
            plan.action == blueprint.action
                || raikan::policy::permitted(view, &baseline, plan.action)
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
}

/// The planner's result does not depend on the thread count (§10), with and without the
/// partner model's spread.
#[test]
fn plans_do_not_depend_on_threads() {
    let variant = Variant::no_variant();
    for (seed, turns, spread) in [(21u64, 8usize, 0.0), (22, 13, 0.2)] {
        let bots = bots_at(&variant, 3, seed, turns);
        let view = bots[turns % 3].view();
        let blueprint = raikan::policy::choose_action(view);
        let mut config = small_search().search;
        config.partner_spread = spread;
        let one = raikan::search::plan(view, &blueprint, &config);
        config.threads = 3;
        let three = raikan::search::plan(view, &blueprint, &config);
        assert_eq!(one.action, three.action, "seed {seed}");
        let means = |p: &raikan::search::PlanResult| -> Vec<(usize, f64)> {
            p.candidates.iter().map(|c| (c.visits, c.mean)).collect()
        };
        assert_eq!(means(&one), means(&three), "seed {seed}");
    }
}

/// The partner model predicts obligations with certainty and otherwise spreads over the
/// blueprint's alternatives; probabilities sum to one and the blueprint's choice is first.
#[test]
fn partner_distribution_is_normalised() {
    use raikan::policy::partner::{is_obligation, partner_distribution};
    let variant = Variant::no_variant();
    let mut spread_seen = false;
    for (seed, turns) in [(31u64, 3usize), (32, 7), (33, 12), (34, 18), (35, 25)] {
        let bots = bots_at(&variant, 4, seed, turns);
        for bot in &bots {
            let view = bot.view();
            let blueprint = raikan::policy::partner_decision(view);
            let alone = partner_distribution(view, 0.0);
            assert_eq!(alone.len(), 1);
            assert_eq!(alone[0].action, blueprint.action);
            let spread = partner_distribution(view, 0.2);
            assert_eq!(spread[0].action, blueprint.action);
            let total: f64 = spread.iter().map(|p| p.probability).sum();
            assert!((total - 1.0).abs() < 1e-12);
            if is_obligation(blueprint.reason) {
                assert_eq!(spread.len(), 1);
            }
            spread_seen |= spread.len() > 1;
            let actions: Vec<_> = spread.iter().map(|p| p.action).collect();
            for (i, a) in actions.iter().enumerate() {
                assert!(!actions[..i].contains(a), "duplicate prediction");
            }
        }
    }
    assert!(spread_seen);
}

/// Extra search candidates (known plays and known trash from any slot) are legal and
/// pass the guard.
#[test]
fn permitted_candidates_are_legal() {
    let variant = Variant::no_variant();
    for (seed, turns) in [(41u64, 10usize), (42, 20), (43, 30), (44, 40)] {
        let bots = bots_at(&variant, 3, seed, turns);
        let view = bots[turns % 3].view();
        if view.state.is_over() {
            continue;
        }
        let baseline = view.worth(0);
        for action in raikan::policy::permitted_candidates(view) {
            assert!(raikan::engine::check_legal(&variant, &view.state, action).is_ok());
            assert!(
                raikan::policy::permitted(view, &baseline, action),
                "{action:?}"
            );
        }
    }
}
