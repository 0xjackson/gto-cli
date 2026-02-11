//! Maps between `Card` structs and u8 indices (0-51) for the fast evaluator.
//!
//! Encoding: index = rank_offset * 4 + suit_offset
//!   rank_offset: 0=Two, 1=Three, ..., 12=Ace
//!   suit_offset: 0=Spades, 1=Hearts, 2=Diamonds, 3=Clubs

use crate::cards::{Card, Rank, Suit, ALL_RANKS, ALL_SUITS};

pub fn card_to_index(card: &Card) -> u8 {
    let rank_idx = card.rank as u8 - 2; // Rank::Two = 2
    let suit_idx = match card.suit {
        Suit::Spades => 0,
        Suit::Hearts => 1,
        Suit::Diamonds => 2,
        Suit::Clubs => 3,
    };
    rank_idx * 4 + suit_idx
}

pub fn index_to_card(index: u8) -> Card {
    Card::new(ALL_RANKS[(index / 4) as usize], ALL_SUITS[(index % 4) as usize])
}

pub fn cards_to_indices(cards: &[Card]) -> Vec<u8> {
    cards.iter().map(card_to_index).collect()
}

/// Build a full deck (0-51) excluding the given dead cards.
pub fn remaining_deck(dead: &[u8]) -> Vec<u8> {
    let mut dead_set = [false; 52];
    for &d in dead {
        dead_set[d as usize] = true;
    }
    (0..52u8).filter(|&c| !dead_set[c as usize]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for i in 0..52u8 {
            let card = index_to_card(i);
            assert_eq!(card_to_index(&card), i, "roundtrip failed for index {}", i);
        }
    }

    #[test]
    fn known_cards() {
        // Two of spades = rank 0, suit 0 → index 0
        assert_eq!(card_to_index(&Card::new(Rank::Two, Suit::Spades)), 0);
        // Ace of clubs = rank 12, suit 3 → index 51
        assert_eq!(card_to_index(&Card::new(Rank::Ace, Suit::Clubs)), 51);
        // Ace of spades = rank 12, suit 0 → index 48
        assert_eq!(card_to_index(&Card::new(Rank::Ace, Suit::Spades)), 48);
    }

    #[test]
    fn remaining_deck_size() {
        let dead = vec![0, 1, 2, 3]; // 4 dead cards
        let deck = remaining_deck(&dead);
        assert_eq!(deck.len(), 48);
        assert!(!deck.contains(&0));
        assert!(!deck.contains(&3));
        assert!(deck.contains(&4));
    }
}
