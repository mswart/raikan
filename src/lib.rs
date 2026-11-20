//! Raikan: a Hanabi bot following the H-Group conventions (see docs/architecture.md).
//!
//! Layers, lowest first: `domain` and `trace` beside everything, then `variants`,
//! `engine`, `knowledge`, `conventions`, `policy`, `search`, `agent` and `adapters`.

pub mod adapters;
pub mod agent;
pub mod conventions;
pub mod domain;
pub mod engine;
pub mod knowledge;
pub mod policy;
pub mod search;
pub mod trace;
pub mod variants;
