//! Stålmarck's proof procedure for propositional logic.
//!
//! The crate decides satisfiability and validity of propositional formulas
//! with the Dilemma proof system of Sheeran and Stålmarck (FMCAD '98):
//!
//! 1. The formula is decomposed into *triples* `x ↔ (a ∧ b)` and
//!    `x ↔ (a ↔ b)` over literals ([`Triples`]).
//! 2. Equivalences between literals are kept in a *formula relation* and
//!    extended by the simple rules of each triple until nothing changes
//!    (0-saturation).
//! 3. The *Dilemma rule* branches on a node being ⊥ or ⊤, saturates both
//!    branches, and keeps what the two have in common. Repeating this up to
//!    nesting depth `k` is k-saturation.
//!
//! A formula is unsatisfiable when the relation becomes contradictory, and
//! satisfiable when all variables are determined and the resulting
//! assignment checks out. Otherwise the answer is *unknown* at the chosen
//! depth: the procedure is complete only when `k` is at least the number of
//! variables.
//!
//! ```
//! use stalmarck::{Formula, SatResult, Solver};
//!
//! let (p, q) = (Formula::var("p"), Formula::var("q"));
//! let f = (p.clone() | q.clone()) & !p & !q;
//! assert_eq!(Solver::default().check_sat(&f).result, SatResult::Unsat);
//! ```
//!
//! The procedure follows the project's formal specification
//! (`docs/formal-specification.md` in the repository). This release is the
//! sequential pure-Rust reference implementation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod formula;
mod relation;
mod rules;
mod saturate;
mod solver;
mod triples;

pub use formula::Formula;
pub use relation::CanonicalRelation;
pub use saturate::Stats;
pub use solver::{
    Config, Model, ProofResult, Report, SatResult, Solver, Unknown,
};
pub use triples::{Lit, Triple, TripleKind, Triples};
