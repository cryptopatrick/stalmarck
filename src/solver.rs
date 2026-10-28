//! Configuration, results and the decision procedure (formal
//! specification §9).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::formula::Formula;
use crate::relation::{CanonicalRelation, Relation};
use crate::saturate::{Budget, Engine, Limit, Stats};
use crate::triples::Triples;

/// Solver configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Maximum Dilemma nesting depth `k` (SPEC-K4). Default: 2.
    pub max_depth: u32,
    /// Share identical subformulas during decomposition (SPEC-D5).
    /// Default: `true`.
    pub sharing: bool,
    /// Stop after this many Dilemma rule applications.
    pub max_branches: Option<u64>,
    /// Stop after this much wall-clock time.
    pub timeout: Option<Duration>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_depth: 2,
            sharing: true,
            max_branches: None,
            timeout: None,
        }
    }
}

/// An assignment of truth values to the input variables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model(BTreeMap<String, bool>);

impl Model {
    /// The value of `variable`, if it occurs in the formula.
    pub fn get(&self, variable: &str) -> Option<bool> {
        self.0.get(variable).copied()
    }

    /// All variables and their values, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool)> {
        self.0.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Why no definite answer was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// Saturation to `max_depth` finished without a contradiction or a
    /// complete assignment.
    DepthExhausted,
    /// `max_branches` was reached.
    BranchLimit,
    /// `timeout` was reached.
    Timeout,
    /// An assignment was found but failed verification. This indicates a
    /// bug, and should be reported.
    ModelCheckFailed,
}

/// Answer to "is the formula satisfiable?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SatResult {
    /// Satisfiable, with a verified model.
    Sat(Model),
    /// Unsatisfiable.
    Unsat,
    /// Undecided at the configured depth or limits.
    Unknown(Unknown),
}

/// Answer to "is the formula valid?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProofResult {
    /// Valid (true under every assignment).
    Valid,
    /// Not valid, with a verified counter-model.
    Invalid(Model),
    /// Undecided at the configured depth or limits.
    Unknown(Unknown),
}

/// A result together with counters and the final relation.
#[derive(Clone, Debug)]
pub struct Report<R> {
    /// The answer.
    pub result: R,
    /// Solver counters.
    pub stats: Stats,
    /// Canonical form of the final formula relation (SPEC-R4), used for
    /// differential testing.
    pub relation: CanonicalRelation,
}

enum Raw {
    Refuted,
    Model(Model),
    Unknown(Unknown),
}

/// Stålmarck's proof procedure with a fixed configuration.
///
/// ```
/// use stalmarck::{Formula, ProofResult, Solver};
///
/// let (a, b, c) = (Formula::var("a"), Formula::var("b"), Formula::var("c"));
/// let lhs = a.clone() & (b.clone() | c.clone());
/// let rhs = (a.clone() & b) | (a & c);
/// let report = Solver::default().prove(&lhs.iff(rhs));
/// assert_eq!(report.result, ProofResult::Valid);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Solver {
    config: Config,
}

impl Solver {
    /// A solver with `config`.
    pub fn new(config: Config) -> Solver {
        Solver { config }
    }

    /// The configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Decides satisfiability of `formula`.
    pub fn check_sat(&self, formula: &Formula) -> Report<SatResult> {
        self.check_sat_triples(&Triples::from_formula(
            formula,
            self.config.sharing,
        ))
    }

    /// Decides validity of `formula`.
    pub fn prove(&self, formula: &Formula) -> Report<ProofResult> {
        self.prove_triples(&Triples::from_formula(
            formula,
            self.config.sharing,
        ))
    }

    /// Decides satisfiability of an already decomposed formula. The
    /// `sharing` option is ignored.
    pub fn check_sat_triples(&self, triples: &Triples) -> Report<SatResult> {
        let (raw, stats, relation) = self.run(triples, true);
        let result = match raw {
            Raw::Refuted => SatResult::Unsat,
            Raw::Model(m) => SatResult::Sat(m),
            Raw::Unknown(u) => SatResult::Unknown(u),
        };
        Report { result, stats, relation }
    }

    /// Decides validity of an already decomposed formula. The `sharing`
    /// option is ignored.
    pub fn prove_triples(&self, triples: &Triples) -> Report<ProofResult> {
        let (raw, stats, relation) = self.run(triples, false);
        let result = match raw {
            Raw::Refuted => ProofResult::Valid,
            Raw::Model(m) => ProofResult::Invalid(m),
            Raw::Unknown(u) => ProofResult::Unknown(u),
        };
        Report { result, stats, relation }
    }

    /// Assumes `root ≡ root_value` and saturates to `max_depth`
    /// (SPEC-D7, SPEC-C1).
    fn run(
        &self,
        triples: &Triples,
        root_value: bool,
    ) -> (Raw, Stats, CanonicalRelation) {
        let budget = Budget {
            max_branches: self.config.max_branches,
            deadline: self.config.timeout.map(|t| Instant::now() + t),
        };
        let mut engine =
            Engine::new(triples.node_count(), triples.triples(), budget);
        let mut rel = Relation::new(triples.node_count());
        engine.start(&mut rel, triples.root(), root_value);
        let limit = engine.saturate(&mut rel, self.config.max_depth).err();

        let raw = if rel.is_contradiction() {
            Raw::Refuted
        } else if let Some(model) = extract_model(triples, &rel, root_value) {
            model
        } else {
            Raw::Unknown(match limit {
                Some(Limit::Branches) => Unknown::BranchLimit,
                Some(Limit::Time) => Unknown::Timeout,
                None => Unknown::DepthExhausted,
            })
        };
        (raw, engine.stats, rel.canonical())
    }
}

/// If every input variable is determined, returns the induced assignment
/// after verifying it (SPEC-C1, SPEC-C2).
fn extract_model(
    triples: &Triples,
    rel: &Relation,
    root_value: bool,
) -> Option<Raw> {
    let mut model = BTreeMap::new();
    for (name, node) in triples.variables() {
        model.insert(name.clone(), rel.value(*node)?);
    }
    let values = triples.evaluate(|name| model[name]);
    let root = triples.root();
    let ok = values[root.node() as usize] != root.is_negated();
    Some(if ok == root_value {
        Raw::Model(Model(model))
    } else {
        Raw::Unknown(Unknown::ModelCheckFailed)
    })
}
