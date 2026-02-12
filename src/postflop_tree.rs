//! Generic postflop game tree builder.
//!
//! Constructs a recursive action tree for heads-up postflop play with
//! configurable bet/raise sizes and depth limits. Reusable for river,
//! turn, and flop solvers.
//!
//! For multi-street trees (turn+river), Showdown terminals from the
//! earlier street are replaced with Chance nodes that branch into the
//! next street's action subtrees.

use crate::card_encoding::remaining_deck;

/// Which player is acting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    OOP,
    IP,
}

impl Player {
    pub fn opponent(self) -> Player {
        match self {
            Player::OOP => Player::IP,
            Player::IP => Player::OOP,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Player::OOP => 0,
            Player::IP => 1,
        }
    }
}

/// An action a player can take at an action node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Check,
    Bet(f64),
    Call(f64),
    Raise(f64),
    Fold,
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Action::Check => "Check".to_string(),
            Action::Bet(amt) => format!("Bet {:.1}", amt),
            Action::Call(amt) => format!("Call {:.1}", amt),
            Action::Raise(amt) => format!("Raise {:.1}", amt),
            Action::Fold => "Fold".to_string(),
        }
    }
}

/// How a terminal node was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    Showdown,
    Fold { folder: Player },
}

/// A node in the postflop game tree.
#[derive(Debug)]
pub enum TreeNode {
    Action {
        node_id: u16,
        player: Player,
        pot: f64,
        stacks: [f64; 2],
        actions: Vec<Action>,
        children: Vec<TreeNode>,
    },
    Terminal {
        terminal_type: TerminalType,
        pot: f64,
        stacks: [f64; 2],
        invested: [f64; 2],
    },
    /// Chance node: deals a card, branches into next-street subtrees.
    Chance {
        pot: f64,
        stacks: [f64; 2],
        invested: [f64; 2],
        /// Possible cards to deal (u8 indices, 0-51).
        cards: Vec<u8>,
        /// One child subtree per card (same order as `cards`).
        children: Vec<TreeNode>,
    },
}

impl TreeNode {
    pub fn count_action_nodes(&self) -> usize {
        match self {
            TreeNode::Action { children, .. } => {
                1 + children.iter().map(|c| c.count_action_nodes()).sum::<usize>()
            }
            TreeNode::Chance { children, .. } => {
                children.iter().map(|c| c.count_action_nodes()).sum()
            }
            TreeNode::Terminal { .. } => 0,
        }
    }

    pub fn count_terminal_nodes(&self) -> usize {
        match self {
            TreeNode::Action { children, .. } => {
                children.iter().map(|c| c.count_terminal_nodes()).sum()
            }
            TreeNode::Chance { children, .. } => {
                children.iter().map(|c| c.count_terminal_nodes()).sum()
            }
            TreeNode::Terminal { .. } => 1,
        }
    }
}

/// Configuration for building a postflop game tree.
pub struct TreeConfig {
    /// Bet sizes as fractions of pot (e.g., [0.33, 0.67, 1.0]).
    pub bet_sizes: Vec<f64>,
    /// Raise sizes as fractions of pot when facing a bet.
    pub raise_sizes: Vec<f64>,
    /// Maximum number of raises per street (typically 3).
    pub max_raises: usize,
    /// Starting pot size.
    pub starting_pot: f64,
    /// Effective stack remaining (beyond what's already in the pot).
    pub effective_stack: f64,
    /// Whether to add all-in as an option when it's not already covered.
    pub add_allin: bool,
}

impl TreeConfig {
    pub fn default_river(starting_pot: f64, effective_stack: f64) -> Self {
        TreeConfig {
            bet_sizes: vec![0.33, 0.67, 1.0],
            raise_sizes: vec![1.0],
            max_raises: 3,
            starting_pot,
            effective_stack,
            add_allin: true,
        }
    }

    pub fn default_turn(starting_pot: f64, effective_stack: f64) -> Self {
        TreeConfig {
            bet_sizes: vec![0.5, 1.0],
            raise_sizes: vec![1.0],
            max_raises: 2,
            starting_pot,
            effective_stack,
            add_allin: true,
        }
    }
}

/// Configuration for a turn+river tree.
pub struct TurnTreeConfig {
    pub turn: TreeConfig,
    pub river_bet_sizes: Vec<f64>,
    pub river_raise_sizes: Vec<f64>,
    pub river_max_raises: usize,
    /// 4-card turn board as u8 indices (used to enumerate river cards).
    pub board: Vec<u8>,
}

impl TurnTreeConfig {
    pub fn new(board: Vec<u8>, starting_pot: f64, effective_stack: f64) -> Self {
        TurnTreeConfig {
            turn: TreeConfig::default_turn(starting_pot, effective_stack),
            river_bet_sizes: vec![0.33, 0.67, 1.0],
            river_raise_sizes: vec![1.0],
            river_max_raises: 3,
            board,
        }
    }
}

/// Build a postflop game tree from the given config.
/// Returns the root node and the total number of action nodes.
pub fn build_tree(config: &TreeConfig) -> (TreeNode, u16) {
    let mut next_id: u16 = 0;
    let invested = [0.0, 0.0]; // how much each player has put in beyond starting pot
    let root = build_node(
        config,
        Player::OOP,
        config.starting_pot,
        [config.effective_stack; 2],
        invested,
        0,      // raises this street
        false,  // facing_bet
        0.0,    // amount_to_call
        false,  // check_back (IP checked after OOP check?)
        &mut next_id,
    );
    (root, next_id)
}

#[allow(clippy::too_many_arguments)]
fn build_node(
    config: &TreeConfig,
    player: Player,
    pot: f64,
    stacks: [f64; 2],
    invested: [f64; 2],
    raises: usize,
    facing_bet: bool,
    amount_to_call: f64,
    oop_checked: bool,
    next_id: &mut u16,
) -> TreeNode {
    let pi = player.index();
    let remaining = stacks[pi];

    // If player has no stack left, they can't act
    if remaining < 0.01 {
        return TreeNode::Terminal {
            terminal_type: TerminalType::Showdown,
            pot,
            stacks,
            invested,
        };
    }

    if facing_bet {
        // Facing a bet/raise: Fold / Call / Raise(sizes)
        build_facing_bet(
            config, player, pot, stacks, invested, raises,
            amount_to_call, next_id,
        )
    } else if player == Player::IP && oop_checked {
        // IP acts after OOP check: Check (back) / Bet(sizes)
        build_open_action(config, player, pot, stacks, invested, raises, true, next_id)
    } else if player == Player::OOP {
        // OOP opens the action: Check / Bet(sizes)
        build_open_action(config, player, pot, stacks, invested, raises, false, next_id)
    } else {
        // Shouldn't happen in normal tree building
        TreeNode::Terminal {
            terminal_type: TerminalType::Showdown,
            pot,
            stacks,
            invested,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_open_action(
    config: &TreeConfig,
    player: Player,
    pot: f64,
    stacks: [f64; 2],
    invested: [f64; 2],
    raises: usize,
    is_check_back: bool,
    next_id: &mut u16,
) -> TreeNode {
    let pi = player.index();
    let remaining = stacks[pi];

    let node_id = *next_id;
    *next_id += 1;

    let mut actions = Vec::new();
    let mut children = Vec::new();

    // Check
    actions.push(Action::Check);
    if is_check_back {
        // IP checks back -> showdown
        children.push(TreeNode::Terminal {
            terminal_type: TerminalType::Showdown,
            pot,
            stacks,
            invested,
        });
    } else {
        // OOP checks -> IP acts
        children.push(build_node(
            config, Player::IP, pot, stacks, invested,
            raises, false, 0.0, true, next_id,
        ));
    }

    // Bet sizes
    let mut added_allin = false;
    for &frac in &config.bet_sizes {
        let raw_bet = pot * frac;
        let bet = raw_bet.min(remaining);

        if bet < 0.01 {
            continue;
        }

        // Skip if this is effectively the same as all-in and we already added it
        if (bet - remaining).abs() < 0.01 {
            if added_allin {
                continue;
            }
            added_allin = true;
        }

        actions.push(Action::Bet(bet));

        let mut new_stacks = stacks;
        new_stacks[pi] -= bet;
        let new_pot = pot + bet;
        let mut new_invested = invested;
        new_invested[pi] += bet;

        // Opponent faces this bet
        children.push(build_node(
            config, player.opponent(), new_pot, new_stacks, new_invested,
            raises, true, bet, false, next_id,
        ));
    }

    // All-in option (only if bet sizes are configured — empty bet_sizes means check-only)
    if config.add_allin && !added_allin && remaining > 0.01 && !config.bet_sizes.is_empty() {
        let min_bet_threshold = pot * 0.2;
        if remaining > min_bet_threshold {
            actions.push(Action::Bet(remaining));

            let mut new_stacks = stacks;
            new_stacks[pi] -= remaining;
            let new_pot = pot + remaining;
            let mut new_invested = invested;
            new_invested[pi] += remaining;

            children.push(build_node(
                config, player.opponent(), new_pot, new_stacks, new_invested,
                raises, true, remaining, false, next_id,
            ));
        }
    }

    TreeNode::Action {
        node_id,
        player,
        pot,
        stacks,
        actions,
        children,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_facing_bet(
    config: &TreeConfig,
    player: Player,
    pot: f64,
    stacks: [f64; 2],
    invested: [f64; 2],
    raises: usize,
    amount_to_call: f64,
    next_id: &mut u16,
) -> TreeNode {
    let pi = player.index();
    let remaining = stacks[pi];

    let node_id = *next_id;
    *next_id += 1;

    let mut actions = Vec::new();
    let mut children = Vec::new();

    // Fold
    actions.push(Action::Fold);
    children.push(TreeNode::Terminal {
        terminal_type: TerminalType::Fold { folder: player },
        pot,
        stacks,
        invested,
    });

    // Call
    let call_amount = amount_to_call.min(remaining);
    actions.push(Action::Call(call_amount));
    {
        let mut new_stacks = stacks;
        new_stacks[pi] -= call_amount;
        let new_pot = pot + call_amount;
        let mut new_invested = invested;
        new_invested[pi] += call_amount;

        // Check if both players are all-in or if remaining stack is gone
        let opp_remaining = new_stacks[player.opponent().index()];
        if new_stacks[pi] < 0.01 || opp_remaining < 0.01 {
            // All-in: go to showdown
            children.push(TreeNode::Terminal {
                terminal_type: TerminalType::Showdown,
                pot: new_pot,
                stacks: new_stacks,
                invested: new_invested,
            });
        } else {
            // Call closes the action -> showdown
            children.push(TreeNode::Terminal {
                terminal_type: TerminalType::Showdown,
                pot: new_pot,
                stacks: new_stacks,
                invested: new_invested,
            });
        }
    }

    // Raise options (if under the cap and has remaining stack after calling)
    if raises < config.max_raises {
        let remaining_after_call = remaining - call_amount;
        if remaining_after_call > 0.01 {
            let pot_after_call = pot + call_amount;
            let mut added_allin = false;

            for &frac in &config.raise_sizes {
                let raise_amount = (pot_after_call * frac).min(remaining_after_call);

                if raise_amount < 0.01 {
                    continue;
                }

                let total_put_in = call_amount + raise_amount;

                if (total_put_in - remaining).abs() < 0.01 {
                    if added_allin {
                        continue;
                    }
                    added_allin = true;
                }

                actions.push(Action::Raise(total_put_in));

                let mut new_stacks = stacks;
                new_stacks[pi] -= total_put_in;
                let new_pot = pot + total_put_in;
                let mut new_invested = invested;
                new_invested[pi] += total_put_in;

                // Opponent now faces this raise
                children.push(build_node(
                    config, player.opponent(), new_pot, new_stacks, new_invested,
                    raises + 1, true, raise_amount, false, next_id,
                ));
            }

            // All-in raise
            if config.add_allin && !added_allin && remaining_after_call > 0.01 {
                let total_put_in = remaining;
                actions.push(Action::Raise(total_put_in));

                let mut new_stacks = stacks;
                new_stacks[pi] = 0.0;
                let new_pot = pot + total_put_in;
                let mut new_invested = invested;
                new_invested[pi] += total_put_in;

                children.push(build_node(
                    config, player.opponent(), new_pot, new_stacks, new_invested,
                    raises + 1, true, total_put_in - call_amount, false, next_id,
                ));
            }
        }
    }

    TreeNode::Action {
        node_id,
        player,
        pot,
        stacks,
        actions,
        children,
    }
}

/// Build a turn+river game tree.
///
/// Constructs the turn action tree, then replaces every Showdown terminal
/// with a Chance node that branches into river subtrees (one per possible
/// river card). Fold terminals are left as-is.
///
/// Returns (root, total_action_nodes).
pub fn build_turn_tree(config: &TurnTreeConfig) -> (TreeNode, u16) {
    // Build single-street turn action tree
    let (turn_tree, mut next_id) = build_tree(&config.turn);

    // Possible river cards = 52 minus board cards
    let river_cards = remaining_deck(&config.board);

    // Transform: replace Showdown terminals with Chance → river subtrees
    let root = attach_river_streets(
        turn_tree,
        &config.river_bet_sizes,
        &config.river_raise_sizes,
        config.river_max_raises,
        &river_cards,
        &mut next_id,
    );

    (root, next_id)
}

/// Recursively walk the tree and replace Showdown terminals with
/// Chance nodes leading to river action subtrees.
fn attach_river_streets(
    node: TreeNode,
    river_bet_sizes: &[f64],
    river_raise_sizes: &[f64],
    river_max_raises: usize,
    river_cards: &[u8],
    next_id: &mut u16,
) -> TreeNode {
    match node {
        TreeNode::Terminal {
            terminal_type: TerminalType::Showdown,
            pot,
            stacks,
            invested,
        } => {
            // Replace with Chance node → river subtrees
            let eff_stack = stacks[0].min(stacks[1]);
            let mut children = Vec::with_capacity(river_cards.len());

            for &_card in river_cards {
                let river_config = TreeConfig {
                    bet_sizes: river_bet_sizes.to_vec(),
                    raise_sizes: river_raise_sizes.to_vec(),
                    max_raises: river_max_raises,
                    starting_pot: pot,
                    effective_stack: eff_stack,
                    add_allin: true,
                };
                let river_root = build_node(
                    &river_config,
                    Player::OOP,
                    pot,
                    [eff_stack; 2],
                    invested,
                    0,
                    false,
                    0.0,
                    false,
                    next_id,
                );
                children.push(river_root);
            }

            TreeNode::Chance {
                pot,
                stacks,
                invested,
                cards: river_cards.to_vec(),
                children,
            }
        }
        TreeNode::Terminal { .. } => node, // Fold terminals stay
        TreeNode::Action {
            node_id,
            player,
            pot,
            stacks,
            actions,
            children,
        } => {
            let new_children = children
                .into_iter()
                .map(|c| {
                    attach_river_streets(
                        c,
                        river_bet_sizes,
                        river_raise_sizes,
                        river_max_raises,
                        river_cards,
                        next_id,
                    )
                })
                .collect();
            TreeNode::Action {
                node_id,
                player,
                pot,
                stacks,
                actions,
                children: new_children,
            }
        }
        TreeNode::Chance { .. } => node, // Shouldn't exist yet, pass through
    }
}

/// Metadata about an action node, used to initialize FlatCfr.
#[derive(Debug, Clone, Copy)]
pub struct NodeMeta {
    pub node_id: u16,
    pub player: Player,
    pub num_actions: u8,
}

/// Collect metadata for all action nodes in the tree, sorted by node_id.
pub fn collect_node_metadata(tree: &TreeNode) -> Vec<NodeMeta> {
    let mut metas = Vec::new();
    collect_meta_recursive(tree, &mut metas);
    metas.sort_by_key(|m| m.node_id);
    metas
}

fn collect_meta_recursive(node: &TreeNode, metas: &mut Vec<NodeMeta>) {
    match node {
        TreeNode::Action {
            node_id,
            player,
            actions,
            children,
            ..
        } => {
            metas.push(NodeMeta {
                node_id: *node_id,
                player: *player,
                num_actions: actions.len() as u8,
            });
            for c in children {
                collect_meta_recursive(c, metas);
            }
        }
        TreeNode::Chance { children, .. } => {
            for c in children {
                collect_meta_recursive(c, metas);
            }
        }
        TreeNode::Terminal { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tree_structure() {
        let config = TreeConfig {
            bet_sizes: vec![1.0],
            raise_sizes: vec![],
            max_raises: 0,
            starting_pot: 10.0,
            effective_stack: 20.0,
            add_allin: false,
        };
        let (root, num_nodes) = build_tree(&config);
        assert!(num_nodes > 0);
        assert!(root.count_action_nodes() > 0);
        assert!(root.count_terminal_nodes() > 0);
    }

    #[test]
    fn check_check_leads_to_showdown() {
        let config = TreeConfig {
            bet_sizes: vec![1.0],
            raise_sizes: vec![],
            max_raises: 0,
            starting_pot: 10.0,
            effective_stack: 20.0,
            add_allin: false,
        };
        let (root, _) = build_tree(&config);

        // Root = OOP action, first child (check) should lead to IP action
        if let TreeNode::Action { children, actions, .. } = &root {
            assert_eq!(actions[0], Action::Check);
            // Check leads to IP action node
            if let TreeNode::Action { children: ip_children, actions: ip_actions, .. } = &children[0] {
                assert_eq!(ip_actions[0], Action::Check);
                // IP check back = showdown
                if let TreeNode::Terminal { terminal_type, .. } = &ip_children[0] {
                    assert_eq!(*terminal_type, TerminalType::Showdown);
                } else {
                    panic!("Expected terminal showdown after check-check");
                }
            } else {
                panic!("Expected IP action node after OOP check");
            }
        } else {
            panic!("Root should be action node");
        }
    }

    #[test]
    fn node_ids_unique_and_sequential() {
        let config = TreeConfig::default_river(10.0, 20.0);
        let (root, num_nodes) = build_tree(&config);

        let mut ids = Vec::new();
        collect_node_ids(&root, &mut ids);
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), num_nodes as usize);
        // IDs should be 0..num_nodes
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(id, i as u16);
        }
    }

    fn collect_node_ids(node: &TreeNode, ids: &mut Vec<u16>) {
        match node {
            TreeNode::Action { node_id, children, .. } => {
                ids.push(*node_id);
                for c in children {
                    collect_node_ids(c, ids);
                }
            }
            TreeNode::Chance { children, .. } => {
                for c in children {
                    collect_node_ids(c, ids);
                }
            }
            TreeNode::Terminal { .. } => {}
        }
    }

    #[test]
    fn allin_clamped_to_stack() {
        let config = TreeConfig {
            bet_sizes: vec![2.0], // 200% pot bet = 20.0, but stack is only 5
            raise_sizes: vec![],
            max_raises: 0,
            starting_pot: 10.0,
            effective_stack: 5.0,
            add_allin: false,
        };
        let (root, _) = build_tree(&config);

        if let TreeNode::Action { actions, .. } = &root {
            // Check + Bet(5.0) clamped to stack
            assert_eq!(actions.len(), 2);
            if let Action::Bet(amt) = actions[1] {
                assert!((amt - 5.0).abs() < 0.01, "Bet should be clamped to 5.0, got {}", amt);
            }
        }
    }

    #[test]
    fn no_bets_means_only_check() {
        let config = TreeConfig {
            bet_sizes: vec![],
            raise_sizes: vec![],
            max_raises: 0,
            starting_pot: 10.0,
            effective_stack: 20.0,
            add_allin: false,
        };
        let (root, _) = build_tree(&config);

        if let TreeNode::Action { actions, .. } = &root {
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0], Action::Check);
        }
    }

    // -----------------------------------------------------------------------
    // Turn tree tests
    // -----------------------------------------------------------------------

    #[test]
    fn turn_tree_has_chance_nodes() {
        // Board: 4 turn cards (indices 0,1,2,3)
        let config = TurnTreeConfig::new(vec![0, 1, 2, 3], 10.0, 20.0);
        let (root, _num_nodes) = build_turn_tree(&config);

        // Count chance nodes in the tree
        fn count_chance(node: &TreeNode) -> usize {
            match node {
                TreeNode::Chance { children, .. } => {
                    1 + children.iter().map(count_chance).sum::<usize>()
                }
                TreeNode::Action { children, .. } => {
                    children.iter().map(count_chance).sum()
                }
                TreeNode::Terminal { .. } => 0,
            }
        }

        let num_chance = count_chance(&root);
        assert!(num_chance > 0, "Turn tree should have chance nodes");
    }

    #[test]
    fn turn_tree_chance_node_has_48_children() {
        // 4 board cards → 48 possible river cards
        let config = TurnTreeConfig::new(vec![0, 1, 2, 3], 10.0, 20.0);
        let (root, _) = build_turn_tree(&config);

        // Find first chance node
        fn find_chance(node: &TreeNode) -> Option<usize> {
            match node {
                TreeNode::Chance { children, .. } => Some(children.len()),
                TreeNode::Action { children, .. } => {
                    children.iter().find_map(find_chance)
                }
                TreeNode::Terminal { .. } => None,
            }
        }

        let num_children = find_chance(&root).expect("Should have a chance node");
        assert_eq!(num_children, 48, "48 possible river cards");
    }

    #[test]
    fn turn_tree_node_ids_unique() {
        let config = TurnTreeConfig::new(vec![0, 1, 2, 3], 10.0, 20.0);
        let (root, num_nodes) = build_turn_tree(&config);

        let mut ids = Vec::new();
        collect_node_ids(&root, &mut ids);
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            total,
            "All node IDs should be unique ({} unique out of {})",
            ids.len(),
            total
        );
        assert_eq!(ids.len(), num_nodes as usize);
    }

    #[test]
    fn turn_tree_no_showdown_terminals() {
        // After transformation, no Showdown terminals should remain
        // (they should all be replaced with Chance nodes)
        // Only Fold terminals should survive
        let config = TurnTreeConfig::new(vec![0, 1, 2, 3], 10.0, 20.0);
        let (root, _) = build_turn_tree(&config);

        fn check_no_showdown_in_turn(node: &TreeNode, depth: usize) {
            match node {
                TreeNode::Terminal { terminal_type, .. } => {
                    // Showdown terminals at turn level shouldn't exist.
                    // They should only exist inside river subtrees (after chance nodes).
                    if depth == 0 {
                        assert_ne!(
                            *terminal_type,
                            TerminalType::Showdown,
                            "Turn level should have no Showdown terminals"
                        );
                    }
                }
                TreeNode::Action { children, .. } => {
                    for c in children {
                        check_no_showdown_in_turn(c, depth);
                    }
                }
                TreeNode::Chance { children, .. } => {
                    // Past a chance node, we're in river subtrees — showdowns are OK
                    for c in children {
                        check_no_showdown_in_turn(c, depth + 1);
                    }
                }
            }
        }

        check_no_showdown_in_turn(&root, 0);
    }

    #[test]
    fn turn_tree_collect_metadata() {
        let config = TurnTreeConfig::new(vec![0, 1, 2, 3], 10.0, 20.0);
        let (root, num_nodes) = build_turn_tree(&config);

        let metas = collect_node_metadata(&root);
        assert_eq!(
            metas.len(),
            num_nodes as usize,
            "Metadata count should match total action nodes"
        );

        // Verify sorted by node_id and sequential
        for (i, m) in metas.iter().enumerate() {
            assert_eq!(m.node_id, i as u16);
            assert!(m.num_actions >= 1);
        }
    }
}
