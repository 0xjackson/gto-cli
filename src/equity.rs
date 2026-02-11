use std::fmt;

use rand::seq::SliceRandom;
use rayon::prelude::*;

use crate::card_encoding::{card_to_index, remaining_deck};
use crate::cards::{hand_combos, Card};
use crate::error::{GtoError, GtoResult};
use crate::lookup_eval::evaluate_fast;

pub struct EquityResult {
    pub win: f64,
    pub tie: f64,
    pub lose: f64,
    pub simulations: usize,
}

impl EquityResult {
    pub fn equity(&self) -> f64 {
        self.win + self.tie / 2.0
    }
}

impl fmt::Display for EquityResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Win {:.1}% | Tie {:.1}% | Lose {:.1}% (equity: {:.1}%)",
            self.win * 100.0,
            self.tie * 100.0,
            self.lose * 100.0,
            self.equity() * 100.0,
        )
    }
}

pub fn equity_vs_hand(
    hand1: &[Card],
    hand2: &[Card],
    board: Option<&[Card]>,
    simulations: usize,
) -> GtoResult<EquityResult> {
    let board = board.unwrap_or(&[]);

    // Convert everything to u8 indices for the fast path
    let h1: [u8; 2] = [card_to_index(&hand1[0]), card_to_index(&hand1[1])];
    let h2: [u8; 2] = [card_to_index(&hand2[0]), card_to_index(&hand2[1])];
    let board_idx: Vec<u8> = board.iter().map(card_to_index).collect();

    let mut dead = Vec::with_capacity(4 + board.len());
    dead.extend_from_slice(&h1);
    dead.extend_from_slice(&h2);
    dead.extend_from_slice(&board_idx);
    let remaining = remaining_deck(&dead);
    let cards_needed = 5 - board_idx.len();

    let results: Vec<(u64, u64, u64)> = (0..simulations)
        .into_par_iter()
        .map(|_| {
            let mut rng = rand::thread_rng();
            let mut deck = remaining.clone();
            deck.shuffle(&mut rng);

            // Build 7-card hands directly as [u8; 7]
            let mut all1 = [0u8; 7];
            let mut all2 = [0u8; 7];
            all1[0] = h1[0]; all1[1] = h1[1];
            all2[0] = h2[0]; all2[1] = h2[1];
            for (i, &c) in board_idx.iter().chain(deck[..cards_needed].iter()).enumerate() {
                all1[2 + i] = c;
                all2[2 + i] = c;
            }

            let r1 = evaluate_fast(&all1);
            let r2 = evaluate_fast(&all2);

            match r1.cmp(&r2) {
                std::cmp::Ordering::Greater => (1, 0, 0),
                std::cmp::Ordering::Equal => (0, 1, 0),
                std::cmp::Ordering::Less => (0, 0, 1),
            }
        })
        .collect();

    let (wins, ties, losses) = results
        .iter()
        .fold((0u64, 0u64, 0u64), |acc, &(w, t, l)| {
            (acc.0 + w, acc.1 + t, acc.2 + l)
        });

    let total = (wins + ties + losses) as f64;
    Ok(EquityResult {
        win: wins as f64 / total,
        tie: ties as f64 / total,
        lose: losses as f64 / total,
        simulations: total as usize,
    })
}

pub fn equity_vs_range(
    hand: &[Card],
    villain_range: &[String],
    board: Option<&[Card]>,
    simulations: usize,
) -> GtoResult<EquityResult> {
    let board = board.unwrap_or(&[]);

    let hero: [u8; 2] = [card_to_index(&hand[0]), card_to_index(&hand[1])];
    let board_idx: Vec<u8> = board.iter().map(card_to_index).collect();

    // Dead cards for filtering combos
    let dead_set: std::collections::HashSet<Card> = hand.iter().chain(board.iter()).copied().collect();

    // Convert villain combos to u8 index pairs
    let mut all_combos: Vec<[u8; 2]> = Vec::new();
    for notation in villain_range {
        for (c1, c2) in hand_combos(notation)? {
            if !dead_set.contains(&c1) && !dead_set.contains(&c2) {
                all_combos.push([card_to_index(&c1), card_to_index(&c2)]);
            }
        }
    }

    if all_combos.is_empty() {
        return Err(GtoError::NoValidCombos);
    }

    let sims_per = (simulations / all_combos.len()).max(1);
    let cards_needed = 5 - board_idx.len();

    let results: Vec<(u64, u64, u64)> = all_combos
        .par_iter()
        .map(|villain| {
            let mut dead = Vec::with_capacity(4 + board_idx.len());
            dead.extend_from_slice(&hero);
            dead.extend_from_slice(&board_idx);
            dead.extend_from_slice(villain);
            let remaining = remaining_deck(&dead);

            let mut wins = 0u64;
            let mut ties = 0u64;
            let mut losses = 0u64;

            let mut rng = rand::thread_rng();
            for _ in 0..sims_per {
                let mut deck = remaining.clone();
                deck.shuffle(&mut rng);

                let mut all1 = [0u8; 7];
                let mut all2 = [0u8; 7];
                all1[0] = hero[0]; all1[1] = hero[1];
                all2[0] = villain[0]; all2[1] = villain[1];
                for (i, &c) in board_idx.iter().chain(deck[..cards_needed].iter()).enumerate() {
                    all1[2 + i] = c;
                    all2[2 + i] = c;
                }

                let r1 = evaluate_fast(&all1);
                let r2 = evaluate_fast(&all2);

                match r1.cmp(&r2) {
                    std::cmp::Ordering::Greater => wins += 1,
                    std::cmp::Ordering::Equal => ties += 1,
                    std::cmp::Ordering::Less => losses += 1,
                }
            }

            (wins, ties, losses)
        })
        .collect();

    let (wins, ties, losses) = results
        .iter()
        .fold((0u64, 0u64, 0u64), |acc, &(w, t, l)| {
            (acc.0 + w, acc.1 + t, acc.2 + l)
        });

    let total = (wins + ties + losses) as f64;
    Ok(EquityResult {
        win: wins as f64 / total,
        tie: ties as f64 / total,
        lose: losses as f64 / total,
        simulations: total as usize,
    })
}
