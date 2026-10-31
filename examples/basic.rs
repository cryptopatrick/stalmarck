//! Proves a distributive law valid and prints the solver counters.

use stalmarck::{Config, Formula, ProofResult, Solver};

fn main() {
    let (a, b, c) = (Formula::var("a"), Formula::var("b"), Formula::var("c"));
    // (a ∧ b) ∨ (a ∧ c) → a ∧ (b ∨ c) needs one Dilemma level.
    let f = ((a.clone() & b.clone()) | (a.clone() & c.clone()))
        .implies(a & (b | c));

    for depth in 0..=1 {
        let solver = Solver::new(Config::default().with_max_depth(depth));
        let report = solver.prove(&f);
        let verdict = match &report.result {
            ProofResult::Valid => "valid".to_string(),
            ProofResult::Invalid(m) => format!("invalid, counter-model {m:?}"),
            ProofResult::Unknown(u) => format!("unknown ({u:?})"),
        };
        println!("depth {depth}: {f} is {verdict}");
        println!("  {:?}", report.stats);
    }
}
