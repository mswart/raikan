//! Adapters between the bot and the outside world: the CLI (`src/main.rs`), the
//! hanab.live client (`src/bin/raikan-web.rs`) and the web UI both of them serve
//! ([`web`]). Relative seats and hand positions exist only here.

pub mod explain;
pub mod load;
pub mod report;
pub mod text;
pub mod web;
