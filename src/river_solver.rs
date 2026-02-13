//! River CFR+ solver.
//!
//! Solves heads-up river spots using CFR+ with exact showdown evaluation.
//! Works at the individual combo level (not canonical 169 buckets) because
//! board interactions depend on exact suits.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::card_encoding::card_to_index;
use crate::cards::{hand_combos, parse_board};
use crate::cfr::{CfrTrainer, InfoSetKey};
use crate::lookup_eval::evaluate_fast;
use crate::postflop_tree::{build_tree, Player, TerminalType, TreeConfig, TreeNode};
use crate::ranges::parse_range;

// ---------------------------------------------------------------------------
// Combo representation
// ---------------------------------------------------------------------------

/// A specific two-card combo as u8 indices (0-51).
#[derive(Debug, Clone, Copy)]
pub struct Combo(pub u8, pub u8);

/// Expand a canonical range (["AA", "AKs", ...]) into specific combos,
/// filtering out any combos that conflict with the board.
pub fn expand_range_to_combos(range: &[String], board: &[u8]) -> Vec<Combo> {
    let board_set: [bool; 52] = {
        let mut s = [false; 52];
        for &b in board {
            s[b as usize] = true;
        }
        s
    };

    let mut combos = Vec::new();
    for hand in range {
        if let Ok(pairs) = hand_combos(hand) {
            for (c1, c2) in pairs {
                let i1 = card_to_index(&c1);
                let i2 = card_to_index(&c2);
                if !board_set[i1 as usize] && !board_set[i2 as usize] {
                    combos.push(Combo(i1, i2));
                }
            }
        }
    }
    combos
}

// ---------------------------------------------------------------------------
// Showdown precomputation
// ---------------------------------------------------------------------------

/// Precomputed showdown data for all valid combo pairs.
pub struct ShowdownTable {
    pub oop_combos: Vec<Combo>,
    pub ip_combos: Vec<Combo>,
    /// For each OOP combo i, the list of valid (non-conflicting) IP combo indices.
    pub valid_ip_for_oop: Vec<Vec<u16>>,
    /// For each IP combo j, the list of valid (non-conflicting) OOP combo indices.
    pub valid_oop_for_ip: Vec<Vec<u16>>,
    /// 7-card eval score for each OOP combo against the board.
    pub oop_scores: Vec<u32>,
    /// 7-card eval score for each IP combo against the board.
    pub ip_scores: Vec<u32>,
}

impl ShowdownTable {
    /// Build the showdown table for a river (5-card board).
    pub fn new(oop_combos: Vec<Combo>, ip_combos: Vec<Combo>, board: &[u8]) -> Self {
        assert_eq!(board.len(), 5, "River board must have exactly 5 cards");

        // Precompute 7-card scores for each combo
        let oop_scores: Vec<u32> = oop_combos
            .iter()
            .map(|c| {
                let hand = [c.0, c.1, board[0], board[1], board[2], board[3], board[4]];
                evaluate_fast(&hand)
            })
            .collect();

        let ip_scores: Vec<u32> = ip_combos
            .iter()
            .map(|c| {
                let hand = [c.0, c.1, board[0], board[1], board[2], board[3], board[4]];
                evaluate_fast(&hand)
            })
            .collect();

        // Build blocker-aware validity tables
        let valid_ip_for_oop: Vec<Vec<u16>> = oop_combos
            .iter()
            .map(|oop| {
                ip_combos
                    .iter()
                    .enumerate()
                    .filter(|(_, ip)| {
                        oop.0 != ip.0 && oop.0 != ip.1 && oop.1 != ip.0 && oop.1 != ip.1
                    })
                    .map(|(j, _)| j as u16)
                    .collect()
            })
            .collect();

        let valid_oop_for_ip: Vec<Vec<u16>> = ip_combos
            .iter()
            .map(|ip| {
                oop_combos
                    .iter()
                    .enumerate()
                    .filter(|(_, oop)| {
                        ip.0 != oop.0 && ip.0 != oop.1 && ip.1 != oop.0 && ip.1 != oop.1
                    })
                    .map(|(i, _)| i as u16)
                    .collect()
            })
            .collect();

        ShowdownTable {
            oop_combos,
            ip_combos,
            valid_ip_for_oop,
            valid_oop_for_ip,
            oop_scores,
            ip_scores,
        }
    }

    pub fn num_oop(&self) -> usize {
        self.oop_combos.len()
    }

    pub fn num_ip(&self) -> usize {
        self.ip_combos.len()
    }
}

// ---------------------------------------------------------------------------
// Solver config & result
// ---------------------------------------------------------------------------

pub struct RiverSolverConfig {
    pub board: Vec<u8>,
    pub oop_range: Vec<String>,
    pub ip_range: Vec<String>,
    pub starting_pot: f64,
    pub effective_stack: f64,
    pub iterations: usize,
    pub bet_sizes: Vec<f64>,
    pub raise_sizes: Vec<f64>,
    pub max_raises: usize,
}

impl RiverSolverConfig {
    pub fn new(
        board_str: &str,
        oop_range_str: &str,
        ip_range_str: &str,
        starting_pot: f64,
        effective_stack: f64,
        iterations: usize,
    ) -> Result<Self, String> {
        let board_cards = parse_board(board_str).map_err(|e| e.to_string())?;
        if board_cards.len() != 5 {
            return Err("River board must have exactly 5 cards".to_string());
        }
        let board: Vec<u8> = board_cards.iter().map(|c| card_to_index(c)).collect();
        let oop_range = parse_range(oop_range_str);
        let ip_range = parse_range(ip_range_str);

        if oop_range.is_empty() {
            return Err("OOP range is empty".to_string());
        }
        if ip_range.is_empty() {
            return Err("IP range is empty".to_string());
        }

        Ok(RiverSolverConfig {
            board,
            oop_range,
            ip_range,
            starting_pot,
            effective_stack,
            iterations,
            bet_sizes: vec![0.33, 0.67, 1.0],
            raise_sizes: vec![1.0],
            max_raises: 3,
        })
    }
}

/// Per-node strategy: action frequencies for each combo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStrategy {
    pub node_id: u16,
    pub player: String,
    pub actions: Vec<String>,
    pub frequencies: Vec<Vec<f64>>, // [combo_idx][action_idx]
}

/// Full solution from the river solver.
#[derive(Debug, Serialize, Deserialize)]
pub struct RiverSolution {
    pub board: String,
    pub oop_range: Vec<String>,
    pub ip_range: Vec<String>,
    pub starting_pot: f64,
    pub effective_stack: f64,
    pub iterations: usize,
    pub exploitability: f64,
    pub oop_combos: Vec<String>,
    pub ip_combos: Vec<String>,
    pub strategies: Vec<NodeStrategy>,
}

// ---------------------------------------------------------------------------
// CFR+ traversal
// ---------------------------------------------------------------------------

/// Solve a river spot.
pub fn solve_river(config: &RiverSolverConfig) -> RiverSolution {
    let tree_config = TreeConfig {
        bet_sizes: config.bet_sizes.clone(),
        raise_sizes: config.raise_sizes.clone(),
        max_raises: config.max_raises,
        starting_pot: config.starting_pot,
        effective_stack: config.effective_stack,
        add_allin: true,
    };

    let (tree, _num_nodes) = build_tree(&tree_config);

    let oop_combos = expand_range_to_combos(&config.oop_range, &config.board);
    let ip_combos = expand_range_to_combos(&config.ip_range, &config.board);

    if oop_combos.is_empty() || ip_combos.is_empty() {
        return empty_solution(config);
    }

    let showdown = ShowdownTable::new(oop_combos, ip_combos, &config.board);
    let mut trainer = CfrTrainer::new();

    // Run alternating CFR+ iterations
    for iter in 0..config.iterations {
        let traverser = if iter % 2 == 0 { Player::OOP } else { Player::IP };

        // Snapshot opponent strategies
        let opp_snapshot = snapshot_strategies(&trainer, &tree, traverser.opponent(), &showdown);

        let num_combos = match traverser {
            Player::OOP => showdown.num_oop(),
            Player::IP => showdown.num_ip(),
        };

        for h in 0..num_combos {
            // Initialize opp_reach: 1.0 for non-conflicting, 0.0 for blocked
            let opp_reach = match traverser {
                Player::OOP => {
                    let valid = &showdown.valid_ip_for_oop[h];
                    let mut reach = vec![0.0f64; showdown.num_ip()];
                    for &j in valid {
                        reach[j as usize] = 1.0;
                    }
                    reach
                }
                Player::IP => {
                    let valid = &showdown.valid_oop_for_ip[h];
                    let mut reach = vec![0.0f64; showdown.num_oop()];
                    for &i in valid {
                        reach[i as usize] = 1.0;
                    }
                    reach
                }
            };

            cfr_traverse(
                &tree,
                traverser,
                h,
                &opp_reach,
                &showdown,
                &opp_snapshot,
                &mut trainer,
            );
        }
    }

    // Extract solution
    extract_solution(config, &tree, &trainer, &showdown)
}

/// Snapshot all opponent strategies for the given player to avoid borrow conflicts.
fn snapshot_strategies(
    trainer: &CfrTrainer,
    tree: &TreeNode,
    player: Player,
    showdown: &ShowdownTable,
) -> HashMap<u16, Vec<Vec<f64>>> {
    let mut snapshot = HashMap::new();
    let num_combos = match player {
        Player::OOP => showdown.num_oop(),
        Player::IP => showdown.num_ip(),
    };
    collect_strategies(tree, player, num_combos, trainer, &mut snapshot);
    snapshot
}

fn collect_strategies(
    node: &TreeNode,
    player: Player,
    num_combos: usize,
    trainer: &CfrTrainer,
    snapshot: &mut HashMap<u16, Vec<Vec<f64>>>,
) {
    match node {
        TreeNode::Action {
            node_id,
            player: node_player,
            children,
            actions,
            ..
        } => {
            if *node_player == player {
                let num_actions = actions.len();
                let strats: Vec<Vec<f64>> = (0..num_combos)
                    .map(|h| {
                        let key = InfoSetKey {
                            hand_bucket: h as u16,
                            node_id: *node_id,
                        };
                        trainer.get_strategy(&key, num_actions)
                    })
                    .collect();
                snapshot.insert(*node_id, strats);
            }
            for child in children {
                collect_strategies(child, player, num_combos, trainer, snapshot);
            }
        }
        TreeNode::Terminal { .. } | TreeNode::Chance { .. } => {}
    }
}

/// Recursive CFR+ traversal for one traverser hand.
/// Returns the counterfactual value of this node for the traverser.
fn cfr_traverse(
    node: &TreeNode,
    traverser: Player,
    hand_idx: usize,
    opp_reach: &[f64],
    showdown: &ShowdownTable,
    opp_snapshot: &HashMap<u16, Vec<Vec<f64>>>,
    trainer: &mut CfrTrainer,
) -> f64 {
    match node {
        TreeNode::Terminal {
            terminal_type, pot, invested, ..
        } => {
            compute_terminal_value(
                *terminal_type, *pot, invested, traverser, hand_idx, opp_reach, showdown,
            )
        }
        TreeNode::Action {
            node_id,
            player,
            children,
            actions,
            ..
        } => {
            let num_actions = actions.len();

            if *player == traverser {
                // Traverser node: compute per-action values, update regrets
                let key = InfoSetKey {
                    hand_bucket: hand_idx as u16,
                    node_id: *node_id,
                };
                let strategy = trainer.get_strategy(&key, num_actions);

                let mut action_values = vec![0.0f64; num_actions];
                let mut node_value = 0.0;

                for a in 0..num_actions {
                    action_values[a] = cfr_traverse(
                        &children[a], traverser, hand_idx, opp_reach,
                        showdown, opp_snapshot, trainer,
                    );
                    node_value += strategy[a] * action_values[a];
                }

                // Compute reach probability (sum of opponent reach)
                let reach_sum: f64 = opp_reach.iter().sum();
                let reach_prob = if reach_sum > 0.0 { 1.0 } else { 0.0 };

                let data = trainer.get_or_create(&key, num_actions);
                data.update(&action_values, node_value, reach_prob);

                node_value
            } else {
                // Opponent node: weight by opponent strategy, propagate modified reach
                let num_opp_combos = opp_reach.len();
                let opp_strats = opp_snapshot.get(node_id);

                let mut node_value = 0.0;

                for a in 0..num_actions {
                    // Build new opp_reach weighted by opponent's strategy for this action
                    let mut new_opp_reach = vec![0.0f64; num_opp_combos];
                    for j in 0..num_opp_combos {
                        if opp_reach[j] > 0.0 {
                            let sigma_j_a = match opp_strats {
                                Some(strats) => strats[j][a],
                                None => 1.0 / num_actions as f64,
                            };
                            new_opp_reach[j] = opp_reach[j] * sigma_j_a;
                        }
                    }

                    node_value += cfr_traverse(
                        &children[a], traverser, hand_idx, &new_opp_reach,
                        showdown, opp_snapshot, trainer,
                    );
                }

                node_value
            }
        }
        TreeNode::Chance { .. } => unreachable!("River solver does not use chance nodes"),
    }
}

/// Compute the terminal payoff for the traverser at a terminal node.
fn compute_terminal_value(
    terminal_type: TerminalType,
    pot: f64,
    invested: &[f64; 2],
    traverser: Player,
    hand_idx: usize,
    opp_reach: &[f64],
    showdown: &ShowdownTable,
) -> f64 {
    let opp_reach_sum: f64 = opp_reach.iter().sum();
    if opp_reach_sum < 1e-10 {
        return 0.0;
    }

    // Payoffs measured relative to start of tree (antes are sunk cost).
    // Win (showdown or opponent folds): pot - invested[traverser]
    // Lose (showdown or traverser folds): -invested[traverser]
    // Tie: pot/2 - invested[traverser]
    let my_invested = invested[traverser.index()];

    match terminal_type {
        TerminalType::Fold { folder } => {
            if folder == traverser {
                // Traverser folds: loses what they invested
                -my_invested * opp_reach_sum
            } else {
                // Opponent folds: traverser wins entire pot minus their investment
                (pot - my_invested) * opp_reach_sum
            }
        }
        TerminalType::Showdown => {
            let win_payoff = pot - my_invested;
            let lose_payoff = -my_invested;
            let tie_payoff = pot / 2.0 - my_invested;
            let mut value = 0.0;

            match traverser {
                Player::OOP => {
                    let my_score = showdown.oop_scores[hand_idx];
                    for &j in &showdown.valid_ip_for_oop[hand_idx] {
                        let j = j as usize;
                        if opp_reach[j] < 1e-10 {
                            continue;
                        }
                        let opp_score = showdown.ip_scores[j];
                        let payoff = if my_score > opp_score {
                            win_payoff
                        } else if my_score < opp_score {
                            lose_payoff
                        } else {
                            tie_payoff
                        };
                        value += opp_reach[j] * payoff;
                    }
                }
                Player::IP => {
                    let my_score = showdown.ip_scores[hand_idx];
                    for &i in &showdown.valid_oop_for_ip[hand_idx] {
                        let i = i as usize;
                        if opp_reach[i] < 1e-10 {
                            continue;
                        }
                        let opp_score = showdown.oop_scores[i];
                        let payoff = if my_score > opp_score {
                            win_payoff
                        } else if my_score < opp_score {
                            lose_payoff
                        } else {
                            tie_payoff
                        };
                        value += opp_reach[i] * payoff;
                    }
                }
            }

            value
        }
    }
}

// ---------------------------------------------------------------------------
// Exploitability
// ---------------------------------------------------------------------------

/// Compute exploitability via best-response traversal.
pub fn compute_exploitability(
    tree: &TreeNode,
    trainer: &CfrTrainer,
    showdown: &ShowdownTable,
) -> f64 {
    let oop_gain = best_response_value(tree, Player::OOP, trainer, showdown);
    let ip_gain = best_response_value(tree, Player::IP, trainer, showdown);
    (oop_gain + ip_gain) / 2.0
}

/// Compute the expected gain from best-response play for one player,
/// given the opponent's average strategy.
fn best_response_value(
    tree: &TreeNode,
    br_player: Player,
    trainer: &CfrTrainer,
    showdown: &ShowdownTable,
) -> f64 {
    let num_br = match br_player {
        Player::OOP => showdown.num_oop(),
        Player::IP => showdown.num_ip(),
    };
    let num_opp = match br_player {
        Player::OOP => showdown.num_ip(),
        Player::IP => showdown.num_oop(),
    };

    let mut total_gain = 0.0;

    for h in 0..num_br {
        // Initialize opp reach
        let opp_reach = match br_player {
            Player::OOP => {
                let valid = &showdown.valid_ip_for_oop[h];
                let mut reach = vec![0.0f64; num_opp];
                for &j in valid {
                    reach[j as usize] = 1.0;
                }
                reach
            }
            Player::IP => {
                let valid = &showdown.valid_oop_for_ip[h];
                let mut reach = vec![0.0f64; num_opp];
                for &i in valid {
                    reach[i as usize] = 1.0;
                }
                reach
            }
        };

        let br_value = br_traverse(tree, br_player, h, &opp_reach, showdown, trainer);

        // Also compute the value using the actual average strategy
        let avg_value = avg_strategy_traverse(tree, br_player, h, &opp_reach, showdown, trainer);

        total_gain += br_value - avg_value;
    }

    total_gain / num_br as f64
}

/// Best-response traversal: for the BR player, pick the best action at each node.
fn br_traverse(
    node: &TreeNode,
    br_player: Player,
    hand_idx: usize,
    opp_reach: &[f64],
    showdown: &ShowdownTable,
    trainer: &CfrTrainer,
) -> f64 {
    match node {
        TreeNode::Terminal { terminal_type, pot, invested, .. } => {
            compute_terminal_value(
                *terminal_type, *pot, invested, br_player, hand_idx, opp_reach, showdown,
            )
        }
        TreeNode::Action { node_id, player, children, actions, .. } => {
            let num_actions = actions.len();

            if *player == br_player {
                // Best response: pick the max-value action
                let mut best = f64::NEG_INFINITY;
                for a in 0..num_actions {
                    let v = br_traverse(
                        &children[a], br_player, hand_idx, opp_reach, showdown, trainer,
                    );
                    if v > best {
                        best = v;
                    }
                }
                best
            } else {
                // Opponent plays average strategy
                let num_opp = opp_reach.len();
                let mut node_value = 0.0;

                for a in 0..num_actions {
                    let mut new_opp_reach = vec![0.0f64; num_opp];
                    for j in 0..num_opp {
                        if opp_reach[j] > 0.0 {
                            let key = InfoSetKey {
                                hand_bucket: j as u16,
                                node_id: *node_id,
                            };
                            let avg = trainer.get_average_strategy(&key, num_actions);
                            new_opp_reach[j] = opp_reach[j] * avg[a];
                        }
                    }
                    node_value += br_traverse(
                        &children[a], br_player, hand_idx, &new_opp_reach, showdown, trainer,
                    );
                }
                node_value
            }
        }
        TreeNode::Chance { .. } => unreachable!("River solver does not use chance nodes"),
    }
}

/// Traverse with both players using average strategies.
fn avg_strategy_traverse(
    node: &TreeNode,
    perspective: Player,
    hand_idx: usize,
    opp_reach: &[f64],
    showdown: &ShowdownTable,
    trainer: &CfrTrainer,
) -> f64 {
    match node {
        TreeNode::Terminal { terminal_type, pot, invested, .. } => {
            compute_terminal_value(
                *terminal_type, *pot, invested, perspective, hand_idx, opp_reach, showdown,
            )
        }
        TreeNode::Action { node_id, player, children, actions, .. } => {
            let num_actions = actions.len();

            if *player == perspective {
                // Use average strategy
                let key = InfoSetKey {
                    hand_bucket: hand_idx as u16,
                    node_id: *node_id,
                };
                let avg = trainer.get_average_strategy(&key, num_actions);

                let mut node_value = 0.0;
                for a in 0..num_actions {
                    let v = avg_strategy_traverse(
                        &children[a], perspective, hand_idx, opp_reach, showdown, trainer,
                    );
                    node_value += avg[a] * v;
                }
                node_value
            } else {
                // Opponent uses average strategy
                let num_opp = opp_reach.len();
                let mut node_value = 0.0;

                for a in 0..num_actions {
                    let mut new_opp_reach = vec![0.0f64; num_opp];
                    for j in 0..num_opp {
                        if opp_reach[j] > 0.0 {
                            let key = InfoSetKey {
                                hand_bucket: j as u16,
                                node_id: *node_id,
                            };
                            let avg = trainer.get_average_strategy(&key, num_actions);
                            new_opp_reach[j] = opp_reach[j] * avg[a];
                        }
                    }
                    node_value += avg_strategy_traverse(
                        &children[a], perspective, hand_idx, &new_opp_reach, showdown, trainer,
                    );
                }
                node_value
            }
        }
        TreeNode::Chance { .. } => unreachable!("River solver does not use chance nodes"),
    }
}

// ---------------------------------------------------------------------------
// Strategy extraction
// ---------------------------------------------------------------------------

fn extract_solution(
    config: &RiverSolverConfig,
    tree: &TreeNode,
    trainer: &CfrTrainer,
    showdown: &ShowdownTable,
) -> RiverSolution {
    let exploitability = compute_exploitability(tree, trainer, showdown);

    let mut strategies = Vec::new();
    extract_node_strategies(tree, trainer, showdown, &mut strategies);

    let board_str = config
        .board
        .iter()
        .map(|&b| {
            let c = crate::card_encoding::index_to_card(b);
            format!("{}", c)
        })
        .collect::<String>();

    let oop_combo_strs: Vec<String> = showdown
        .oop_combos
        .iter()
        .map(|c| {
            let c1 = crate::card_encoding::index_to_card(c.0);
            let c2 = crate::card_encoding::index_to_card(c.1);
            format!("{}{}", c1, c2)
        })
        .collect();

    let ip_combo_strs: Vec<String> = showdown
        .ip_combos
        .iter()
        .map(|c| {
            let c1 = crate::card_encoding::index_to_card(c.0);
            let c2 = crate::card_encoding::index_to_card(c.1);
            format!("{}{}", c1, c2)
        })
        .collect();

    RiverSolution {
        board: board_str,
        oop_range: config.oop_range.clone(),
        ip_range: config.ip_range.clone(),
        starting_pot: config.starting_pot,
        effective_stack: config.effective_stack,
        iterations: config.iterations,
        exploitability,
        oop_combos: oop_combo_strs,
        ip_combos: ip_combo_strs,
        strategies,
    }
}

fn extract_node_strategies(
    node: &TreeNode,
    trainer: &CfrTrainer,
    showdown: &ShowdownTable,
    strategies: &mut Vec<NodeStrategy>,
) {
    match node {
        TreeNode::Action {
            node_id,
            player,
            children,
            actions,
            ..
        } => {
            let num_actions = actions.len();
            let num_combos = match player {
                Player::OOP => showdown.num_oop(),
                Player::IP => showdown.num_ip(),
            };

            let frequencies: Vec<Vec<f64>> = (0..num_combos)
                .map(|h| {
                    let key = InfoSetKey {
                        hand_bucket: h as u16,
                        node_id: *node_id,
                    };
                    trainer.get_average_strategy(&key, num_actions)
                })
                .collect();

            let action_labels: Vec<String> = actions.iter().map(|a| a.label()).collect();

            strategies.push(NodeStrategy {
                node_id: *node_id,
                player: match player {
                    Player::OOP => "OOP".to_string(),
                    Player::IP => "IP".to_string(),
                },
                actions: action_labels,
                frequencies,
            });

            for child in children {
                extract_node_strategies(child, trainer, showdown, strategies);
            }
        }
        TreeNode::Terminal { .. } | TreeNode::Chance { .. } => {}
    }
}

fn empty_solution(config: &RiverSolverConfig) -> RiverSolution {
    let board_str = config
        .board
        .iter()
        .map(|&b| {
            let c = crate::card_encoding::index_to_card(b);
            format!("{}", c)
        })
        .collect::<String>();

    RiverSolution {
        board: board_str,
        oop_range: config.oop_range.clone(),
        ip_range: config.ip_range.clone(),
        starting_pot: config.starting_pot,
        effective_stack: config.effective_stack,
        iterations: config.iterations,
        exploitability: 0.0,
        oop_combos: vec![],
        ip_combos: vec![],
        strategies: vec![],
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl RiverSolution {
    pub fn display(&self) {
        use colored::Colorize;

        println!();
        println!(
            "  {} River Solution  |  Board: {}  |  Pot: {:.0}  |  Stack: {:.0}  |  {} iterations",
            "GTO".bold(),
            self.board,
            self.starting_pot,
            self.effective_stack,
            self.iterations,
        );
        println!(
            "  Exploitability: {:.4}",
            self.exploitability,
        );
        println!(
            "  OOP range: {} ({} combos)  |  IP range: {} ({} combos)",
            self.oop_range.join(","),
            self.oop_combos.len(),
            self.ip_range.join(","),
            self.ip_combos.len(),
        );

        // Display root node strategy (OOP's first decision)
        if let Some(root_strat) = self.strategies.first() {
            println!();
            println!(
                "  {} at root (node {}):",
                root_strat.player.bold(),
                root_strat.node_id
            );
            println!("  Actions: {}", root_strat.actions.join(" | "));

            let num_to_show = root_strat.frequencies.len().min(20);
            let combos = if root_strat.player == "OOP" {
                &self.oop_combos
            } else {
                &self.ip_combos
            };

            for i in 0..num_to_show {
                let freq_str: String = root_strat.frequencies[i]
                    .iter()
                    .zip(&root_strat.actions)
                    .map(|(f, a)| {
                        let pct = (f * 100.0).round() as u32;
                        if pct > 70 {
                            format!("{}:{}", a, format!("{}%", pct).green())
                        } else if pct > 30 {
                            format!("{}:{}", a, format!("{}%", pct).yellow())
                        } else {
                            format!("{}:{}%", a, pct)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("    {}  {}", combos[i].bold(), freq_str);
            }
            if root_strat.frequencies.len() > num_to_show {
                println!("    ... and {} more combos", root_strat.frequencies.len() - num_to_show);
            }
        }

        println!();
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

impl RiverSolution {
    pub fn cache_path(&self) -> std::path::PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let dir = std::path::Path::new(&home).join(".gto-cli").join("solver");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!(
            "river_{}_{:.0}_{:.0}.bin",
            self.board, self.starting_pot, self.effective_stack,
        ))
    }

    pub fn save_cache(&self) {
        if let Ok(data) = bincode::serialize(self) {
            let path = self.cache_path();
            std::fs::write(path, data).ok();
        }
    }

    pub fn load_cache(board: &str, pot: f64, stack: f64) -> Option<RiverSolution> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path = std::path::Path::new(&home)
            .join(".gto-cli")
            .join("solver")
            .join(format!("river_{}_{:.0}_{:.0}.bin", board, pot, stack));
        let data = std::fs::read(path).ok()?;
        bincode::deserialize(&data).ok()
    }
}
