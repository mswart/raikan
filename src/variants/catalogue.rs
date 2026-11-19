//! Raw hanab.live catalogue (`suits.json`, `variants.json`), vendored at a pinned revision.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Upstream revision of the vendored catalogue files in `data/hanabi-live/`.
pub const CATALOGUE_REVISION: &str = "1701eb99677f";

const SUITS_JSON: &str = include_str!("../../data/hanabi-live/suits.json");
const VARIANTS_JSON: &str = include_str!("../../data/hanabi-live/variants.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuitJson {
    pub name: String,
    pub id: String,
    pub abbreviation: Option<String>,
    pub display_name: Option<String>,
    pub clue_colors: Option<Vec<String>>,
    #[serde(default)]
    pub one_of_each: bool,
    #[serde(default)]
    pub all_clue_colors: bool,
    #[serde(default)]
    pub no_clue_colors: bool,
    #[serde(default)]
    pub all_clue_ranks: bool,
    #[serde(default)]
    pub no_clue_ranks: bool,
    #[serde(default)]
    pub prism: bool,
    #[serde(default)]
    pub inverted: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantJson {
    pub id: u32,
    pub name: String,
    pub suits: Vec<String>,
    pub clue_colors: Option<Vec<String>>,
    pub clue_ranks: Option<Vec<u8>>,
    pub stack_size: Option<u8>,
    pub special_rank: Option<u8>,
    #[serde(default)]
    pub special_rank_all_clue_colors: bool,
    #[serde(default)]
    pub special_rank_all_clue_ranks: bool,
    #[serde(default)]
    pub special_rank_no_clue_colors: bool,
    #[serde(default)]
    pub special_rank_no_clue_ranks: bool,
    #[serde(default)]
    pub special_rank_deceptive: bool,
    pub critical_rank: Option<u8>,
    #[serde(default)]
    pub scarce_ones: bool,
    #[serde(default)]
    pub clue_starved: bool,
    #[serde(default)]
    pub color_clues_touch_nothing: bool,
    #[serde(default)]
    pub rank_clues_touch_nothing: bool,
    #[serde(default)]
    pub alternating_clues: bool,
    #[serde(default)]
    pub cow_and_pig: bool,
    #[serde(default)]
    pub duck: bool,
    #[serde(default)]
    pub odds_and_evens: bool,
    #[serde(default)]
    pub synesthesia: bool,
    #[serde(default)]
    pub up_or_down: bool,
    #[serde(default)]
    pub throw_it_in_a_hole: bool,
    #[serde(default)]
    pub funnels: bool,
    #[serde(default)]
    pub chimneys: bool,
    #[serde(default)]
    pub sudoku: bool,
}

pub struct Catalogue {
    pub suits: HashMap<String, SuitJson>,
    pub variants: Vec<VariantJson>,
}

/// The parsed catalogue, built once per process.
pub fn catalogue() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        let suits: Vec<SuitJson> =
            serde_json::from_str(SUITS_JSON).expect("vendored suits.json must parse");
        let variants: Vec<VariantJson> =
            serde_json::from_str(VARIANTS_JSON).expect("vendored variants.json must parse");
        Catalogue {
            suits: suits.into_iter().map(|s| (s.name.clone(), s)).collect(),
            variants,
        }
    })
}
