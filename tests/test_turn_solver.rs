//! Tests for the turn solver.

use gto_cli::turn_solver::{solve_turn, TurnSolverConfig};

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

#[test]
fn config_rejects_wrong_board_size() {
    // 3 cards = flop, not turn
    let result = TurnSolverConfig::new("As3h4d", "AA", "KK", 10.0, 20.0, 100);
    assert!(result.is_err());

    // 5 cards = river, not turn
    let result = TurnSolverConfig::new("As3h4d5c8s", "AA", "KK", 10.0, 20.0, 100);
    assert!(result.is_err());
}

#[test]
fn config_accepts_4_card_board() {
    let result = TurnSolverConfig::new("As3h4d5c", "AA", "KK", 10.0, 20.0, 100);
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Basic solver tests (small ranges, low iterations for speed)
// ---------------------------------------------------------------------------

#[test]
fn solver_produces_valid_strategies() {
    let config = TurnSolverConfig::new(
        "2s3h4d5c", // turn board
        "AA,KK",
        "QQ,JJ",
        10.0,
        20.0,
        200,
    )
    .unwrap();

    let result = solve_turn(&config);

    assert!(!result.strategies.is_empty(), "Should have strategies");
    assert!(!result.oop_combos.is_empty(), "Should have OOP combos");
    assert!(!result.ip_combos.is_empty(), "Should have IP combos");

    // All strategies should be valid probability distributions
    for strat in &result.strategies {
        for freq in &strat.frequencies {
            let sum: f64 = freq.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.05,
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
    // AA vs 72o on a board where AA always has the best hand
    // AA should bet aggressively
    let config = TurnSolverConfig::new(
        "Ks9d4c2h", // dry board
        "AA",
        "72o",
        10.0,
        20.0,
        300,
    )
    .unwrap();

    let result = solve_turn(&config);
    assert!(!result.strategies.is_empty());

    let root = &result.strategies[0];
    assert_eq!(root.player, "OOP");

    // AA should be betting more than checking at the root
    for freq in &root.frequencies {
        let check_freq = freq[0];
        let bet_freq: f64 = freq[1..].iter().sum();
        assert!(
            bet_freq > 0.3,
            "AA should bet at reasonable frequency vs 72o (bet={:.2}, check={:.2})",
            bet_freq,
            check_freq
        );
    }
}

#[test]
fn solver_exploitability_reasonable() {
    let config = TurnSolverConfig::new(
        "Ks9d4c2h",
        "AA,KK",
        "QQ,JJ",
        10.0,
        20.0,
        500,
    )
    .unwrap();

    let result = solve_turn(&config);

    // Exploitability should be finite and not wildly large
    assert!(
        result.exploitability.is_finite(),
        "Exploitability should be finite"
    );
    // With 500 iterations, exploitability should be reasonable
    // (not necessarily tiny, but not huge)
    assert!(
        result.exploitability.abs() < 50.0,
        "Exploitability should be reasonable, got {:.4}",
        result.exploitability
    );
}

#[test]
fn solver_empty_range_after_blockers() {
    // Board uses As Ah — range AA has very few combos
    let config = TurnSolverConfig::new(
        "AsAh4d5c",
        "AA",  // Only AdAc survives
        "KK",
        10.0,
        20.0,
        100,
    )
    .unwrap();

    let result = solve_turn(&config);
    // Should still produce a valid result with just 1 OOP combo
    assert_eq!(result.oop_combos.len(), 1, "Only AdAc should survive");
    assert!(!result.strategies.is_empty());
}

#[test]
fn solver_board_str_correct() {
    let config = TurnSolverConfig::new(
        "Ks9d4c2h",
        "AA",
        "KK",
        10.0,
        20.0,
        100,
    )
    .unwrap();

    let result = solve_turn(&config);
    assert_eq!(result.board, "Ks9d4c2h");
}

#[test]
fn solver_iterations_stored() {
    let config = TurnSolverConfig::new(
        "Ks9d4c2h",
        "AA",
        "KK",
        10.0,
        20.0,
        123,
    )
    .unwrap();

    let result = solve_turn(&config);
    assert_eq!(result.iterations, 123);
}

#[test]
fn solver_multiple_actions_available() {
    // With default turn config (50%, 100% pot bets), root should have
    // Check + 2 bet sizes + possibly all-in = 3-4 actions
    let config = TurnSolverConfig::new(
        "Ks9d4c2h",
        "AA,KK,QQ",
        "JJ,TT,99",
        10.0,
        20.0,
        100,
    )
    .unwrap();

    let result = solve_turn(&config);
    let root = &result.strategies[0];
    assert!(
        root.actions.len() >= 3,
        "Should have at least 3 actions (check + bets), got {}",
        root.actions.len()
    );
}
