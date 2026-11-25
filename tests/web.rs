//! The web UI's API (`adapters::web`): a live table shows only the observing bot's view; a
//! finished game's history becomes a complete replay with every view, rankings and bug
//! reports.

use raikan::adapters::explain::replay_events;
use raikan::adapters::web::{Hub, Request, Session};
use raikan::agent::{Agent, BotAgent, Config, run_game};
use raikan::domain::Seat;
use raikan::engine::{Event, Replay, shuffled_world};
use raikan::variants::Variant;
use serde_json::{Value, json};

fn game() -> (Variant, Replay, Vec<Event>) {
    let variant = Variant::no_variant();
    let world = shuffled_world(&variant, 21);
    let mut bots: Vec<BotAgent> = (0..3).map(|_| BotAgent::default()).collect();
    let mut refs: Vec<&mut dyn Agent> = bots.iter_mut().map(|b| b as &mut dyn Agent).collect();
    let replay = run_game(&variant, &world, &mut refs).replay;
    let events = replay_events(&variant, &replay)
        .unwrap()
        .into_iter()
        .map(|(_, e)| e)
        .collect();
    (variant, replay, events)
}

/// The events before turn `turn` (the deal and `turn - 1` actions with their draws).
fn before(events: &[Event], turn: usize) -> Vec<Event> {
    let mut actions = 0;
    events
        .iter()
        .take_while(|e| {
            if e.is_action() {
                actions += 1;
            }
            actions < turn
        })
        .copied()
        .collect()
}

fn get(hub: &Hub, path: &str) -> (u16, Value) {
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let request = Request {
        method: "GET".into(),
        path: path.into(),
        query: query
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap();
                (k.to_string(), v.to_string())
            })
            .collect(),
        ..Request::default()
    };
    let response = hub.handle(&request);
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}

fn post(hub: &Hub, path: &str, body: Value) -> (u16, Value) {
    let request = Request {
        method: "POST".into(),
        path: path.into(),
        content_type: Some("application/json".into()),
        body: body.to_string().into_bytes(),
        ..Request::default()
    };
    let response = hub.handle(&request);
    (
        response.status,
        serde_json::from_slice(&response.body).unwrap(),
    )
}

#[test]
fn a_live_table_shows_only_the_observers_view() {
    let (variant, replay, events) = game();
    // seat 1 moves at turns 2, 5, 8, ...: the latest position is seat 1's decision
    let turn = 11;
    let observed: Vec<Event> = before(&events, turn)
        .iter()
        .map(|e| e.observed_by(Seat(1)))
        .collect();
    let hub = Hub::new(Config::default());
    hub.publish(Session::observed(
        "live",
        "live table",
        &variant,
        &replay.players,
        Some(Seat(1)),
        &observed,
        true,
    ));

    let (status, board) = get(&hub, "/api/board?viewer=0");
    assert_eq!(status, 200, "{board}");
    assert_eq!(board["session"]["complete"], false);
    assert_eq!(board["session"]["observer"], 1);
    assert_eq!(board["session"]["turns"], turn);
    let b = &board["board"];
    assert_eq!(b["viewer"], 1, "a live table has one viewer");
    assert_eq!(b["to_move"], 1);
    for hand in b["hands"].as_array().unwrap() {
        for card in hand["cards"].as_array().unwrap() {
            let own = hand["seat"] == 1;
            assert_eq!(card["identity"].is_null(), own, "{card}");
            // the holder's knowledge as seat 1 models it, for the other hands
            assert_eq!(card["holder"].is_null(), own, "{card}");
            assert!(
                card["actual"].is_null(),
                "no seat's own view but the observer's"
            );
        }
    }
    assert!(!b["legal"].as_array().unwrap().is_empty());

    // the observer's decision can be ranked, another seat's cannot
    let (status, explanation) = post(&hub, "/api/explain", json!({"turn": turn, "search": "off"}));
    assert_eq!(status, 200, "{explanation}");
    assert_eq!(explanation["seat"], 1);
    assert!(explanation["position"]["hands"][1]["cards"][0]["identity"] == "?");
    let (status, error) = post(&hub, "/api/explain", json!({"turn": turn - 1}));
    assert_eq!(status, 422);
    assert!(
        error["error"].as_str().unwrap().contains("live game"),
        "{error}"
    );
    // no bug reports without the whole deck
    let (status, _) = post(
        &hub,
        "/api/report",
        json!({"turn": turn, "expected": "play 1"}),
    );
    assert_eq!(status, 422);
    let (status, history) = get(&hub, &format!("/api/card?turn={turn}&card=0"));
    assert_eq!(status, 200, "{history}");
}

#[test]
fn a_finished_history_becomes_a_complete_replay() {
    let (variant, replay, events) = game();
    let hub = Hub::new(Config::default());
    // the full history as hanab.live sends it at the end, received by seat 2
    hub.publish(Session::observed(
        "done",
        "finished table",
        &variant,
        &replay.players,
        Some(Seat(2)),
        &events,
        false,
    ));
    let (status, board) = get(&hub, "/api/board?turn=7&viewer=0");
    assert_eq!(status, 200, "{board}");
    assert_eq!(board["session"]["complete"], true);
    assert!(board["session"]["observer"].is_null());
    let b = &board["board"];
    assert_eq!(b["viewer"], 0);
    let own = &b["hands"][0]["cards"][0];
    assert!(!own["identity"].is_null(), "replays show every card");
    assert_eq!(own["visible"], false);
    assert!(!b["hands"][1]["cards"][0]["actual"].is_null());
    let played = b["played"]["spec"].as_str().unwrap().to_string();
    let entry = &b["log"][6];
    assert_eq!(entry["turn"], 7);
    assert_eq!(
        b["log"].as_array().unwrap().len(),
        replay.game_actions().count()
    );

    // any seat's decision, ranked
    let (status, explanation) = post(&hub, "/api/explain", json!({"turn": 7}));
    assert_eq!(status, 200, "{explanation}");
    assert_eq!(explanation["played"], explanation["blueprint"]["action"]);
    assert!(
        explanation["comparison"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["spec"] == played.as_str())
    );

    // a bug report replays the game to the decision
    let other = b["legal"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["spec"].as_str().unwrap())
        .find(|&s| s != played)
        .unwrap()
        .to_string();
    let (status, report) = post(
        &hub,
        "/api/report",
        json!({"turn": 7, "expected": other, "comment": "a test", "rule": "L1.test"}),
    );
    assert_eq!(status, 200, "{report}");
    let text = report["text"].as_str().unwrap();
    let seat = report["seat"].as_u64().unwrap();
    assert!(text.starts_with("# rule: L1.test\n"), "{text}");
    assert!(
        text.contains(&format!("\naction {seat}: {other}\n")),
        "{text}"
    );
    assert!(
        text.contains(&format!("\nnot-action {seat}: {played}\n")),
        "{text}"
    );
}

#[test]
fn unknown_routes_and_methods_are_refused() {
    let hub = Hub::new(Config::default());
    assert_eq!(get(&hub, "/nope").0, 404);
    assert_eq!(get(&hub, "/api/explain").0, 405);
    assert_eq!(get(&hub, "/api/board").0, 404, "no game yet");
    let (status, error) = post(&hub, "/api/load", json!({"replay": "/etc/passwd"}));
    assert_eq!(status, 422, "files are never read: {error}");
    let page = hub.handle(&Request {
        method: "GET".into(),
        path: "/".into(),
        ..Request::default()
    });
    assert_eq!(page.status, 200);
    assert!(String::from_utf8_lossy(&page.body).contains("<title>Raikan</title>"));
}
