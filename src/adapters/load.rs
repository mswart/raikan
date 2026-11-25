//! Replays from what people paste: a hanab.live game id or replay URL (downloaded from
//! hanab.live with the default `fetch` feature), a `replay-json` URL, or replay JSON.

use crate::engine::{GameReference, Replay};

/// A replay read from text, with what the text said about it.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub replay: Replay,
    /// The turn of a replay URL's `#T` fragment.
    pub turn: Option<usize>,
    /// The hanab.live game id, when the text named one.
    pub game_id: Option<u64>,
}

/// Read a replay from `text` (never from a file: the caller decides which files to read).
pub fn replay_from_text(text: &str) -> Result<Loaded, String> {
    let text = text.trim();
    if let Some(game) = GameReference::parse(text) {
        let json = download(&game.export_url())
            .map_err(|e| format!("cannot download game {}: {e}", game.id))?;
        let replay =
            Replay::from_json(&json).map_err(|e| format!("cannot parse game {}: {e}", game.id))?;
        return Ok(Loaded {
            replay,
            turn: game.turn,
            game_id: Some(game.id),
        });
    }
    let replay = if text.starts_with('{') {
        Replay::from_json(text)
    } else if text.starts_with("http") {
        Replay::from_url(text)
    } else {
        return Err(format!(
            "cannot read {text:?}: expected a hanab.live game id or replay URL, a replay-json \
             URL or replay JSON"
        ));
    }
    .map_err(|e| format!("cannot parse replay: {e}"))?;
    Ok(Loaded {
        replay,
        turn: None,
        game_id: None,
    })
}

/// Download `url` as text.
#[cfg(feature = "fetch")]
pub fn download(url: &str) -> Result<String, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .and_then(|client| client.get(url).send())
        .and_then(|response| response.error_for_status())
        .map_err(|e| e.to_string())?;
    response.text().map_err(|e| e.to_string())
}

#[cfg(not(feature = "fetch"))]
pub fn download(url: &str) -> Result<String, String> {
    Err(format!(
        "built without the `fetch` feature; save {url} to a file and pass that instead"
    ))
}
