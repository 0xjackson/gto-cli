//! Cross-validation between fast evaluator and original evaluator.
//! Ensures they agree on every hand category, kicker ordering, and
//! comparison result.

use gto_cli::card_encoding::{card_to_index, cards_to_indices, index_to_card};
use gto_cli::cards::{parse_board, parse_card, Card};
use gto_cli::hand_evaluator::{compare_hands, evaluate_hand, HandCategory};
use gto_cli::lookup_eval::{category_from_score, evaluate_fast};

fn c(notation: &str) -> Card {
    parse_card(notation).unwrap()
}

fn fast_score(hole: &[Card], board: &[Card]) -> u32 {
    let indices: Vec<u8> = hole.iter().chain(board.iter()).map(card_to_index).collect();
    evaluate_fast(&indices)
}

// -------------------------------------------------------------------------
// Cross-validation: old and new agree on category
// -------------------------------------------------------------------------

fn assert_same_category(hole: &[Card], board: &[Card], label: &str) {
    let old = evaluate_hand(hole, board).unwrap();
    let new_score = fast_score(hole, board);
    let new_cat = category_from_score(new_score);
    assert_eq!(
        old.category, new_cat,
        "{}: old={:?} new={:?}",
        label, old.category, new_cat
    );
}

#[test]
fn cross_validate_royal_flush() {
    let hole = vec![c("As"), c("Ks")];
    let board = parse_board("QsTsJs2h3d").unwrap();
    assert_same_category(&hole, &board, "royal flush");
}

#[test]
fn cross_validate_straight_flush() {
    let hole = vec![c("9h"), c("8h")];
    let board = parse_board("7h6h5hAcKd").unwrap();
    assert_same_category(&hole, &board, "straight flush");
}

#[test]
fn cross_validate_quads() {
    let hole = vec![c("Ks"), c("Kh")];
    let board = parse_board("KdKc5s2h3d").unwrap();
    assert_same_category(&hole, &board, "quads");
}

#[test]
fn cross_validate_full_house() {
    let hole = vec![c("As"), c("Ah")];
    let board = parse_board("AdKsKh2c3d").unwrap();
    assert_same_category(&hole, &board, "full house");
}

#[test]
fn cross_validate_flush() {
    let hole = vec![c("As"), c("Ts")];
    let board = parse_board("8s5s2sKdQh").unwrap();
    assert_same_category(&hole, &board, "flush");
}

#[test]
fn cross_validate_straight() {
    let hole = vec![c("9s"), c("8h")];
    let board = parse_board("7d6c5sAhKd").unwrap();
    assert_same_category(&hole, &board, "straight");
}

#[test]
fn cross_validate_wheel() {
    let hole = vec![c("As"), c("2h")];
    let board = parse_board("3d4c5sKhQd").unwrap();
    assert_same_category(&hole, &board, "wheel");
}

#[test]
fn cross_validate_trips() {
    let hole = vec![c("Qs"), c("Qh")];
    let board = parse_board("Qd7s3h2cKd").unwrap();
    assert_same_category(&hole, &board, "trips");
}

#[test]
fn cross_validate_two_pair() {
    let hole = vec![c("As"), c("Kh")];
    let board = parse_board("AdKs5c2h3d").unwrap();
    assert_same_category(&hole, &board, "two pair");
}

#[test]
fn cross_validate_one_pair() {
    let hole = vec![c("As"), c("Ah")];
    let board = parse_board("Kd7s3c2h5d").unwrap();
    assert_same_category(&hole, &board, "one pair");
}

#[test]
fn cross_validate_high_card() {
    let hole = vec![c("As"), c("Kh")];
    let board = parse_board("Qd9s3c2h5d").unwrap();
    assert_same_category(&hole, &board, "high card");
}

// -------------------------------------------------------------------------
// Cross-validation: old and new agree on comparison results
// -------------------------------------------------------------------------

fn assert_same_comparison(h1: &[Card], h2: &[Card], board: &[Card], label: &str) {
    let old_cmp = compare_hands(h1, h2, board).unwrap();

    let idx1: Vec<u8> = h1.iter().chain(board.iter()).map(card_to_index).collect();
    let idx2: Vec<u8> = h2.iter().chain(board.iter()).map(card_to_index).collect();
    let s1 = evaluate_fast(&idx1);
    let s2 = evaluate_fast(&idx2);
    let new_cmp = match s1.cmp(&s2) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
    };

    assert_eq!(
        old_cmp, new_cmp,
        "{}: old_cmp={} new_cmp={} (old_scores: h1={:?} h2={:?}, new_scores: h1={} h2={})",
        label,
        old_cmp,
        new_cmp,
        evaluate_hand(h1, board).unwrap().category,
        evaluate_hand(h2, board).unwrap().category,
        s1,
        s2
    );
}

#[test]
fn cross_cmp_flush_beats_straight() {
    let board = parse_board("7s6s5s4dAh").unwrap();
    assert_same_comparison(
        &[c("As"), c("2s")],
        &[c("8h"), c("9h")],
        &board,
        "flush vs straight",
    );
}

#[test]
fn cross_cmp_higher_pair_wins() {
    let board = parse_board("2s5d8cTh3d").unwrap();
    assert_same_comparison(
        &[c("As"), c("Ah")],
        &[c("Ks"), c("Kh")],
        &board,
        "AA vs KK on board",
    );
}

#[test]
fn cross_cmp_kicker_decides() {
    let board = parse_board("As5d8cTh3d").unwrap();
    assert_same_comparison(
        &[c("Ad"), c("Kh")],
        &[c("Ah"), c("Qd")],
        &board,
        "AK vs AQ kicker",
    );
}

#[test]
fn cross_cmp_tie() {
    let board = parse_board("AsKdQhJsTs").unwrap();
    assert_same_comparison(
        &[c("2h"), c("3d")],
        &[c("4h"), c("5d")],
        &board,
        "board plays tie",
    );
}

#[test]
fn cross_cmp_two_pair_kicker() {
    let board = parse_board("AsAd5s5d2c").unwrap();
    assert_same_comparison(
        &[c("Kh"), c("3c")],
        &[c("Qh"), c("3d")],
        &board,
        "AA55K vs AA55Q",
    );
}

#[test]
fn cross_cmp_full_house_pair_rank() {
    let board = parse_board("AsAhAd2c3d").unwrap();
    assert_same_comparison(
        &[c("Ks"), c("Kh")],
        &[c("Qs"), c("Qh")],
        &board,
        "AAAKK vs AAAQQ",
    );
}

#[test]
fn cross_cmp_trips_second_kicker() {
    let board = parse_board("AsAhAdKc2s").unwrap();
    assert_same_comparison(
        &[c("Qh"), c("3d")],
        &[c("Jh"), c("4d")],
        &board,
        "trips A with Q vs J kicker",
    );
}

// -------------------------------------------------------------------------
// Exhaustive: random 7-card hands — old vs new agree
// -------------------------------------------------------------------------

#[test]
fn cross_validate_1000_random_comparisons() {
    use rand::seq::SliceRandom;

    let mut rng = rand::thread_rng();
    let full_deck: Vec<u8> = (0..52).collect();

    for _ in 0..1000 {
        let mut deck = full_deck.clone();
        deck.shuffle(&mut rng);

        // Deal two 2-card hands + 5-card board
        let h1_idx = [deck[0], deck[1]];
        let h2_idx = [deck[2], deck[3]];
        let board_idx = [deck[4], deck[5], deck[6], deck[7], deck[8]];

        let h1_cards: Vec<Card> = h1_idx.iter().map(|&i| index_to_card(i)).collect();
        let h2_cards: Vec<Card> = h2_idx.iter().map(|&i| index_to_card(i)).collect();
        let board_cards: Vec<Card> = board_idx.iter().map(|&i| index_to_card(i)).collect();

        // Old evaluator comparison
        let old_cmp = compare_hands(&h1_cards, &h2_cards, &board_cards).unwrap();

        // New evaluator comparison
        let all1: Vec<u8> = h1_idx.iter().chain(board_idx.iter()).copied().collect();
        let all2: Vec<u8> = h2_idx.iter().chain(board_idx.iter()).copied().collect();
        let s1 = evaluate_fast(&all1);
        let s2 = evaluate_fast(&all2);
        let new_cmp = match s1.cmp(&s2) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        };

        assert_eq!(
            old_cmp, new_cmp,
            "Mismatch on h1={:?} h2={:?} board={:?}: old={} new={} (scores {} vs {})",
            h1_cards, h2_cards, board_cards, old_cmp, new_cmp, s1, s2
        );
    }
}

// -------------------------------------------------------------------------
// Benchmark-style: evaluate many hands to confirm speed
// -------------------------------------------------------------------------

#[test]
fn speed_sanity_check() {
    use std::time::Instant;
    use rand::seq::SliceRandom;

    let mut rng = rand::thread_rng();
    let full_deck: Vec<u8> = (0..52).collect();
    let n = 100_000;

    let start = Instant::now();
    for _ in 0..n {
        let mut deck = full_deck.clone();
        deck.shuffle(&mut rng);
        let cards: [u8; 7] = [deck[0], deck[1], deck[2], deck[3], deck[4], deck[5], deck[6]];
        let _ = evaluate_fast(&cards);
    }
    let elapsed = start.elapsed();
    let per_sec = n as f64 / elapsed.as_secs_f64();

    // In debug mode: ~50K/sec (unoptimized). In release: ~2M+ evals/sec
    // (includes shuffle overhead; raw eval speed is higher).
    // Just check it doesn't catastrophically regress.
    eprintln!("Fast evaluator: {:.0} evals/sec ({:.2}ms for {})", per_sec, elapsed.as_secs_f64() * 1000.0, n);
    assert!(
        per_sec > 10_000.0,
        "Expected >10K evals/sec even in debug, got {:.0}",
        per_sec
    );
}
