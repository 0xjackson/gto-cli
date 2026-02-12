//! Tests for the flop solver.

use gto_cli::flop_solver::{solve_flop, FlopSolverConfig};

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

#[test]
fn config_rejects_wrong_board_size() {
    // 4 cards = turn, not flop
    let result = FlopSolverConfig::new("As3h4d5c", "AA", "KK", 10.0, 50.0, 100);
    assert!(result.is_err());

    // 5 cards = river, not flop
    let result = FlopSolverConfig::new("As3h4d5c8s", "AA", "KK", 10.0, 50.0, 100);
    assert!(result.is_err());

    // 2 cards = not enough
    let result = FlopSolverConfig::new("As3h", "AA", "KK", 10.0, 50.0, 100);
    assert!(result.is_err());
}

#[test]
fn config_accepts_3_card_board() {
    let result = FlopSolverConfig::new("As3h4d", "AA", "KK", 10.0, 50.0, 100);
    assert!(result.is_ok());
}

#[test]
fn config_rejects_empty_oop_range() {
    let result = FlopSolverConfig::new("As3h4d", "", "KK", 10.0, 50.0, 100);
    assert!(result.is_err());
}

#[test]
fn config_rejects_empty_ip_range() {
    let result = FlopSolverConfig::new("As3h4d", "AA", "", 10.0, 50.0, 100);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Basic solver tests (small ranges, low iterations for speed)
// ---------------------------------------------------------------------------

#[test]
fn solver_produces_valid_strategies() {
    let config = FlopSolverConfig::new(
        "2s3h4d", // flop board
        "AA,KK",
        "QQ,JJ",
        10.0,
        50.0,
        1000,
    )
    .unwrap();

    let result = solve_flop(&config);

    assert!(!result.strategies.is_empty(), "Should have strategies");
    assert!(!result.oop_combos.is_empty(), "Should have OOP combos");
    assert!(!result.ip_combos.is_empty(), "Should have IP combos");

    // All strategies should be valid probability distributions
    for strat in &result.strategies {
        for freq in &strat.frequencies {
            let sum: f64 = freq.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.1,
                "Strategy frequencies should sum to ~1.0, got {:.4} at node {}",
                sum,
                strat.node_id
            );
            for &f in freq {
                assert!(
                    f >= -0.01 && f <= 1.01,
                    "Frequency should be in [0,1], got {:.4}",
                    f
                );
            }
        }
    }
}

#[test]
fn solver_nuts_vs_air() {
    // AA vs 72o on a dry board — AA should bet aggressively
    let config = FlopSolverConfig::new(
        "Ks9d4c", // dry board
        "AA",
        "72o",
        10.0,
        50.0,
        5000,
    )
    .unwrap();

    let result = solve_flop(&config);
    assert!(!result.strategies.is_empty());

    let root = &result.strategies[0];
    assert_eq!(root.player, "OOP");

    // AA should be betting more than checking at the root
    for freq in &root.frequencies {
        let bet_freq: f64 = freq[1..].iter().sum();
        assert!(
            bet_freq > 0.2,
            "AA should bet at reasonable frequency vs 72o (bet={:.2})",
            bet_freq,
        );
    }
}

#[test]
fn solver_exploitability_finite() {
    let config = FlopSolverConfig::new(
        "Ks9d4c",
        "AA,KK",
        "QQ,JJ",
        10.0,
        50.0,
        2000,
    )
    .unwrap();

    let result = solve_flop(&config);

    assert!(
        result.exploitability.is_finite(),
        "Exploitability should be finite"
    );
}

#[test]
fn solver_board_str_correct() {
    let config = FlopSolverConfig::new(
        "Ks9d4c",
        "AA",
        "KK",
        10.0,
        50.0,
        500,
    )
    .unwrap();

    let result = solve_flop(&config);
    assert_eq!(result.board, "Ks9d4c");
}

#[test]
fn solver_iterations_stored() {
    let config = FlopSolverConfig::new(
        "Ks9d4c",
        "AA",
        "KK",
        10.0,
        50.0,
        1234,
    )
    .unwrap();

    let result = solve_flop(&config);
    assert_eq!(result.iterations, 1234);
}

#[test]
fn solver_multiple_actions_available() {
    // With default flop config (33%, 75% pot bets), root should have
    // Check + 2 bet sizes + possibly all-in = 3-4 actions
    let config = FlopSolverConfig::new(
        "Ks9d4c",
        "AA,KK,QQ",
        "JJ,TT,99",
        10.0,
        50.0,
        500,
    )
    .unwrap();

    let result = solve_flop(&config);
    let root = &result.strategies[0];
    assert!(
        root.actions.len() >= 3,
        "Should have at least 3 actions (check + bets), got {}",
        root.actions.len()
    );
}
