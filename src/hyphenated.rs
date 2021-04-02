use crate::game::{self, CardPlayState};

pub struct HyphenatedPlayer {}

impl game::PlayerStrategy for HyphenatedPlayer {
    fn act(&self, game: &game::Game) -> game::Move {
        // look for critical cards on chop:
        for player in 1..game.num_players() as usize {
            let mut chop = game.num_hand_cards(player) - 1;
            while game.card_cluded(chop, player) {
                if chop == 0 {
                    break;
                }
                chop -= 1
            }
            if game.card_cluded(chop, player) {
                break;
            }
            let card = game.player_card(chop, player);
            if let CardPlayState::Critical() = card.play_state(&game) {
                return game::Move::Clue(player as u8, game::Clue::Rank(card.rank));
            }
        }
        // discard
        let mut chop = game.num_hand_cards(0) - 1;
        while chop > 0 && game.card_cluded(chop, 0) {
            chop -= 1
        }
        game::Move::Discard(chop)
    }
}
