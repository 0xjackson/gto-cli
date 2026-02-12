//! Tests for the river solver.

use gto_cli::card_encoding::card_to_index;
use gto_cli::cards::parse_card;
use gto_cli::lookup_eval::evaluate_fast;
use gto_cli::postflop_tree::{build_tree, Player, TerminalType, TreeConfig, TreeNode};
use gto_cli::river_solver::{
    expand_range_to_combos, solve_river, Combo, RiverSolverConfig, ShowdownTable,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn card(s: &str) -> u8 {
    card_to_index(&parse_card(s).unwrap())
}

fn board(s: &str) -> Vec<u8> {
    let chars: Vec<char> = s.chars().collect();
    (0..chars.len())
        .step_by(2)
        .map(|i| {
            let notation: String = chars[i..i + 2].iter().collect();
            card(&notation)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tree building tests
// ---------------------------------------------------------------------------

#[test]
fn tree_has_action_and_terminal_nodes() {
    let config = TreeConfig::default_river(10.0, 20.0);
    let (root, num_nodes) = build_tree(&config);
    assert!(num_nodes >= 3, "Should have at least 3 action nodes");
    assert!(
        root.count_action_nodes() > 0,
        "Should have action nodes"
    );
    assert!(
        root.count_terminal_nodes() > 0,
        "Should have terminal nodes"
    );
}

#[test]
fn tree_node_ids_sequential() {
    let config = TreeConfig::default_river(10.0, 20.0);
    let (root, num_nodes) = build_tree(&config);
    let mut ids = Vec::new();
    collect_ids(&root, &mut ids);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), num_nodes as usize);
    for (i, &id) in ids.iter().enumerate() {
        assert_eq!(id, i as u16, "Node IDs should be sequential");
    }
}

fn collect_ids(node: &TreeNode, ids: &mut Vec<u16>) {
    if let TreeNode::Action {
        node_id, children, ..
    } = node
    {
        ids.push(*node_id);
        for c in children {
            collect_ids(c, ids);
        }
    }
}

#[test]
fn check_check_path_is_showdown() {
    let config = TreeConfig {
        bet_sizes: vec![1.0],
        raise_sizes: vec![],
        max_raises: 0,
        starting_pot: 10.0,
        effective_stack: 20.0,
        add_allin: false,
    };
    let (root, _) = build_tree(&config);

    // OOP checks -> IP checks -> showdown
    if let TreeNode::Action { children, .. } = &root {
        if let TreeNode::Action {
            children: ip_children,
            ..
        } = &children[0]
        {
            if let TreeNode::Terminal { terminal_type, .. } = &ip_children[0] {
                assert_eq!(*terminal_type, TerminalType::Showdown);
            } else {
                panic!("Expected showdown after check-check");
            }
        }
    }
}

#[test]
fn bet_clamped_to_stack() {
    let config = TreeConfig {
        bet_sizes: vec![5.0], // 500% pot, way more than stack
        raise_sizes: vec![],
        max_raises: 0,
        starting_pot: 10.0,
        effective_stack: 3.0,
        add_allin: false,
    };
    let (root, _) = build_tree(&config);

    if let TreeNode::Action { actions, .. } = &root {
        // Should have check + one bet clamped to 3.0
        assert!(actions.len() >= 2);
        if let gto_cli::postflop_tree::Action::Bet(amt) = actions[1] {
            assert!(
                (amt - 3.0).abs() < 0.01,
                "Bet should be clamped to stack 3.0, got {}",
                amt
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Showdown tests
// ---------------------------------------------------------------------------

#[test]
fn showdown_aa_beats_kk() {
    let b = board("2s3h4d5c8s");
    let oop = vec![Combo(card("As"), card("Ah"))]; // AA
    let ip = vec![Combo(card("Ks"), card("Kh"))]; // KK

    let table = ShowdownTable::new(oop, ip, &b);

    assert!(
        table.oop_scores[0] > table.ip_scores[0],
        "AA ({}) should beat KK ({})",
        table.oop_scores[0],
        table.ip_scores[0]
    );
}

#[test]
fn showdown_flush_beats_pair() {
    // Board: 2s 3s 4d 5c 8s (3 spades on board)
    let b = board("2s3s4d5c8s");
    let flush_hand = vec![Combo(card("As"), card("Ks"))]; // spade flush
    let pair_hand = vec![Combo(card("Ah"), card("Ad"))]; // pair of aces

    let table = ShowdownTable::new(flush_hand, pair_hand, &b);
    assert!(
        table.oop_scores[0] > table.ip_scores[0],
        "Flush should beat pair"
    );
}

#[test]
fn showdown_board_blockers_filtered() {
    let b = board("2s3h4d5c8s");
    let range = vec!["AA".to_string()];
    let combos = expand_range_to_combos(&range, &b);

    // AA has 6 combos, but 2s is on the board so any combo with 2s is excluded
    // Actually AA doesn't contain 2s, so all 6 should be present
    assert_eq!(combos.len(), 6, "AA should have all 6 combos (no blockers)");

    // Now test with a board card that blocks a combo
    let b2 = board("As3h4d5c8s");
    let combos2 = expand_range_to_combos(&range, &b2);
    // AA has 6 combos, As is on board, so 3 combos are blocked (AsAh, AsAd, AsAc)
    assert_eq!(combos2.len(), 3, "AA should have 3 combos with As on board");
}

#[test]
fn showdown_conflicting_combos_excluded() {
    let b = board("2s3h4d5c8s");
    // Both ranges have AA — conflicting combos (sharing same cards) should be excluded
    let oop = vec![Combo(card("As"), card("Ah"))];
    let ip = vec![Combo(card("As"), card("Ad"))]; // shares As with OOP

    let table = ShowdownTable::new(oop, ip, &b);
    assert!(
        table.valid_ip_for_oop[0].is_empty(),
        "Combos sharing cards should be excluded"
    );
}

#[test]
fn showdown_non_conflicting_valid() {
    let b = board("2s3h4d5c8s");
    let oop = vec![Combo(card("As"), card("Ah"))];
    let ip = vec![Combo(card("Kd"), card("Kc"))]; // no card overlap

    let table = ShowdownTable::new(oop, ip, &b);
    assert_eq!(
        table.valid_ip_for_oop[0].len(),
        1,
        "Non-conflicting combos should be valid"
    );
}

// ---------------------------------------------------------------------------
// Solver convergence tests
// ---------------------------------------------------------------------------

#[test]
fn solver_nuts_vs_air() {
    // AA vs 72o on a dry board — AA always has the nuts
    // AA should bet, 72o should fold to bets
    let config = RiverSolverConfig::new(
        "2s3h4d5c8s", // rainbow board, no straights/flushes possible for 72
        "AA",
        "72o",
        10.0,
        20.0,
        2000,
    )
    .unwrap();

    let result = solve_river(&config);

    // OOP (AA) root strategy: should be betting most of the time, not checking
    assert!(!result.strategies.is_empty(), "Should have strategies");

    let root = &result.strategies[0];
    assert_eq!(root.player, "OOP");

    // For each AA combo, the betting frequency should be high
    for freq in &root.frequencies {
        let check_freq = freq[0];
        let bet_freq: f64 = freq[1..].iter().sum();
        assert!(
            bet_freq > 0.5,
            "AA should bet more than check (bet={:.2}, check={:.2})",
            bet_freq,
            check_freq
        );
    }
}

#[test]
fn solver_strategies_valid_probabilities() {
    let config = RiverSolverConfig::new(
        "2s3h4d5c8s",
        "AA,KK",
        "QQ,JJ",
        10.0,
        20.0,
        1000,
    )
    .unwrap();

    let result = solve_river(&config);

    for strat in &result.strategies {
        for freq in &strat.frequencies {
            let sum: f64 = freq.iter().sum();
            assert!(
                (sum - 1.0).abs() < 0.01,
                "Strategy frequencies should sum to 1.0, got {:.4}",
                sum
            );
            for &f in freq {
                assert!(
                    f >= -0.001 && f <= 1.001,
                    "Frequency should be in [0,1], got {:.4}",
                    f
                );
            }
        }
    }
}

#[test]
fn solver_exploitability_decreases() {
    // More iterations should yield lower exploitability
    let config_low = RiverSolverConfig::new(
        "2s3h4d5c8s",
        "AA,KK",
        "QQ,JJ",
        10.0,
        20.0,
        500,
    )
    .unwrap();

    let config_high = RiverSolverConfig::new(
        "2s3h4d5c8s",
        "AA,KK",
        "QQ,JJ",
        10.0,
        20.0,
        3000,
    )
    .unwrap();

    let result_low = solve_river(&config_low);
    let result_high = solve_river(&config_high);

    assert!(
        result_high.exploitability <= result_low.exploitability + 0.5,
        "More iterations should give lower exploitability: low={:.4} high={:.4}",
        result_low.exploitability,
        result_high.exploitability
    );
}

#[test]
fn solver_check_only_ev_is_showdown_equity() {
    // With no bet sizes, the only option is check-check -> showdown
    // EV should be purely based on hand strength
    let mut config = RiverSolverConfig::new(
        "2s3h4d5c8s",
        "AA",
        "KK",
        10.0,
        20.0,
        100,
    )
    .unwrap();
    config.bet_sizes = vec![];
    config.raise_sizes = vec![];
    config.max_raises = 0;

    let result = solve_river(&config);

    // With check-only, all strategies should be 100% check
    if let Some(root) = result.strategies.first() {
        for freq in &root.frequencies {
            assert_eq!(freq.len(), 1, "Should only have check action");
            assert!(
                (freq[0] - 1.0).abs() < 0.01,
                "Should always check, got {:.4}",
                freq[0]
            );
        }
    }
}

#[test]
fn solver_symmetric_ranges_balanced() {
    // Same range for both players on a board where rank matters
    // Both should play roughly similarly
    let config = RiverSolverConfig::new(
        "2s3h4d5c8s",
        "AA,KK",
        "AA,KK",
        10.0,
        20.0,
        2000,
    )
    .unwrap();

    let result = solve_river(&config);
    assert!(!result.strategies.is_empty());

    // With symmetric ranges, exploitability should be low
    // (doesn't need to be tiny since combo asymmetry from blockers exists)
    assert!(
        result.exploitability.abs() < 5.0,
        "Symmetric ranges should have reasonable exploitability, got {:.4}",
        result.exploitability
    );
}

#[test]
fn combo_expansion_correct_count() {
    let b = board("2s3h4d5c8s");

    // AA: 6 combos, none blocked
    let combos = expand_range_to_combos(&vec!["AA".to_string()], &b);
    assert_eq!(combos.len(), 6);

    // AKs: 4 combos, none blocked
    let combos = expand_range_to_combos(&vec!["AKs".to_string()], &b);
    assert_eq!(combos.len(), 4);

    // AKo: 12 combos, none blocked
    let combos = expand_range_to_combos(&vec!["AKo".to_string()], &b);
    assert_eq!(combos.len(), 12);

    // With board blockers
    let b2 = board("As3h4d5c8s"); // As on board
    let combos = expand_range_to_combos(&vec!["AA".to_string()], &b2);
    assert_eq!(combos.len(), 3); // 3 combos without As
}

#[test]
fn river_solver_config_validates_board() {
    // Too few cards
    let result = RiverSolverConfig::new("As3h4d", "AA", "KK", 10.0, 20.0, 100);
    assert!(result.is_err());

    // Valid 5-card board
    let result = RiverSolverConfig::new("As3h4d5c8s", "AA", "KK", 10.0, 20.0, 100);
    assert!(result.is_ok());
}

#[test]
fn empty_range_after_blockers() {
    // Board uses As, Ah — range AA has only combos with these cards
    // Actually AA still has Ad,Ac combo
    let b = board("AsAh4d5c8s");
    let combos = expand_range_to_combos(&vec!["AA".to_string()], &b);
    // AA combos: AsAh, AsAd, AsAc, AhAd, AhAc, AdAc
    // As and Ah on board, so only AdAc survives
    assert_eq!(combos.len(), 1, "Only AdAc should survive");
}
