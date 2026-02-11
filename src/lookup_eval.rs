//! Fast hand evaluator using rank histograms and bit manipulation.
//!
//! Returns a u32 "hand score" where higher = better hand. Scores can be
//! compared directly with `>` / `<` / `==` — no need to unpack category
//! or kickers.
//!
//! Encoding (24 bits used):
//!   bits 23-20: category (0=HighCard .. 9=RoyalFlush)
//!   bits 19-16: primary rank value (2-14)
//!   bits 15-12: secondary rank value
//!   bits 11-8:  kicker 1
//!   bits  7-4:  kicker 2
//!   bits  3-0:  kicker 3
//!
//! Performance: ~10-50M evaluations/sec (vs ~500K with itertools approach).

use once_cell::sync::Lazy;

use crate::hand_evaluator::HandCategory;

// -------------------------------------------------------------------------
// Precomputed straight detection table
// -------------------------------------------------------------------------

/// For a 13-bit rank bitmask, returns the highest straight's high card
/// (rank value 5-14), or 0 if no straight exists.
///
/// Bit layout: bit 0 = Two(2), bit 1 = Three(3), ..., bit 12 = Ace(14).
static STRAIGHT_TABLE: Lazy<[u8; 8192]> = Lazy::new(|| {
    let mut table = [0u8; 8192];
    for mask in 0u16..8192 {
        let mut best = 0u8;

        // Regular straights: 5 consecutive bits
        // high_bit 4..=12 corresponds to high card 6..14
        for high_bit in 4..=12u8 {
            let pat = 0x1Fu16 << (high_bit - 4);
            if mask & pat == pat {
                best = high_bit + 2; // bit index → rank value
            }
        }

        // Wheel: A-2-3-4-5 = bits 12,0,1,2,3
        let wheel: u16 = (1 << 12) | 0b1111;
        if mask & wheel == wheel && best == 0 {
            best = 5; // 5-high straight
        }

        table[mask as usize] = best;
    }
    table
});

// -------------------------------------------------------------------------
// Score packing
// -------------------------------------------------------------------------

/// Pack a hand category + up to 5 rank values into a single u32.
#[inline]
fn hand_score(category: u8, v: &[u8]) -> u32 {
    let mut s = (category as u32) << 20;
    let shifts: [u8; 5] = [16, 12, 8, 4, 0];
    for (i, &r) in v.iter().enumerate() {
        if i >= 5 { break; }
        s |= (r as u32) << shifts[i];
    }
    s
}

/// Extract the top `n` set bits from a 13-bit mask as rank values (high→low).
#[inline]
fn top_n_from_mask(mask: u16, n: usize) -> [u8; 5] {
    let mut result = [0u8; 5];
    let mut count = 0;
    for bit in (0..13u8).rev() {
        if mask & (1 << bit) != 0 {
            result[count] = bit + 2; // bit index → rank value
            count += 1;
            if count == n { break; }
        }
    }
    result
}

// -------------------------------------------------------------------------
// Core evaluator — works for 5, 6, or 7 cards
// -------------------------------------------------------------------------

/// Evaluate a hand of 5-7 cards (encoded as u8 indices 0-51).
/// Returns a u32 score: higher = better. Directly comparable.
pub fn evaluate_fast(cards: &[u8]) -> u32 {
    debug_assert!(cards.len() >= 5 && cards.len() <= 7);

    let mut rank_counts = [0u8; 13]; // index 0=Two .. 12=Ace
    let mut suit_masks = [0u16; 4];  // 13-bit rank mask per suit
    let mut suit_counts = [0u8; 4];

    for &c in cards {
        let rank = (c >> 2) as usize;   // c / 4
        let suit = (c & 0x3) as usize;  // c % 4
        rank_counts[rank] += 1;
        suit_masks[suit] |= 1 << rank;
        suit_counts[suit] += 1;
    }

    // --- Flush path (5+ cards of one suit) ---
    // If a flush exists, it always beats any non-flush hand that can
    // coexist in the same 7 cards (quads/full-house can't coexist with
    // a flush due to pigeonhole on suits).
    if let Some(suit) = suit_counts.iter().position(|&c| c >= 5) {
        let fmask = suit_masks[suit];
        let sf_high = STRAIGHT_TABLE[fmask as usize];
        if sf_high > 0 {
            if sf_high == 14 {
                return hand_score(9, &[14]); // Royal flush
            }
            return hand_score(8, &[sf_high]); // Straight flush
        }
        let ranks = top_n_from_mask(fmask, 5);
        return hand_score(5, &ranks); // Flush
    }

    // --- Non-flush path ---
    evaluate_non_flush(&rank_counts)
}

/// Evaluate the best 5-card non-flush hand from rank frequency counts.
fn evaluate_non_flush(rc: &[u8; 13]) -> u32 {
    // Collect ranks by frequency, scanning high (Ace=12) to low (Two=0)
    // so each list is already sorted descending by rank value.

    // Max possible counts in 7 cards:
    //   quads: 1, trips: 2, pairs: 3, singles: 7
    let mut quad = [0u8; 1];   let mut nq: usize = 0;
    let mut trip = [0u8; 2];   let mut nt: usize = 0;
    let mut pair = [0u8; 3];   let mut np: usize = 0;
    let mut sing = [0u8; 7];   let mut ns: usize = 0;

    for idx in (0..13usize).rev() {
        let rv = idx as u8 + 2; // rank value 2-14
        match rc[idx] {
            4 => { quad[nq] = rv; nq += 1; }
            3 => { trip[nt] = rv; nt += 1; }
            2 => { pair[np] = rv; np += 1; }
            1 => { sing[ns] = rv; ns += 1; }
            _ => {}
        }
    }

    // Four of a Kind — best kicker from remaining cards
    if nq >= 1 {
        let kick = if nt > 0 { trip[0] }
                   else if np > 0 { pair[0] }
                   else { sing[0] };
        return hand_score(7, &[quad[0], kick]);
    }

    // Full House — best trips + best pair (second trips counts as pair)
    if nt >= 1 && (np >= 1 || nt >= 2) {
        let pr = if nt >= 2 { trip[1] } else { pair[0] };
        return hand_score(6, &[trip[0], pr]);
    }

    // Straight — check combined rank presence mask
    let rank_mask: u16 = (0..13).fold(0u16, |m, i| {
        if rc[i] > 0 { m | (1 << i) } else { m }
    });
    let sh = STRAIGHT_TABLE[rank_mask as usize];
    if sh > 0 {
        return hand_score(4, &[sh]);
    }

    // Three of a Kind — trips + 2 best kickers (only singles here)
    if nt >= 1 {
        return hand_score(3, &[trip[0], sing[0], sing[1]]);
    }

    // Two Pair — best 2 pairs + best kicker
    if np >= 2 {
        let kick = if np >= 3 && pair[2] > sing.get(0).copied().unwrap_or(0) {
            pair[2]
        } else {
            sing.get(0).copied().unwrap_or(0)
        };
        return hand_score(2, &[pair[0], pair[1], kick]);
    }

    // One Pair — pair + 3 best kickers
    if np == 1 {
        return hand_score(1, &[pair[0], sing[0], sing[1], sing[2]]);
    }

    // High Card — 5 best singles
    hand_score(0, &[sing[0], sing[1], sing[2], sing[3], sing[4]])
}

// -------------------------------------------------------------------------
// Score → HandCategory (for display code)
// -------------------------------------------------------------------------

/// Extract the HandCategory from a packed score.
pub fn category_from_score(score: u32) -> HandCategory {
    match (score >> 20) & 0xF {
        9 => HandCategory::RoyalFlush,
        8 => HandCategory::StraightFlush,
        7 => HandCategory::FourOfAKind,
        6 => HandCategory::FullHouse,
        5 => HandCategory::Flush,
        4 => HandCategory::Straight,
        3 => HandCategory::ThreeOfAKind,
        2 => HandCategory::TwoPair,
        1 => HandCategory::OnePair,
        _ => HandCategory::HighCard,
    }
}

/// Extract kicker values from a packed score (up to 5 values).
pub fn kickers_from_score(score: u32) -> Vec<u8> {
    let mut kickers = Vec::new();
    for &shift in &[16, 12, 8, 4, 0] {
        let v = ((score >> shift) & 0xF) as u8;
        if v > 0 {
            kickers.push(v);
        }
    }
    kickers
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: encode cards from notation like "As" = Ace of spades
    fn idx(notation: &str) -> u8 {
        let chars: Vec<char> = notation.chars().collect();
        let rank = match chars[0] {
            '2' => 0, '3' => 1, '4' => 2, '5' => 3, '6' => 4,
            '7' => 5, '8' => 6, '9' => 7, 'T' => 8, 'J' => 9,
            'Q' => 10, 'K' => 11, 'A' => 12, _ => panic!("bad rank"),
        };
        let suit = match chars[1] {
            's' => 0, 'h' => 1, 'd' => 2, 'c' => 3, _ => panic!("bad suit"),
        };
        rank * 4 + suit
    }

    fn ids(cards: &[&str]) -> Vec<u8> {
        cards.iter().map(|s| idx(s)).collect()
    }

    #[test]
    fn royal_flush() {
        let cards = ids(&["As", "Ks", "Qs", "Js", "Ts"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::RoyalFlush);
    }

    #[test]
    fn straight_flush_7high() {
        let cards = ids(&["7h", "6h", "5h", "4h", "3h"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::StraightFlush);
    }

    #[test]
    fn steel_wheel() {
        // A-2-3-4-5 of hearts = straight flush, 5-high
        let cards = ids(&["Ah", "2h", "3h", "4h", "5h"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::StraightFlush);
    }

    #[test]
    fn quads() {
        let cards = ids(&["Ks", "Kh", "Kd", "Kc", "As"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::FourOfAKind);
    }

    #[test]
    fn full_house() {
        let cards = ids(&["As", "Ah", "Ad", "Ks", "Kh"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::FullHouse);
    }

    #[test]
    fn flush() {
        let cards = ids(&["As", "Ts", "8s", "5s", "2s"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::Flush);
    }

    #[test]
    fn straight() {
        let cards = ids(&["9s", "8h", "7d", "6c", "5s"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::Straight);
    }

    #[test]
    fn wheel_straight() {
        let cards = ids(&["As", "2h", "3d", "4c", "5s"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::Straight);
    }

    #[test]
    fn trips() {
        let cards = ids(&["Qs", "Qh", "Qd", "Ks", "7h"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::ThreeOfAKind);
    }

    #[test]
    fn two_pair() {
        let cards = ids(&["As", "Ad", "Kh", "Ks", "Qc"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::TwoPair);
    }

    #[test]
    fn one_pair() {
        let cards = ids(&["As", "Ah", "Kd", "Qs", "Jh"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::OnePair);
    }

    #[test]
    fn high_card() {
        let cards = ids(&["As", "Kh", "Qd", "Js", "9c"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::HighCard);
    }

    #[test]
    fn category_ordering() {
        // Each hand type beats the one below it
        let hands: Vec<(Vec<u8>, HandCategory)> = vec![
            (ids(&["As", "Ks", "Qs", "Js", "Ts"]), HandCategory::RoyalFlush),
            (ids(&["9h", "8h", "7h", "6h", "5h"]), HandCategory::StraightFlush),
            (ids(&["Ks", "Kh", "Kd", "Kc", "As"]), HandCategory::FourOfAKind),
            (ids(&["As", "Ah", "Ad", "Ks", "Kh"]), HandCategory::FullHouse),
            (ids(&["As", "Ts", "8s", "5s", "2s"]), HandCategory::Flush),
            (ids(&["9s", "8h", "7d", "6c", "5s"]), HandCategory::Straight),
            (ids(&["Qs", "Qh", "Qd", "Ks", "7h"]), HandCategory::ThreeOfAKind),
            (ids(&["As", "Ad", "Kh", "Ks", "Qc"]), HandCategory::TwoPair),
            (ids(&["As", "Ah", "Kd", "Qs", "Jh"]), HandCategory::OnePair),
            (ids(&["As", "Kh", "Qd", "Js", "9c"]), HandCategory::HighCard),
        ];

        let scores: Vec<u32> = hands.iter().map(|(h, _)| evaluate_fast(h)).collect();

        for i in 0..scores.len() - 1 {
            assert!(
                scores[i] > scores[i + 1],
                "{:?} (score {}) should beat {:?} (score {})",
                hands[i].1, scores[i], hands[i + 1].1, scores[i + 1]
            );
        }
    }

    #[test]
    fn kicker_resolution_pairs() {
        // AA with K kicker vs AA with Q kicker
        let aak = ids(&["As", "Ah", "Kd", "7s", "3c"]);
        let aaq = ids(&["Ad", "Ac", "Qh", "7d", "3h"]);
        assert!(evaluate_fast(&aak) > evaluate_fast(&aaq));
    }

    #[test]
    fn seven_card_royal_flush() {
        let cards = ids(&["As", "Ks", "Qs", "Js", "Ts", "2h", "3d"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::RoyalFlush);
    }

    #[test]
    fn seven_card_finds_best() {
        // 7h8h on 6h5h4hAcKd → straight flush beats the straight/pair
        let cards = ids(&["7h", "8h", "6h", "5h", "4h", "Ac", "Kd"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::StraightFlush);
    }

    #[test]
    fn seven_card_full_house_over_pair() {
        // AhAs on AdKsKh2c3d → full house (AAA-KK)
        let cards = ids(&["Ah", "As", "Ad", "Ks", "Kh", "2c", "3d"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::FullHouse);
    }

    #[test]
    fn wheel_below_six_high() {
        let wheel = ids(&["As", "2h", "3d", "4c", "5s"]);
        let six_high = ids(&["2s", "3h", "4d", "5c", "6s"]);
        assert!(evaluate_fast(&six_high) > evaluate_fast(&wheel));
    }

    #[test]
    fn seven_card_two_pair_best_kicker() {
        // 7 cards with 3 pairs: should pick best 2 pairs + best kicker
        let cards = ids(&["As", "Ad", "Kh", "Kd", "Qs", "Qd", "Jc"]);
        let score = evaluate_fast(&cards);
        assert_eq!(category_from_score(score), HandCategory::TwoPair);
        let kickers = kickers_from_score(score);
        assert_eq!(kickers[0], 14); // Ace pair
        assert_eq!(kickers[1], 13); // King pair
        assert_eq!(kickers[2], 12); // Queen kicker (from 3rd pair)
    }
}
