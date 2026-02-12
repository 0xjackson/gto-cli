mod card_encoding;
mod cards;
mod cfr;
mod cli;
mod display;
mod equity;
mod error;
mod flat_cfr;
mod game_tree;
mod hand_evaluator;
mod lookup_eval;
mod math_engine;
mod multiway;
mod play;
mod postflop;
mod postflop_tree;
mod preflop;
mod preflop_solver;
mod ranges;
mod river_solver;
mod turn_solver;

fn main() {
    cli::run();
}
