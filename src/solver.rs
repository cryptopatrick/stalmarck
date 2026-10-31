//! Configuration, results and the decision procedure (formal
//! specification §9).

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use crate::formula::Formula;
use crate::relation::{CanonicalRelation, Relation};
use crate::saturate::{
    BranchState, Budget, Engine, ExternalBranches, Limit, Stats,
};
use crate::triples::Triples;

/// Solver configuration. Start from [`Config::default`] and adjust it with
/// the `with_*` methods:
///
/// ```
/// use stalmarck::{Config, Solver};
///
/// let solver = Solver::new(Config::default().with_max_depth(1));
/// assert_eq!(solver.config().max_depth, 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// How Dilemma branches keep the parent state. Both strategies give the
    /// same results; they differ in time and memory.
    pub branch_state: BranchState,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_depth: 2,
            sharing: true,
            max_branches: None,
            timeout: None,
            branch_state: BranchState::Rollback,
        }
    }
}

impl Config {
    /// Sets [`max_depth`](Config::max_depth).
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: u32) -> Config {
        self.max_depth = max_depth;
        self
    }

    /// Sets [`sharing`](Config::sharing).
    #[must_use]
    pub fn with_sharing(mut self, sharing: bool) -> Config {
        self.sharing = sharing;
        self
    }

    /// Sets [`max_branches`](Config::max_branches).
    #[must_use]
    pub fn with_max_branches(mut self, max_branches: u64) -> Config {
        self.max_branches = Some(max_branches);
        self
    }

    /// Sets [`timeout`](Config::timeout).
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Config {
        self.timeout = Some(timeout);
        self
    }

    /// Sets [`branch_state`](Config::branch_state).
    #[must_use]
    pub fn with_branch_state(mut self, branch_state: BranchState) -> Config {
        self.branch_state = branch_state;
        self
    }
}

/// An assignment of truth values to the input variables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model(BTreeMap<String, bool>);

impl Model {
    pub(crate) fn from_map(map: BTreeMap<String, bool>) -> Model {
        Model(map)
    }

    /// The number of variables.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the model assigns no variables.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value of `variable`, if it occurs in the formula.
    #[must_use]
    pub fn get(&self, variable: &str) -> Option<bool> {
        self.0.get(variable).copied()
    }

    /// All variables and their values, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool)> {
        self.0.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Why no definite answer was reached.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// The back end failed, with its error message. The relation in the
    /// report is the sound one from before the failure. This indicates a
    /// bug, and should be reported.
    BackendError(String),
}

impl fmt::Display for Unknown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unknown::DepthExhausted => f.write_str("depth exhausted"),
            Unknown::BranchLimit => f.write_str("branch limit reached"),
            Unknown::Timeout => f.write_str("timed out"),
            Unknown::ModelCheckFailed => f.write_str("model check failed"),
            Unknown::BackendError(e) => write!(f, "back end failed: {e}"),
        }
    }
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

impl SatResult {
    /// Whether the formula is satisfiable.
    #[must_use]
    pub fn is_sat(&self) -> bool {
        matches!(self, SatResult::Sat(_))
    }

    /// Whether the formula is unsatisfiable.
    #[must_use]
    pub fn is_unsat(&self) -> bool {
        matches!(self, SatResult::Unsat)
    }

    /// The satisfying model, if there is one.
    #[must_use]
    pub fn model(&self) -> Option<&Model> {
        match self {
            SatResult::Sat(m) => Some(m),
            SatResult::Unsat | SatResult::Unknown(_) => None,
        }
    }
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

impl ProofResult {
    /// Whether the formula is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self, ProofResult::Valid)
    }

    /// Whether the formula is not valid.
    #[must_use]
    pub fn is_invalid(&self) -> bool {
        matches!(self, ProofResult::Invalid(_))
    }

    /// The counter-model, if there is one.
    #[must_use]
    pub fn counter_model(&self) -> Option<&Model> {
        match self {
            ProofResult::Invalid(m) => Some(m),
            ProofResult::Valid | ProofResult::Unknown(_) => None,
        }
    }
}

/// A result together with counters and the final relation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Report<R> {
    /// The answer.
    pub result: R,
    /// Solver counters.
    pub stats: Stats,
    /// Canonical form of the final formula relation (SPEC-R4), used for
    /// differential testing.
    pub relation: CanonicalRelation,
    /// Where the time went.
    pub timing: Timing,
}

/// Wall-clock time spent in the solver (ADR-0008 boundaries).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Timing {
    /// Building indexes and the initial relation.
    pub init: Duration,
    /// Saturation to `max_depth`, including `merge`.
    pub solve: Duration,
    /// Computing and installing Dilemma merges.
    pub merge: Duration,
    /// Time spent inside Prolog queries (Scryer back end only).
    pub prolog: Duration,
    /// Time spent encoding parent states as Prolog queries (Scryer back
    /// end only).
    pub to_prolog: Duration,
    /// Time spent decoding Prolog answers (Scryer back end only).
    pub from_prolog: Duration,
}

pub(crate) type RawReport = (Raw, Stats, CanonicalRelation, Timing);

pub(crate) fn sat_report(
    (raw, stats, relation, timing): RawReport,
) -> Report<SatResult> {
    let result = match raw {
        Raw::Refuted => SatResult::Unsat,
        Raw::Model(m) => SatResult::Sat(m),
        Raw::Unknown(u) => SatResult::Unknown(u),
    };
    Report { result, stats, relation, timing }
}

pub(crate) fn proof_report(
    (raw, stats, relation, timing): RawReport,
) -> Report<ProofResult> {
    let result = match raw {
        Raw::Refuted => ProofResult::Valid,
        Raw::Model(m) => ProofResult::Invalid(m),
        Raw::Unknown(u) => ProofResult::Unknown(u),
    };
    Report { result, stats, relation, timing }
}

pub(crate) enum Raw {
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
    #[must_use]
    pub fn new(config: Config) -> Solver {
        Solver { config }
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Decides satisfiability of `formula`.
    #[must_use]
    pub fn check_sat(&self, formula: &Formula) -> Report<SatResult> {
        self.check_sat_triples(&Triples::from_formula(
            formula,
            self.config.sharing,
        ))
    }

    /// Decides validity of `formula`.
    #[must_use]
    pub fn prove(&self, formula: &Formula) -> Report<ProofResult> {
        self.prove_triples(&Triples::from_formula(
            formula,
            self.config.sharing,
        ))
    }

    /// Decides satisfiability of an already decomposed formula. The
    /// `sharing` option is ignored.
    #[must_use]
    pub fn check_sat_triples(&self, triples: &Triples) -> Report<SatResult> {
        sat_report(self.run(triples, true, None))
    }

    /// Decides validity of an already decomposed formula. The `sharing`
    /// option is ignored.
    #[must_use]
    pub fn prove_triples(&self, triples: &Triples) -> Report<ProofResult> {
        proof_report(self.run(triples, false, None))
    }

    /// Assumes `root ≡ root_value` and saturates to `max_depth`
    /// (SPEC-D7, SPEC-C1).
    pub(crate) fn run(
        &self,
        triples: &Triples,
        root_value: bool,
        external: Option<&mut dyn ExternalBranches>,
    ) -> RawReport {
        let t0 = Instant::now();
        let budget = Budget {
            max_branches: self.config.max_branches,
            deadline: self.config.timeout.map(|t| Instant::now() + t),
        };
        let mut engine = Engine::new(
            triples.node_count(),
            triples.triples(),
            budget,
            self.config.branch_state,
        );
        if let Some(external) = external {
            engine.set_external(external);
        }
        let mut rel = Relation::new(triples.node_count());
        engine.start(&mut rel, triples.root(), root_value);
        let t1 = Instant::now();
        let limit = engine.saturate(&mut rel, self.config.max_depth).err();
        let timing = Timing {
            init: t1 - t0,
            solve: t1.elapsed(),
            merge: engine.merge_time,
            ..Timing::default()
        };

        let raw = if rel.is_contradiction() {
            Raw::Refuted
        } else if let Some(model) = extract_model(triples, &rel, root_value) {
            model
        } else {
            Raw::Unknown(match limit {
                Some(Limit::Branches) => Unknown::BranchLimit,
                Some(Limit::Time) => Unknown::Timeout,
                Some(Limit::Backend(e)) => Unknown::BackendError(e),
                None => Unknown::DepthExhausted,
            })
        };
        (raw, engine.stats, rel.canonical(), timing)
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
