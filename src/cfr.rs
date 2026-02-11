//! Core CFR+ (Counterfactual Regret Minimization Plus) algorithm.
//!
//! Each information set tracks cumulative regret per action and cumulative
//! strategy weights. The average strategy over all iterations converges to
//! a Nash equilibrium.

use std::collections::HashMap;

/// One information set's accumulated data.
#[derive(Debug, Clone)]
pub struct InfoSetData {
    /// Number of actions available at this information set.
    pub num_actions: usize,
    /// Cumulative regret for each action (floored to 0 in CFR+).
    pub cumulative_regret: Vec<f64>,
    /// Cumulative strategy weight for each action (for computing average strategy).
    pub cumulative_strategy: Vec<f64>,
}

impl InfoSetData {
    pub fn new(num_actions: usize) -> Self {
        InfoSetData {
            num_actions,
            cumulative_regret: vec![0.0; num_actions],
            cumulative_strategy: vec![0.0; num_actions],
        }
    }

    /// Current strategy via regret matching: proportional to positive regrets.
    /// If all regrets are non-positive, returns uniform distribution.
    pub fn current_strategy(&self) -> Vec<f64> {
        let positive_sum: f64 = self
            .cumulative_regret
            .iter()
            .map(|&r| r.max(0.0))
            .sum();

        if positive_sum > 0.0 {
            self.cumulative_regret
                .iter()
                .map(|&r| r.max(0.0) / positive_sum)
                .collect()
        } else {
            vec![1.0 / self.num_actions as f64; self.num_actions]
        }
    }

    /// Average strategy over all iterations — this is the actual Nash
    /// equilibrium approximation.
    pub fn average_strategy(&self) -> Vec<f64> {
        let total: f64 = self.cumulative_strategy.iter().sum();
        if total > 0.0 {
            self.cumulative_strategy.iter().map(|&s| s / total).collect()
        } else {
            vec![1.0 / self.num_actions as f64; self.num_actions]
        }
    }

    /// Update regrets and strategy weights after one traversal.
    /// `action_utilities`: the counterfactual value of each action.
    /// `reach_prob`: the probability of reaching this info set (for strategy weighting).
    pub fn update(&mut self, action_utilities: &[f64], node_utility: f64, reach_prob: f64) {
        let strategy = self.current_strategy();

        for a in 0..self.num_actions {
            // Regret = "how much better action a would have been"
            let regret = action_utilities[a] - node_utility;

            // CFR+: floor cumulative regret at 0
            self.cumulative_regret[a] = (self.cumulative_regret[a] + regret).max(0.0);

            // Accumulate strategy weighted by reach probability
            self.cumulative_strategy[a] += reach_prob * strategy[a];
        }
    }
}

/// Key for an information set: encodes what the player knows.
/// For push/fold: the canonical hand index (0-168) + the decision point.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InfoSetKey {
    /// Canonical hand bucket (0-168 for preflop hands).
    pub hand_bucket: u16,
    /// Which decision point in the game tree (0 = SB push/fold, 1 = BB call/fold).
    pub node_id: u16,
}

/// The CFR trainer holds all information set data.
pub struct CfrTrainer {
    pub info_sets: HashMap<InfoSetKey, InfoSetData>,
}

impl CfrTrainer {
    pub fn new() -> Self {
        CfrTrainer {
            info_sets: HashMap::new(),
        }
    }

    /// Get or create an information set entry.
    pub fn get_or_create(&mut self, key: &InfoSetKey, num_actions: usize) -> &mut InfoSetData {
        self.info_sets
            .entry(key.clone())
            .or_insert_with(|| InfoSetData::new(num_actions))
    }

    /// Get the current strategy for an info set (read-only).
    pub fn get_strategy(&self, key: &InfoSetKey, num_actions: usize) -> Vec<f64> {
        match self.info_sets.get(key) {
            Some(data) => data.current_strategy(),
            None => vec![1.0 / num_actions as f64; num_actions],
        }
    }

    /// Get the converged average strategy.
    pub fn get_average_strategy(&self, key: &InfoSetKey, num_actions: usize) -> Vec<f64> {
        match self.info_sets.get(key) {
            Some(data) => data.average_strategy(),
            None => vec![1.0 / num_actions as f64; num_actions],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_with_no_regret() {
        let data = InfoSetData::new(3);
        let strat = data.current_strategy();
        assert_eq!(strat.len(), 3);
        for &p in &strat {
            assert!((p - 1.0 / 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn regret_matching_proportional() {
        let mut data = InfoSetData::new(2);
        data.cumulative_regret = vec![3.0, 1.0];
        let strat = data.current_strategy();
        assert!((strat[0] - 0.75).abs() < 1e-9);
        assert!((strat[1] - 0.25).abs() < 1e-9);
    }

    #[test]
    fn negative_regret_floored() {
        let mut data = InfoSetData::new(2);
        data.cumulative_regret = vec![-5.0, 3.0];
        let strat = data.current_strategy();
        // -5 floors to 0, so all weight on action 1
        assert!((strat[0] - 0.0).abs() < 1e-9);
        assert!((strat[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn average_strategy_accumulates() {
        let mut data = InfoSetData::new(2);
        // Simulate two updates with different strategies
        data.cumulative_strategy = vec![0.6, 0.4];
        let avg = data.average_strategy();
        assert!((avg[0] - 0.6).abs() < 1e-9);
        assert!((avg[1] - 0.4).abs() < 1e-9);
    }

    #[test]
    fn cfr_plus_floors_regret() {
        let mut data = InfoSetData::new(2);
        data.cumulative_regret = vec![1.0, 1.0];
        // Action 0 had utility -10, action 1 had utility 5, node utility = 0
        data.update(&[-10.0, 5.0], 0.0, 1.0);
        // regret[0] = max(1.0 + (-10 - 0), 0) = max(-9, 0) = 0
        // regret[1] = max(1.0 + (5 - 0), 0) = 6.0
        assert!((data.cumulative_regret[0] - 0.0).abs() < 1e-9);
        assert!((data.cumulative_regret[1] - 6.0).abs() < 1e-9);
    }

    #[test]
    fn trainer_get_or_create() {
        let mut trainer = CfrTrainer::new();
        let key = InfoSetKey { hand_bucket: 0, node_id: 0 };
        trainer.get_or_create(&key, 2);
        assert!(trainer.info_sets.contains_key(&key));
    }
}
