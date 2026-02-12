//! Equity-based hand bucketing for flop solver.
//!
//! Groups hand combos into equal-frequency equity buckets to reduce the
//! information set count from ~1000 combos to ~200 buckets. Equity is
//! computed via Monte Carlo sampling against a uniform random opponent.

use rand::Rng;

use crate::lookup_eval::evaluate_fast;

/// Compute equity of a specific combo (c0, c1) against a uniformly random
/// opponent hand on the given board, using Monte Carlo sampling.
///
/// For river (5 cards): exhaustive enumeration over all possible opponent hands.
/// For flop/turn (3-4 cards): Monte Carlo sampling of runouts + opponents.
pub fn combo_equity_vs_random(c0: u8, c1: u8, board: &[u8], num_samples: usize) -> f64 {
    let board_len = board.len();

    // Build dead card set
    let mut dead = [false; 52];
    dead[c0 as usize] = true;
    dead[c1 as usize] = true;
    for &b in board {
        dead[b as usize] = true;
    }

    if board_len == 5 {
        // River: exhaustive evaluation
        exhaustive_river_equity(c0, c1, board, &dead)
    } else {
        // Flop or turn: Monte Carlo
        monte_carlo_equity(c0, c1, board, &dead, num_samples)
    }
}

/// Exhaustive equity on the river (5-card board).
fn exhaustive_river_equity(c0: u8, c1: u8, board: &[u8], dead: &[bool; 52]) -> f64 {
    let my_score = evaluate_fast(&[c0, c1, board[0], board[1], board[2], board[3], board[4]]);

    let live: Vec<u8> = (0..52u8).filter(|&c| !dead[c as usize]).collect();
    let n = live.len();
    let mut wins = 0.0;
    let mut total = 0.0;

    for i in 0..n {
        for j in (i + 1)..n {
            let opp_score = evaluate_fast(&[
                live[i],
                live[j],
                board[0],
                board[1],
                board[2],
                board[3],
                board[4],
            ]);
            total += 1.0;
            if my_score > opp_score {
                wins += 1.0;
            } else if my_score == opp_score {
                wins += 0.5;
            }
        }
    }

    if total > 0.0 {
        wins / total
    } else {
        0.5
    }
}

/// Monte Carlo equity for flop/turn boards.
fn monte_carlo_equity(
    c0: u8,
    c1: u8,
    board: &[u8],
    dead: &[bool; 52],
    num_samples: usize,
) -> f64 {
    let live: Vec<u8> = (0..52u8).filter(|&c| !dead[c as usize]).collect();
    let cards_needed = 5 - board.len(); // cards to complete the board
    let mut rng = rand::thread_rng();

    let mut wins = 0.0;
    let mut total = 0.0;

    for _ in 0..num_samples {
        // We need `cards_needed` runout cards + 2 opponent cards
        let needed = cards_needed + 2;
        if live.len() < needed {
            break;
        }

        // Fisher-Yates partial shuffle to pick `needed` cards
        let mut deck = live.clone();
        for k in 0..needed {
            let idx = rng.gen_range(k..deck.len());
            deck.swap(k, idx);
        }

        // Build full 5-card board
        let mut full_board = [0u8; 5];
        for (i, &b) in board.iter().enumerate() {
            full_board[i] = b;
        }
        for i in 0..cards_needed {
            full_board[board.len() + i] = deck[i];
        }

        // Opponent cards
        let opp0 = deck[cards_needed];
        let opp1 = deck[cards_needed + 1];

        let my_score = evaluate_fast(&[
            c0,
            c1,
            full_board[0],
            full_board[1],
            full_board[2],
            full_board[3],
            full_board[4],
        ]);
        let opp_score = evaluate_fast(&[
            opp0,
            opp1,
            full_board[0],
            full_board[1],
            full_board[2],
            full_board[3],
            full_board[4],
        ]);

        total += 1.0;
        if my_score > opp_score {
            wins += 1.0;
        } else if my_score == opp_score {
            wins += 0.5;
        }
    }

    if total > 0.0 {
        wins / total
    } else {
        0.5
    }
}

/// Assign combos to equity buckets using equal-frequency binning.
///
/// Returns a Vec<u16> of the same length as `combos`, where each element
/// is the bucket index (0..num_buckets-1) for that combo.
///
/// Combos are sorted by equity, then divided into `num_buckets` equally-sized
/// groups. If there are fewer combos than buckets, each combo gets its own bucket.
pub fn assign_buckets(
    combos: &[(u8, u8)],
    board: &[u8],
    num_buckets: usize,
    num_samples: usize,
) -> Vec<u16> {
    let n = combos.len();
    if n == 0 {
        return vec![];
    }

    // Compute equity for each combo
    let equities: Vec<f64> = combos
        .iter()
        .map(|&(c0, c1)| combo_equity_vs_random(c0, c1, board, num_samples))
        .collect();

    // Sort by equity, keeping track of original indices
    let mut indexed: Vec<(usize, f64)> = equities.iter().enumerate().map(|(i, &e)| (i, e)).collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    // Equal-frequency binning
    let actual_buckets = num_buckets.min(n);
    let mut result = vec![0u16; n];

    for (rank, &(orig_idx, _)) in indexed.iter().enumerate() {
        let bucket = (rank * actual_buckets / n).min(actual_buckets - 1);
        result[orig_idx] = bucket as u16;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_encoding::card_to_index;
    use crate::cards::parse_board;

    fn board_indices(s: &str) -> Vec<u8> {
        parse_board(s)
            .unwrap()
            .iter()
            .map(|c| card_to_index(c))
            .collect()
    }

    #[test]
    fn river_equity_aces_vs_random() {
        // AA on a low board should have very high equity
        let board = board_indices("2s3h4d5c8h");
        // As Ac
        let c0 = card_to_index(&crate::cards::parse_card("As").unwrap());
        let c1 = card_to_index(&crate::cards::parse_card("Ac").unwrap());

        let eq = combo_equity_vs_random(c0, c1, &board, 0);
        assert!(eq > 0.7, "AA should have high equity on low board, got {:.3}", eq);
    }

    #[test]
    fn river_equity_low_hand() {
        // 72o on a high board should have low equity
        let board = board_indices("AsKdQc9h6s");
        let c0 = card_to_index(&crate::cards::parse_card("7h").unwrap());
        let c1 = card_to_index(&crate::cards::parse_card("2c").unwrap());

        let eq = combo_equity_vs_random(c0, c1, &board, 0);
        assert!(eq < 0.5, "72o should have low equity on AKQ96 board, got {:.3}", eq);
    }

    #[test]
    fn flop_equity_reasonable() {
        let board = board_indices("Ks9d4c");
        let c0 = card_to_index(&crate::cards::parse_card("As").unwrap());
        let c1 = card_to_index(&crate::cards::parse_card("Ah").unwrap());

        let eq = combo_equity_vs_random(c0, c1, &board, 500);
        assert!(eq > 0.5, "AA should have >50% equity on flop, got {:.3}", eq);
        assert!(eq < 1.0, "Equity should be <1.0, got {:.3}", eq);
    }

    #[test]
    fn bucket_assignment_correct_count() {
        let board = board_indices("2s3h4d5c8h");
        let combos: Vec<(u8, u8)> = vec![(48, 49), (44, 45), (40, 41), (36, 37)]; // 4 combos
        let buckets = assign_buckets(&combos, &board, 2, 0);
        assert_eq!(buckets.len(), 4);
        // Should have 2 in each bucket
        let b0_count = buckets.iter().filter(|&&b| b == 0).count();
        let b1_count = buckets.iter().filter(|&&b| b == 1).count();
        assert_eq!(b0_count, 2);
        assert_eq!(b1_count, 2);
    }

    #[test]
    fn bucket_more_buckets_than_combos() {
        let board = board_indices("2s3h4d5c8h");
        let combos: Vec<(u8, u8)> = vec![(48, 49), (44, 45)]; // 2 combos
        let buckets = assign_buckets(&combos, &board, 10, 0);
        assert_eq!(buckets.len(), 2);
        // Each combo should be in its own bucket (0 and 1)
        assert!(buckets[0] != buckets[1] || combos.len() <= 1);
    }

    #[test]
    fn bucket_empty_combos() {
        let board = board_indices("2s3h4d5c8h");
        let combos: Vec<(u8, u8)> = vec![];
        let buckets = assign_buckets(&combos, &board, 10, 0);
        assert!(buckets.is_empty());
    }
}
