//! Implementation B: Rust with embedded Scryer Prolog (feature `scryer`,
//! ADR-0005).
//!
//! Rust parses and decomposes the input, runs the top-level 0-saturation,
//! and drives the top-level Dilemma loop and its merges. Each Dilemma
//! *branch* runs in Prolog (`stalmarck.pl`): the assumption, the simple
//! rules, 0-saturation and any nested dilemmas. The results, final relations
//! and counters equal those of [`Solver`] for the same
//! configuration.
//!
//! Known limitation: the `timeout` is checked only between top-level
//! branches, not inside a Prolog query.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use scryer_prolog::{LeafAnswer, Machine, MachineBuilder, StreamConfig, Term};

use crate::formula::Formula;
use crate::relation::Relation;
use crate::saturate::{BranchOutcome, ExternalBranches, Stats};
use crate::solver::{
    Config, ProofResult, Report, SatResult, Solver, proof_report, sat_report,
};
use crate::triples::{TripleKind, Triples};

const PROGRAM: &str = include_str!("stalmarck.pl");

/// Stålmarck's proof procedure with Dilemma branches executed in Scryer
/// Prolog. It has the same interface and results as [`Solver`].
///
/// ```
/// use stalmarck::{Formula, ProofResult, scryer::ScryerSolver};
///
/// let f: Formula = "(a & b) | (a & c) -> a & (b | c)".parse().unwrap();
/// let report = ScryerSolver::default().prove(&f);
/// assert_eq!(report.result, ProofResult::Valid);
/// ```
#[derive(Clone, Debug, Default)]
pub struct ScryerSolver {
    config: Config,
}

impl ScryerSolver {
    /// A solver with `config`. `config.branch_state` is ignored: Prolog
    /// keeps branch states in persistent maps.
    pub fn new(config: Config) -> ScryerSolver {
        ScryerSolver { config }
    }

    /// Decides satisfiability of `formula`.
    pub fn check_sat(&self, formula: &Formula) -> Report<SatResult> {
        let t = Triples::from_formula(formula, self.config.sharing);
        self.check_sat_triples(&t)
    }

    /// Decides validity of `formula`.
    pub fn prove(&self, formula: &Formula) -> Report<ProofResult> {
        let t = Triples::from_formula(formula, self.config.sharing);
        self.prove_triples(&t)
    }

    /// Decides satisfiability of an already decomposed formula.
    pub fn check_sat_triples(&self, triples: &Triples) -> Report<SatResult> {
        sat_report(self.run(triples, true))
    }

    /// Decides validity of an already decomposed formula.
    pub fn prove_triples(&self, triples: &Triples) -> Report<ProofResult> {
        proof_report(self.run(triples, false))
    }

    fn run(
        &self,
        triples: &Triples,
        root_value: bool,
    ) -> crate::solver::RawReport {
        let start = Instant::now();
        let mut prolog = PrologBranches::new(triples);
        let setup = start.elapsed();
        let solver = Solver::new(self.config.clone());
        let mut raw = solver.run(triples, root_value, Some(&mut prolog));
        raw.3.init += setup;
        raw.3.prolog = prolog.prolog;
        raw.3.to_prolog = prolog.to_prolog;
        raw.3.from_prolog = prolog.from_prolog;
        raw
    }
}

/// One Scryer machine with the branch program and the triples loaded.
struct PrologBranches {
    machine: Machine,
    node_count: u32,
    prolog: Duration,
    to_prolog: Duration,
    from_prolog: Duration,
}

impl PrologBranches {
    fn new(triples: &Triples) -> PrologBranches {
        let mut machine = MachineBuilder::new()
            .with_streams(StreamConfig::in_memory())
            .build();
        machine.consult_module_string(
            "stal",
            format!("{PROGRAM}\n{}", facts(triples)),
        );
        PrologBranches {
            machine,
            node_count: triples.node_count(),
            prolog: Duration::ZERO,
            to_prolog: Duration::ZERO,
            from_prolog: Duration::ZERO,
        }
    }
}

/// The triples as facts, appended to the program (see `stalmarck.pl`).
fn facts(triples: &Triples) -> String {
    let mut s = String::new();
    let n = triples.node_count();
    let _ = writeln!(s, "nnodes({n}).");
    let mut occ: Vec<Vec<usize>> = vec![Vec::new(); n as usize];
    for (i, t) in triples.triples().iter().enumerate() {
        let kind = match t.kind {
            TripleKind::And => "and",
            TripleKind::Eq => "eq",
        };
        let (a, b) = (t.a.code(), t.b.code());
        let _ = writeln!(s, "t({i}, {kind}, {}, {a}, {b}).", t.out);
        let mut nodes = [t.out, t.a.node(), t.b.node()];
        nodes.sort_unstable();
        for (j, &v) in nodes.iter().enumerate() {
            if v != 0 && (j == 0 || nodes[j - 1] != v) {
                occ[v as usize].push(i);
            }
        }
    }
    for (v, ts) in occ.iter().enumerate().filter(|(_, ts)| !ts.is_empty()) {
        let ids: Vec<String> = ts.iter().map(|t| t.to_string()).collect();
        let _ = writeln!(s, "occ({v}, [{}]).", ids.join(","));
    }
    // Nodes without occurrences still need a clause.
    s.push_str("occ(_, []).\n");
    s
}

fn int(t: &Term) -> u64 {
    match t {
        Term::Integer(i) => i.to_string().parse().expect("small integer"),
        t => panic!("Prolog returned {t:?} where an integer was expected"),
    }
}

fn stats(t: &Term) -> Stats {
    let Term::Compound(name, a) = t else {
        panic!("Prolog returned {t:?} instead of stats/8");
    };
    assert!(name == "stats" && a.len() == 8, "bad stats term {t:?}");
    Stats {
        triples_processed: int(&a[0]),
        deductions_attempted: int(&a[1]),
        deductions_successful: int(&a[2]),
        merges: int(&a[3]),
        worklist_insertions: int(&a[4]),
        worklist_removals: int(&a[5]),
        dilemma_branches: int(&a[6]),
        dilemma_productive: int(&a[7]),
    }
}

impl ExternalBranches for PrologBranches {
    fn branch(
        &mut self,
        parent: &Relation,
        v: u32,
        value: bool,
        depth: u32,
        max_branches: Option<u64>,
    ) -> BranchOutcome {
        let t0 = Instant::now();
        let mut state = String::from("[");
        for (i, (u, m, neg)) in parent.classes().into_iter().enumerate() {
            let sep = if i == 0 { "" } else { "," };
            let _ = write!(state, "{sep}c({u},{m},{})", neg as u8);
        }
        state.push(']');
        let constant = if value { 0 } else { 1 };
        let max = max_branches.map_or(-1, |m| m.min(i64::MAX as u64) as i64);
        let query = format!(
            "catch(stal:branch_entry({state},{v},{constant},{depth},{max},Out),\
             E,Out=error(E))."
        );
        let t1 = Instant::now();
        let answers: Vec<_> = self.machine.run_query(query).collect();
        let t2 = Instant::now();
        let out = match answers.first() {
            Some(Ok(LeafAnswer::LeafAnswer { bindings, .. })) => {
                bindings.get("Out").cloned()
            }
            _ => None,
        }
        .unwrap_or_else(|| panic!("Prolog branch query failed: {answers:?}"));
        let outcome = match &out {
            Term::Compound(name, a) if name == "r" && a.len() == 2 => {
                let Term::List(classes) = &a[0] else {
                    panic!("bad class list {:?}", a[0]);
                };
                let classes = classes.iter().map(|c| match c {
                    Term::Compound(n, x) if n == "c" && x.len() == 3 => {
                        (int(&x[0]) as u32, int(&x[1]) as u32, int(&x[2]) == 1)
                    }
                    c => panic!("bad class entry {c:?}"),
                });
                BranchOutcome {
                    relation: Some(Relation::from_classes(
                        self.node_count,
                        classes,
                    )),
                    stats: stats(&a[1]),
                    limited: false,
                }
            }
            Term::Compound(name, a) if name == "contradiction" => {
                BranchOutcome {
                    relation: None,
                    stats: stats(&a[0]),
                    limited: false,
                }
            }
            Term::Compound(name, a) if name == "limit" => BranchOutcome {
                relation: None,
                stats: stats(&a[0]),
                limited: true,
            },
            t => panic!("Prolog branch error: {t:?}"),
        };
        self.prolog += t2 - t1;
        self.to_prolog += t1 - t0;
        self.from_prolog += t2.elapsed();
        outcome
    }
}
