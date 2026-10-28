//! Correctness tests against the formal specification (§11, §12).

use stalmarck::{
    CanonicalRelation, Config, Formula, ProofResult, SatResult, Solver,
    Triples, Unknown,
};

fn v(name: &str) -> Formula {
    Formula::var(name)
}

fn solver(depth: u32, sharing: bool) -> Solver {
    Solver::new(Config { max_depth: depth, sharing, ..Config::default() })
}

/// Smallest depth at which `f` is proved valid.
fn hardness(f: &Formula, sharing: bool) -> Option<u32> {
    (0..=3).find(|&k| solver(k, sharing).prove(f).result == ProofResult::Valid)
}

#[test]
fn hardness_matches_literature() {
    let (a, b, c) = (v("a"), v("b"), v("c"));
    let (p, q) = (v("p"), v("q"));
    // [ST98 §3.3]
    let distrib = (a.clone() & (b.clone() | c.clone()))
        .implies((a.clone() & b.clone()) | (a.clone() & c.clone()));
    let reverse = ((a.clone() & b.clone()) | (a.clone() & c.clone()))
        .implies(a.clone() & (b.clone() | c.clone()));
    // [TR12 Ex. 1-3]
    let ex1 = (a.clone() & b.clone()) | (!a.clone() | !b.clone());
    let ex2 =
        (a.clone() & (b.clone() | c.clone())).iff((a.clone() & b) | (a & c));
    let ex3 = p.clone().implies(q.clone()).iff((!q).implies(!p));

    for (f, shared, unshared) in [
        (&distrib, 0, 0),
        (&reverse, 1, 1),
        (&ex1, 0, 0),
        (&ex2, 1, 1),
        (&ex3, 0, 1),
    ] {
        assert_eq!(hardness(f, true), Some(shared), "{f}");
        assert_eq!(hardness(f, false), Some(unshared), "{f}");
    }
}

#[test]
fn simple_answers() {
    let (p, q) = (v("p"), v("q"));
    let s = Solver::default();
    match s.check_sat(&(p.clone() & !q.clone())).result {
        SatResult::Sat(m) => {
            assert_eq!(m.get("p"), Some(true));
            assert_eq!(m.get("q"), Some(false));
        }
        r => panic!("expected SAT, got {r:?}"),
    }
    assert_eq!(
        s.check_sat(&(p.clone() & !p.clone())).result,
        SatResult::Unsat
    );
    assert_eq!(s.prove(&(p.clone() | !p.clone())).result, ProofResult::Valid);
    assert_eq!(
        s.check_sat(&Formula::constant(false)).result,
        SatResult::Unsat
    );
    // p ∨ q is satisfiable, but saturation never fixes p or q.
    assert_eq!(
        s.check_sat(&(p.clone() | q.clone())).result,
        SatResult::Unknown(Unknown::DepthExhausted)
    );
    match s.prove(&(p | q)).result {
        ProofResult::Invalid(m) => {
            assert_eq!((m.get("p"), m.get("q")), (Some(false), Some(false)))
        }
        r => panic!("expected INVALID, got {r:?}"),
    }
}

#[test]
fn branch_limit_gives_unknown() {
    let (a, b, c) = (v("a"), v("b"), v("c"));
    let reverse = ((a.clone() & b.clone()) | (a.clone() & c.clone()))
        .implies(a & (b | c));
    let s = Solver::new(Config {
        max_depth: 1,
        max_branches: Some(0),
        ..Config::default()
    });
    assert_eq!(
        s.prove(&reverse).result,
        ProofResult::Unknown(Unknown::BranchLimit)
    );
}

// ------------------------------------------------------------------ random

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_formula(rng: &mut Rng, vars: u64, depth: u32) -> Formula {
    if depth == 0 || rng.below(5) == 0 {
        return match rng.below(20) {
            0 => Formula::constant(rng.below(2) == 0),
            _ => v(&format!("x{}", rng.below(vars))),
        };
    }
    let (l, r) = (
        random_formula(rng, vars, depth - 1),
        random_formula(rng, vars, depth - 1),
    );
    match rng.below(6) {
        0 => !l,
        1 => l & r,
        2 => l | r,
        3 => l.implies(r),
        4 => l.iff(r),
        _ => l ^ r,
    }
}

/// All models of `f`, as functions from variable name to value.
fn models(f: &Formula) -> Vec<Vec<(String, bool)>> {
    let vars: Vec<String> =
        f.variables().iter().map(|s| s.to_string()).collect();
    (0u32..1 << vars.len())
        .map(|bits| {
            vars.iter()
                .enumerate()
                .map(|(i, x)| (x.clone(), bits >> i & 1 == 1))
                .collect::<Vec<_>>()
        })
        .filter(|m| f.evaluate(&|x| m.iter().find(|(y, _)| y == x).unwrap().1))
        .collect()
}

#[test]
fn random_formulas_are_sound_and_complete() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..600 {
        let vars = 1 + rng.below(4);
        let depth = 1 + rng.below(4) as u32;
        let f = random_formula(&mut rng, vars, depth);
        let sharing = rng.below(2) == 0;
        let ms = models(&f);
        let triples = Triples::from_formula(&f, sharing);

        // P1: decomposition preserves meaning.
        for bits in 0u32..1 << triples.variables().len() {
            let val = |x: &str| {
                let i = triples.variables().iter().position(|(y, _)| y == x);
                bits >> i.unwrap() & 1 == 1
            };
            let nodes = triples.evaluate(val);
            let root = triples.root();
            assert_eq!(
                nodes[root.node() as usize] != root.is_negated(),
                f.evaluate(&val),
                "{f}"
            );
        }

        for k in 0..=2 {
            let report = solver(k, sharing).check_sat(&f);
            match &report.result {
                SatResult::Unsat => assert!(ms.is_empty(), "{f} k={k}"),
                SatResult::Sat(m) => {
                    assert!(f.evaluate(&|x| m.get(x).unwrap()), "{f} k={k}")
                }
                SatResult::Unknown(u) => {
                    assert_eq!(*u, Unknown::DepthExhausted, "{f}")
                }
            }
            // P2: every equivalence in the final relation holds in every
            // model.
            if let CanonicalRelation::Classes(classes) = &report.relation {
                for m in &ms {
                    let get =
                        |x: &str| m.iter().find(|(y, _)| y == x).unwrap().1;
                    let nodes = triples.evaluate(get);
                    for (v, &(min, neg)) in classes.iter().enumerate() {
                        assert_eq!(
                            nodes[v],
                            nodes[min as usize] != neg,
                            "unsound: {f} k={k}"
                        );
                    }
                }
            }
        }

        // P8: complete when k >= number of variables.
        let k = triples.variables().len() as u32;
        let r = solver(k, sharing).check_sat(&f).result;
        if ms.is_empty() {
            assert_eq!(r, SatResult::Unsat, "{f}");
        } else {
            assert_ne!(r, SatResult::Unsat, "{f}");
        }
    }
}

#[test]
fn sharing_toggle_preserves_definite_answers() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..300 {
        let f = random_formula(&mut rng, 4, 4);
        let shared = solver(2, true).check_sat(&f).result;
        let unshared = solver(2, false).check_sat(&f).result;
        let definite = |r: &SatResult| !matches!(r, SatResult::Unknown(_));
        if definite(&shared) && definite(&unshared) {
            assert_eq!(
                shared == SatResult::Unsat,
                unshared == SatResult::Unsat,
                "{f}"
            );
        }
    }
}
