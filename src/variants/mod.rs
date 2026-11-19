//! Variant-driven game rules: identities, copies, clues, touch and stacks.
//!
//! A [`Variant`] is built from the vendored hanab.live catalogue and precomputes every table
//! the other layers need. Suit, clue colour and identity are separate concepts: the only
//! place a touch rule lives is [`Variant::touch`]. Code above this layer asks capability
//! queries and never keys on variant names.

pub mod catalogue;

use crate::domain::{ClueId, Identity, IdentitySet, MAX_CARDS, MAX_IDENTITIES, MAX_SUITS};
use catalogue::{SuitJson, VariantJson, catalogue};
use serde::{Deserialize, Serialize};

pub use catalogue::CATALOGUE_REVISION;

/// The hanab.live wire value of a clue: a clue-colour index or a rank value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClueValue {
    Color(u8),
    Rank(u8),
}

/// A mechanic that the engine does not implement yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariantError {
    UnknownVariant(String),
    UnknownSuit(String),
    Unsupported {
        variant: String,
        feature: &'static str,
    },
}

impl std::fmt::Display for VariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownVariant(name) => write!(f, "unknown variant {name:?}"),
            Self::UnknownSuit(name) => write!(f, "unknown suit {name:?}"),
            Self::Unsupported { variant, feature } => {
                write!(f, "variant {variant:?} needs unsupported feature {feature}")
            }
        }
    }
}

impl std::error::Error for VariantError {}

/// Gameplay properties of one suit in a variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuitInfo {
    pub name: String,
    /// Single lower-case letter(s) used in text formats (`r`, `y`, `m` for rainbow, …).
    pub abbreviation: String,
    pub clue_colors: Vec<String>,
    pub one_of_each: bool,
    pub all_clue_colors: bool,
    pub no_clue_colors: bool,
    pub all_clue_ranks: bool,
    pub no_clue_ranks: bool,
    pub prism: bool,
}

/// State of one play stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stack {
    /// Built upwards; the value is the highest played rank (0 = empty).
    Up(u8),
}

impl Default for Stack {
    fn default() -> Self {
        Stack::Up(0)
    }
}

/// All play stacks, indexed by suit.
pub type Stacks = [Stack; MAX_SUITS];

/// Per-identity counters (e.g. discarded copies).
pub type IdentityCounts = [u8; MAX_IDENTITIES];

/// Clue-token parameters in half-token units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenRules {
    pub max: u8,
    pub start: u8,
    pub clue_cost: u8,
    pub discard_gain: u8,
    pub stack_complete_gain: u8,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub id: u32,
    pub name: String,
    pub suits: Vec<SuitInfo>,
    /// Ranks per suit (5 normally, 4 with a reduced stack size).
    pub num_ranks: u8,
    pub clue_colors: Vec<String>,
    pub clue_ranks: Vec<u8>,
    clues: Vec<ClueValue>,
    touch: Vec<IdentitySet>,
    copies: IdentityCounts,
    num_cards: u8,
    pub tokens: TokenRules,
    /// Variant-wide rule modifiers in effect (e.g. `special-rank`, `clue-starved`).
    pub modifiers: Vec<&'static str>,
}

impl Variant {
    /// The standard five-suit game.
    pub fn no_variant() -> Variant {
        Self::by_name("No Variant").expect("No Variant is always supported")
    }

    pub fn by_name(name: &str) -> Result<Variant, VariantError> {
        let json = catalogue()
            .variants
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| VariantError::UnknownVariant(name.to_string()))?;
        Self::from_json(json)
    }

    pub fn by_id(id: u32) -> Result<Variant, VariantError> {
        let json = catalogue()
            .variants
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| VariantError::UnknownVariant(id.to_string()))?;
        Self::from_json(json)
    }

    /// Every variant name in the catalogue, supported or not.
    pub fn catalogue_names() -> impl Iterator<Item = &'static str> {
        catalogue().variants.iter().map(|v| v.name.as_str())
    }

    fn from_json(json: &VariantJson) -> Result<Variant, VariantError> {
        let unsupported = |feature| VariantError::Unsupported {
            variant: json.name.clone(),
            feature,
        };
        for (flag, feature) in [
            (json.up_or_down, "up-or-down"),
            (json.throw_it_in_a_hole, "throw-it-in-a-hole"),
            (json.cow_and_pig, "cow-and-pig"),
            (json.duck, "duck"),
            (json.alternating_clues, "alternating-clues"),
            (json.sudoku, "sudoku"),
            // Upstream permits clues that touch nothing in these variants; the engine and
            // the clue reader require a touched card.
            (
                json.color_clues_touch_nothing,
                "empty clues (color clues touch nothing)",
            ),
            (
                json.rank_clues_touch_nothing,
                "empty clues (rank clues touch nothing)",
            ),
        ] {
            if flag {
                return Err(unsupported(feature));
            }
        }

        let mut suit_jsons: Vec<&SuitJson> = Vec::with_capacity(json.suits.len());
        for name in &json.suits {
            if name.ends_with(" Reversed") {
                return Err(unsupported("reversed suits"));
            }
            let suit = catalogue()
                .suits
                .get(name)
                .ok_or_else(|| VariantError::UnknownSuit(name.clone()))?;
            if suit.inverted {
                return Err(unsupported("inverted suits"));
            }
            suit_jsons.push(suit);
        }
        if suit_jsons.len() > MAX_SUITS {
            return Err(unsupported("more than six suits"));
        }

        let suits: Vec<SuitInfo> = suit_jsons
            .iter()
            .map(|s| {
                let clue_colors = match &s.clue_colors {
                    Some(colors) => colors.clone(),
                    None if s.all_clue_colors || s.no_clue_colors || s.prism => Vec::new(),
                    None => vec![s.name.clone()],
                };
                SuitInfo {
                    name: s.name.clone(),
                    abbreviation: s
                        .abbreviation
                        .clone()
                        .unwrap_or_else(|| default_abbreviation(&s.name))
                        .to_lowercase(),
                    clue_colors,
                    one_of_each: s.one_of_each,
                    all_clue_colors: s.all_clue_colors,
                    no_clue_colors: s.no_clue_colors,
                    all_clue_ranks: s.all_clue_ranks,
                    no_clue_ranks: s.no_clue_ranks,
                    prism: s.prism,
                }
            })
            .collect();

        let clue_colors = match &json.clue_colors {
            Some(colors) => colors.clone(),
            None => {
                let mut colors: Vec<String> = Vec::new();
                for suit in &suits {
                    if suit.all_clue_colors {
                        continue;
                    }
                    for color in &suit.clue_colors {
                        if !colors.contains(color) {
                            colors.push(color.clone());
                        }
                    }
                }
                colors
            }
        };
        let clue_ranks = json
            .clue_ranks
            .clone()
            .unwrap_or_else(|| vec![1, 2, 3, 4, 5]);
        let num_ranks = json.stack_size.unwrap_or(5);

        let mut clues = Vec::new();
        for color in 0..clue_colors.len() {
            clues.push(ClueValue::Color(color as u8));
        }
        for &rank in &clue_ranks {
            clues.push(ClueValue::Rank(rank));
        }

        let num_identities = suits.len() * num_ranks as usize;
        assert!(num_identities <= MAX_IDENTITIES);

        let mut copies = [0u8; MAX_IDENTITIES];
        for (suit_index, suit) in suits.iter().enumerate() {
            for rank in 1..=num_ranks {
                let count = if suit.one_of_each || json.critical_rank == Some(rank) {
                    1
                } else {
                    match rank {
                        1 if json.scarce_ones => 2,
                        1 => 3,
                        5 => 1,
                        _ => 2,
                    }
                };
                copies[suit_index * num_ranks as usize + rank as usize - 1] = count;
            }
        }
        let num_cards: usize = copies.iter().map(|&c| c as usize).sum();
        assert!(num_cards <= MAX_CARDS);

        let mut touch = Vec::with_capacity(clues.len());
        for clue in &clues {
            let mut set = IdentitySet::EMPTY;
            for (suit_index, suit) in suits.iter().enumerate() {
                for rank in 1..=num_ranks {
                    let touched = match *clue {
                        ClueValue::Color(color) => touches_color(
                            json,
                            &clue_colors,
                            suit,
                            rank,
                            &clue_colors[color as usize],
                        ),
                        ClueValue::Rank(clue_rank) => {
                            touches_rank(json, &clue_ranks, suit_index, suit, rank, clue_rank)
                        }
                    };
                    if touched {
                        set.insert(Identity((suit_index * num_ranks as usize) as u8 + rank - 1));
                    }
                }
            }
            touch.push(set);
        }

        let tokens = if json.clue_starved {
            TokenRules {
                max: 16,
                start: 16,
                clue_cost: 2,
                discard_gain: 1,
                stack_complete_gain: 1,
            }
        } else {
            TokenRules {
                max: 16,
                start: 16,
                clue_cost: 2,
                discard_gain: 2,
                stack_complete_gain: 2,
            }
        };

        let modifiers: Vec<&'static str> = [
            (json.special_rank.is_some(), "special-rank"),
            (json.critical_rank.is_some(), "critical-rank"),
            (json.scarce_ones, "scarce-ones"),
            (json.clue_starved, "clue-starved"),
            (json.color_clues_touch_nothing, "color-clues-touch-nothing"),
            (json.rank_clues_touch_nothing, "rank-clues-touch-nothing"),
            (json.odds_and_evens, "odds-and-evens"),
            (json.synesthesia, "synesthesia"),
            (json.funnels, "funnels"),
            (json.chimneys, "chimneys"),
            (json.stack_size.is_some(), "stack-size"),
            (json.clue_colors.is_some(), "clue-colors"),
            (json.clue_ranks.is_some(), "clue-ranks"),
        ]
        .into_iter()
        .filter_map(|(set, name)| set.then_some(name))
        .collect();

        Ok(Variant {
            modifiers,
            id: json.id,
            name: json.name.clone(),
            suits,
            num_ranks,
            clue_colors,
            clue_ranks,
            clues,
            touch,
            copies,
            num_cards: num_cards as u8,
            tokens,
        })
    }

    // ----- identities -----

    pub fn num_suits(&self) -> usize {
        self.suits.len()
    }

    pub fn num_identities(&self) -> usize {
        self.suits.len() * self.num_ranks as usize
    }

    pub fn all_identities(&self) -> IdentitySet {
        IdentitySet::first(self.num_identities())
    }

    pub fn identity(&self, suit: usize, rank: u8) -> Identity {
        debug_assert!(suit < self.suits.len() && (1..=self.num_ranks).contains(&rank));
        Identity((suit * self.num_ranks as usize) as u8 + rank - 1)
    }

    pub fn suit_of(&self, identity: Identity) -> usize {
        identity.index() / self.num_ranks as usize
    }

    pub fn rank_of(&self, identity: Identity) -> u8 {
        (identity.index() % self.num_ranks as usize) as u8 + 1
    }

    /// All identities of one suit.
    pub fn suit_identities(&self, suit: usize) -> IdentitySet {
        IdentitySet(((1u64 << self.num_ranks) - 1) << (suit * self.num_ranks as usize))
    }

    /// All identities of one rank.
    pub fn rank_identities(&self, rank: u8) -> IdentitySet {
        (0..self.suits.len())
            .map(|s| self.identity(s, rank))
            .collect()
    }

    pub fn copies(&self, identity: Identity) -> u8 {
        self.copies[identity.index()]
    }

    pub fn num_cards(&self) -> u8 {
        self.num_cards
    }

    pub fn max_score(&self) -> u8 {
        self.suits.len() as u8 * self.num_ranks
    }

    /// Short text form such as `r1` or `m5`.
    pub fn identity_name(&self, identity: Identity) -> String {
        format!(
            "{}{}",
            self.suits[self.suit_of(identity)].abbreviation,
            self.rank_of(identity)
        )
    }

    /// Parse the short text form produced by [`Variant::identity_name`].
    pub fn parse_identity(&self, text: &str) -> Option<Identity> {
        let rank: u8 = text.get(text.len().checked_sub(1)?..)?.parse().ok()?;
        let abbreviation = &text[..text.len() - 1];
        let suit = self
            .suits
            .iter()
            .position(|s| s.abbreviation == abbreviation)?;
        (1..=self.num_ranks)
            .contains(&rank)
            .then(|| self.identity(suit, rank))
    }

    /// The deck in catalogue order: suits in order, ranks ascending, copies consecutive.
    pub fn deck_order(&self) -> Vec<Identity> {
        let mut deck = Vec::with_capacity(self.num_cards as usize);
        for identity in self.all_identities().iter() {
            for _ in 0..self.copies(identity) {
                deck.push(identity);
            }
        }
        deck
    }

    // ----- clues -----

    pub fn num_clues(&self) -> usize {
        self.clues.len()
    }

    pub fn clue_ids(&self) -> impl Iterator<Item = ClueId> + use<> {
        (0..self.clues.len() as u8).map(ClueId)
    }

    pub fn clue_value(&self, clue: ClueId) -> ClueValue {
        self.clues[clue.index()]
    }

    pub fn clue_id(&self, value: ClueValue) -> Option<ClueId> {
        self.clues
            .iter()
            .position(|&c| c == value)
            .map(|i| ClueId(i as u8))
    }

    /// The identities a clue touches. This is the only touch rule in the code base.
    pub fn touch(&self, clue: ClueId) -> IdentitySet {
        self.touch[clue.index()]
    }

    pub fn clue_name(&self, clue: ClueId) -> String {
        match self.clue_value(clue) {
            ClueValue::Color(c) => self.clue_colors[c as usize].to_lowercase(),
            ClueValue::Rank(r) => r.to_string(),
        }
    }

    // ----- capabilities -----

    /// Some identity is touched by more than one colour clue (rainbow, dual colours, prism).
    pub fn has_multi_color_touch(&self) -> bool {
        self.all_identities().iter().any(|i| {
            self.clue_ids()
                .filter(|&c| matches!(self.clue_value(c), ClueValue::Color(_)))
                .filter(|&c| self.touch(c).contains(i))
                .count()
                > 1
        })
    }

    /// Some identity is touched by no colour clue (white, null, …).
    pub fn has_colorless(&self) -> bool {
        self.all_identities().iter().any(|i| {
            !self
                .clue_ids()
                .filter(|&c| matches!(self.clue_value(c), ClueValue::Color(_)))
                .any(|c| self.touch(c).contains(i))
        })
    }

    /// Whether some card of `suit` is touched by more than one colour clue (rainbow,
    /// dual colours, prism).
    pub fn is_multi_color_suit(&self, suit: usize) -> bool {
        let colors: Vec<ClueId> = self
            .clue_ids()
            .filter(|&c| matches!(self.clue_value(c), ClueValue::Color(_)))
            .collect();
        self.suit_identities(suit).iter().any(|i| {
            colors
                .iter()
                .filter(|&&c| self.touch(c).contains(i))
                .count()
                > 1
        })
    }

    /// Whether the H-Group conventions implemented so far cover this variant: plain suits
    /// and rainbow suits, without variant-wide modifiers. Returns what is missing.
    pub fn convention_support(&self) -> Result<(), String> {
        if let Some(modifier) = self.modifiers.first() {
            return Err(format!("variant modifier {modifier}"));
        }
        for suit in &self.suits {
            let plain = suit.clue_colors.len() == 1
                && !(suit.all_clue_colors
                    || suit.no_clue_colors
                    || suit.all_clue_ranks
                    || suit.no_clue_ranks
                    || suit.prism
                    || suit.one_of_each);
            let rainbow = suit.all_clue_colors
                && !(suit.no_clue_colors
                    || suit.all_clue_ranks
                    || suit.no_clue_ranks
                    || suit.prism
                    || suit.one_of_each);
            if !plain && !rainbow {
                return Err(format!("{} suit", suit.name));
            }
        }
        Ok(())
    }

    /// Some suit has a single copy of each card (black and other dark suits).
    pub fn has_unique_suits(&self) -> bool {
        self.suits.iter().any(|s| s.one_of_each)
    }

    // ----- stacks -----

    /// Identities that can be played successfully now.
    pub fn playable(&self, stacks: &Stacks) -> IdentitySet {
        let mut set = IdentitySet::EMPTY;
        for (suit, &Stack::Up(top)) in stacks.iter().enumerate().take(self.suits.len()) {
            if top < self.num_ranks {
                set.insert(self.identity(suit, top + 1));
            }
        }
        set
    }

    /// Identities that can never be played anymore: already played, or a lower rank has no
    /// copy left.
    pub fn trash(&self, stacks: &Stacks, discarded: &IdentityCounts) -> IdentitySet {
        let mut set = IdentitySet::EMPTY;
        for (suit, &Stack::Up(top)) in stacks.iter().enumerate().take(self.suits.len()) {
            let mut blocked = false;
            for rank in 1..=self.num_ranks {
                let identity = self.identity(suit, rank);
                if rank <= top || blocked {
                    set.insert(identity);
                }
                if rank > top && discarded[identity.index()] >= self.copies(identity) {
                    blocked = true;
                }
            }
        }
        set
    }

    /// Identities that are still needed and have exactly one copy left.
    pub fn critical(&self, stacks: &Stacks, discarded: &IdentityCounts) -> IdentitySet {
        let trash = self.trash(stacks, discarded);
        self.all_identities()
            .iter()
            .filter(|&i| !trash.contains(i))
            .filter(|&i| self.copies(i) - discarded[i.index()].min(self.copies(i)) == 1)
            .collect()
    }

    /// Identities that must be played before `identity` can be played.
    pub fn predecessors(&self, identity: Identity, stacks: &Stacks) -> IdentitySet {
        let suit = self.suit_of(identity);
        let Stack::Up(top) = stacks[suit];
        (top + 1..self.rank_of(identity))
            .map(|rank| self.identity(suit, rank))
            .collect()
    }

    /// Highest score still reachable given the discarded cards.
    pub fn max_reachable_score(&self, discarded: &IdentityCounts) -> u8 {
        let mut score = 0;
        for suit in 0..self.suits.len() {
            for rank in 1..=self.num_ranks {
                let identity = self.identity(suit, rank);
                if discarded[identity.index()] >= self.copies(identity) {
                    break;
                }
                score += 1;
            }
        }
        score
    }

    pub fn stack_score(&self, stacks: &Stacks) -> u8 {
        stacks[..self.suits.len()]
            .iter()
            .map(|s| match s {
                Stack::Up(top) => *top,
            })
            .sum()
    }

    /// Whether a played identity completes its stack.
    pub fn completes_stack(&self, identity: Identity) -> bool {
        self.rank_of(identity) == self.num_ranks
    }
}

fn default_abbreviation(name: &str) -> String {
    match name {
        "Black" => "K".to_string(),
        "Rainbow" => "M".to_string(),
        "Brown" => "N".to_string(),
        _ => name.chars().next().unwrap_or('?').to_string(),
    }
}

fn touches_color(
    json: &VariantJson,
    clue_colors: &[String],
    suit: &SuitInfo,
    rank: u8,
    color: &str,
) -> bool {
    if json.color_clues_touch_nothing {
        return false;
    }
    let rank_color = || clue_colors.get((rank as usize - 1) % clue_colors.len().max(1));
    if json.synesthesia && !suit.no_clue_ranks && rank_color().is_some_and(|c| c == color) {
        return true;
    }
    if suit.all_clue_colors {
        return true;
    }
    if suit.no_clue_colors {
        return false;
    }
    if Some(rank) == json.special_rank {
        if json.special_rank_all_clue_colors {
            return true;
        }
        if json.special_rank_no_clue_colors {
            return false;
        }
    }
    if suit.prism {
        return rank_color().is_some_and(|c| c == color);
    }
    suit.clue_colors.iter().any(|c| c == color)
}

fn touches_rank(
    json: &VariantJson,
    clue_ranks: &[u8],
    suit_index: usize,
    suit: &SuitInfo,
    rank: u8,
    clue_rank: u8,
) -> bool {
    if json.rank_clues_touch_nothing {
        return false;
    }
    if suit.all_clue_ranks {
        return true;
    }
    if suit.no_clue_ranks {
        return false;
    }
    if json.funnels {
        return rank <= clue_rank;
    }
    if json.chimneys {
        return rank >= clue_rank;
    }
    if json.odds_and_evens {
        return if clue_rank == 1 {
            rank % 2 == 1
        } else {
            rank % 2 == 0
        };
    }
    if Some(rank) == json.special_rank {
        if json.special_rank_all_clue_ranks {
            return true;
        }
        if json.special_rank_no_clue_ranks {
            return false;
        }
        if json.special_rank_deceptive {
            return clue_ranks.get(suit_index % clue_ranks.len().max(1)) == Some(&clue_rank);
        }
    }
    clue_rank == rank
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(variant: &Variant, set: IdentitySet) -> Vec<String> {
        set.iter().map(|i| variant.identity_name(i)).collect()
    }

    fn color(variant: &Variant, name: &str) -> ClueId {
        let index = variant.clue_colors.iter().position(|c| c == name).unwrap();
        variant.clue_id(ClueValue::Color(index as u8)).unwrap()
    }

    fn rank(variant: &Variant, rank: u8) -> ClueId {
        variant.clue_id(ClueValue::Rank(rank)).unwrap()
    }

    #[test]
    fn whole_catalogue_loads_or_reports_unsupported() {
        let mut supported = 0;
        for name in Variant::catalogue_names() {
            match Variant::by_name(name) {
                Ok(variant) => {
                    supported += 1;
                    assert!(variant.num_identities() <= MAX_IDENTITIES, "{name}");
                    assert!(variant.num_cards() as usize <= MAX_CARDS, "{name}");
                    assert_eq!(variant.deck_order().len(), variant.num_cards() as usize);
                }
                Err(VariantError::Unsupported { .. }) => {}
                Err(err) => panic!("{name}: {err}"),
            }
        }
        assert!(supported > 1000, "only {supported} variants supported");
    }

    #[test]
    fn no_variant() {
        let v = Variant::no_variant();
        assert_eq!(v.id, 0);
        assert_eq!(v.num_cards(), 50);
        assert_eq!(v.max_score(), 25);
        assert_eq!(v.clue_colors, ["Red", "Yellow", "Green", "Blue", "Purple"]);
        assert_eq!(v.num_clues(), 10);
        assert_eq!(
            names(&v, v.touch(color(&v, "Red"))),
            ["r1", "r2", "r3", "r4", "r5"]
        );
        assert_eq!(
            names(&v, v.touch(rank(&v, 2))),
            ["r2", "y2", "g2", "b2", "p2"]
        );
        assert_eq!(v.copies(v.identity(0, 1)), 3);
        assert_eq!(v.copies(v.identity(4, 5)), 1);
        assert_eq!(v.parse_identity("b3"), Some(v.identity(3, 3)));
        assert!(!v.has_multi_color_touch());
        assert!(!v.has_colorless());
        assert!(!v.has_unique_suits());
    }

    #[test]
    fn six_suits() {
        let v = Variant::by_name("6 Suits").unwrap();
        assert_eq!(v.num_cards(), 60);
        assert_eq!(v.num_identities(), 30);
        assert_eq!(
            names(&v, v.touch(color(&v, "Teal"))),
            ["t1", "t2", "t3", "t4", "t5"]
        );
        assert_eq!(v.touch(rank(&v, 5)).len(), 6);
    }

    #[test]
    fn black() {
        let v = Variant::by_name("Black (6 Suits)").unwrap();
        let black = v.suits.iter().position(|s| s.name == "Black").unwrap();
        assert_eq!(v.suits[black].abbreviation, "k");
        for rank in 1..=5 {
            assert_eq!(v.copies(v.identity(black, rank)), 1);
        }
        assert_eq!(v.num_cards(), 55);
        assert_eq!(
            names(&v, v.touch(color(&v, "Black"))),
            ["k1", "k2", "k3", "k4", "k5"]
        );
        assert!(v.has_unique_suits());
        // Every black card is critical from the start.
        let critical = v.critical(&Stacks::default(), &[0; MAX_IDENTITIES]);
        assert!(v.suit_identities(black).is_subset(critical));
    }

    #[test]
    fn rainbow() {
        let v = Variant::by_name("Rainbow (6 Suits)").unwrap();
        let rainbow = v.suits.iter().position(|s| s.name == "Rainbow").unwrap();
        assert_eq!(v.clue_colors.len(), 5, "rainbow adds no clue colour");
        for c in 0..5u8 {
            let touched = v.touch(v.clue_id(ClueValue::Color(c)).unwrap());
            assert!(v.suit_identities(rainbow).is_subset(touched));
            assert_eq!(touched.len(), 10);
        }
        assert!(v.has_multi_color_touch());
        assert!(!v.has_colorless());
    }

    #[test]
    fn white() {
        let v = Variant::by_name("White (6 Suits)").unwrap();
        let white = v.suits.iter().position(|s| s.name == "White").unwrap();
        assert_eq!(v.clue_colors.len(), 5, "white adds no clue colour");
        for c in 0..5u8 {
            let touched = v.touch(v.clue_id(ClueValue::Color(c)).unwrap());
            assert!(!touched.intersects(v.suit_identities(white)));
        }
        assert!(v.touch(rank(&v, 3)).contains(v.identity(white, 3)));
        assert!(v.has_colorless());
    }

    #[test]
    fn convention_support() {
        for name in [
            "No Variant",
            "6 Suits",
            "Rainbow (6 Suits)",
            "Rainbow (5 Suits)",
        ] {
            assert_eq!(
                Variant::by_name(name).unwrap().convention_support(),
                Ok(()),
                "{name}"
            );
        }
        for name in [
            "Black (6 Suits)",
            "White (6 Suits)",
            "Pink (6 Suits)",
            "Muddy Rainbow (6 Suits)",
            "Rainbow-Ones (6 Suits)",
        ] {
            assert!(
                Variant::by_name(name)
                    .unwrap()
                    .convention_support()
                    .is_err(),
                "{name}"
            );
        }
        let rainbow = Variant::by_name("Rainbow (6 Suits)").unwrap();
        assert!(rainbow.is_multi_color_suit(5));
        assert!(!rainbow.is_multi_color_suit(0));
    }

    #[test]
    fn unsupported_mechanics_are_reported() {
        for name in [
            "Orange (6 Suits)",
            "Up or Down (6 Suits)",
            "Throw It in a Hole (6 Suits)",
            "Color Blind (5 Suits)",
            "Number Blind (5 Suits)",
        ] {
            assert!(
                matches!(
                    Variant::by_name(name),
                    Err(VariantError::Unsupported { .. })
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn stacks() {
        let v = Variant::no_variant();
        let mut stacks = Stacks::default();
        stacks[0] = Stack::Up(2);
        let mut discarded = [0u8; MAX_IDENTITIES];
        assert_eq!(
            names(&v, v.playable(&stacks)),
            ["r3", "y1", "g1", "b1", "p1"]
        );
        // both y3 lost: y4 and y5 can never be played
        discarded[v.identity(1, 3).index()] = 2;
        let trash = v.trash(&stacks, &discarded);
        assert_eq!(names(&v, trash), ["r1", "r2", "y4", "y5"]);
        let critical = v.critical(&stacks, &discarded);
        assert!(critical.contains(v.identity(0, 5)));
        assert!(!critical.contains(v.identity(1, 5)));
        assert_eq!(
            names(&v, v.predecessors(v.identity(0, 5), &stacks)),
            ["r3", "r4"]
        );
        assert_eq!(v.max_reachable_score(&discarded), 22);
        assert_eq!(v.stack_score(&stacks), 2);
    }
}
