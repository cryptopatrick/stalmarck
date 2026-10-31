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
    #[must_use]
    pub fn new(config: Config) -> ScryerSolver {
        ScryerSolver { config }
    }

    /// Decides satisfiability of `formula`.
    #[must_use]
    pub fn check_sat(&self, formula: &Formula) -> Report<SatResult> {
        let t = Triples::from_formula(formula, self.config.sharing);
        self.check_sat_triples(&t)
    }

    /// Decides validity of `formula`.
    #[must_use]
    pub fn prove(&self, formula: &Formula) -> Report<ProofResult> {
        let t = Triples::from_formula(formula, self.config.sharing);
        self.prove_triples(&t)
    }

    /// Decides satisfiability of an already decomposed formula.
    #[must_use]
    pub fn check_sat_triples(&self, triples: &Triples) -> Report<SatResult> {
        sat_report(self.run(triples, true))
    }

    /// Decides validity of an already decomposed formula.
    #[must_use]
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
        let ids: Vec<String> =
            ts.iter().map(std::string::ToString::to_string).collect();
        let _ = writeln!(s, "occ({v}, [{}]).", ids.join(","));
    }
    // Nodes without occurrences still need a clause.
    s.push_str("occ(_, []).\n");
    s
}

fn int(t: &Term) -> Result<u64, String> {
    match t {
        Term::Integer(i) => {
            i.to_string().parse().map_err(|e| format!("bad integer {i}: {e}"))
        }
        t => Err(format!("expected an integer, got {t:?}")),
    }
}

fn node(t: &Term) -> Result<u32, String> {
    u32::try_from(int(t)?).map_err(|e| format!("bad node: {e}"))
}

fn stats(t: &Term) -> Result<Stats, String> {
    match t {
        Term::Compound(name, a) if name == "stats" && a.len() == 8 => {
            Ok(Stats {
                triples_processed: int(&a[0])?,
                deductions_attempted: int(&a[1])?,
                deductions_successful: int(&a[2])?,
                merges: int(&a[3])?,
                worklist_insertions: int(&a[4])?,
                worklist_removals: int(&a[5])?,
                dilemma_branches: int(&a[6])?,
                dilemma_productive: int(&a[7])?,
            })
        }
        t => Err(format!("expected stats/8, got {t:?}")),
    }
}

fn class(t: &Term) -> Result<(u32, u32, bool), String> {
    match t {
        Term::Compound(name, x) if name == "c" && x.len() == 3 => {
            Ok((node(&x[0])?, node(&x[1])?, int(&x[2])? == 1))
        }
        t => Err(format!("bad class entry {t:?}")),
    }
}

/// Decodes the `Out` binding of `branch_entry/6`.
fn decode(out: &Term, node_count: u32) -> Result<BranchOutcome, String> {
    let Term::Compound(name, a) = out else {
        return Err(format!("Prolog branch error: {out:?}"));
    };
    match (name.as_str(), a.as_slice()) {
        ("r", [Term::List(classes), s]) => {
            let classes =
                classes.iter().map(class).collect::<Result<Vec<_>, _>>()?;
            Ok(BranchOutcome {
                relation: Some(Relation::from_classes(node_count, classes)),
                stats: stats(s)?,
                limited: false,
            })
        }
        ("contradiction", [s]) => Ok(BranchOutcome {
            relation: None,
            stats: stats(s)?,
            limited: false,
        }),
        ("limit", [s]) => Ok(BranchOutcome {
            relation: None,
            stats: stats(s)?,
            limited: true,
        }),
        _ => Err(format!("Prolog branch error: {out:?}")),
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
    ) -> Result<BranchOutcome, String> {
        let t0 = Instant::now();
        let mut state = String::from("[");
        for (i, (u, m, neg)) in parent.classes().into_iter().enumerate() {
            let sep = if i == 0 { "" } else { "," };
            let _ = write!(state, "{sep}c({u},{m},{})", u8::from(neg));
        }
        state.push(']');
        let constant = i32::from(!value);
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
                bindings.get("Out")
            }
            _ => None,
        }
        .ok_or_else(|| format!("Prolog branch query failed: {answers:?}"))?;
        let outcome = decode(out, self.node_count);
        self.prolog += t2 - t1;
        self.to_prolog += t1 - t0;
        self.from_prolog += t2.elapsed();
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_prolog_answers_are_errors() {
        let stats_term = Term::Compound("stats".into(), vec![]);
        for t in [
            Term::Atom("oops".into()),
            Term::Compound("error".into(), vec![Term::Atom("e".into())]),
            Term::Compound("limit".into(), vec![stats_term]),
            Term::Compound("r".into(), vec![Term::Atom("x".into())]),
        ] {
            assert!(decode(&t, 3).is_err(), "{t:?}");
        }
    }
}
