//! Differential tests: Rust + Scryer against pure Rust (PRD §35). Same
//! verdicts, final relations and counters.

use stalmarck::scryer::ScryerSolver;
use stalmarck::{Config, Formula, Solver, Triples, generate};

fn config(k: u32) -> Config {
    Config { max_depth: k, ..Config::default() }
}

fn agree(f: &Formula, cfg: Config) {
    let a = Solver::new(cfg.clone()).check_sat(f);
    let b = ScryerSolver::new(cfg.clone()).check_sat(f);
    assert_eq!(a.result, b.result, "{f} {cfg:?}");
    assert_eq!(a.relation, b.relation, "{f} {cfg:?}");
    assert_eq!(a.stats, b.stats, "{f} {cfg:?}");
}

#[test]
fn literature_examples() {
    for src in [
        "!((a & (b | c)) -> ((a & b) | (a & c)))",
        "!(((a & b) | (a & c)) -> (a & (b | c)))",
        "!((a & (b | c)) <-> ((a & b) | (a & c)))",
        "!((p -> q) <-> (!q -> !p))",
    ] {
        let f: Formula = src.parse().unwrap();
        for k in 0..=2 {
            agree(&f, config(k));
        }
    }
}

#[test]
fn random_formulas() {
    for seed in 0..60 {
        let f = generate::random_formula(seed, 2 + seed as u32 % 4, 5);
        for k in 1..=2 {
            agree(&f, config(k));
        }
    }
}

#[test]
fn random_cnf_and_pigeonhole() {
    for seed in 0..10 {
        let cnf = generate::random_kcnf(seed, 8, 34, 3);
        let t = Triples::from_cnf(&cnf, true);
        let a = Solver::new(config(2)).check_sat_triples(&t);
        let b = ScryerSolver::new(config(2)).check_sat_triples(&t);
        assert_eq!(
            (a.result, a.relation, a.stats),
            (b.result, b.relation, b.stats)
        );
    }
    let t = Triples::from_cnf(&generate::pigeonhole(3), true);
    let a = Solver::new(config(2)).check_sat_triples(&t);
    let b = ScryerSolver::new(config(2)).check_sat_triples(&t);
    assert_eq!(
        (a.result, a.relation, a.stats),
        (b.result, b.relation, b.stats)
    );
}

#[test]
fn branch_limits_agree() {
    let f = generate::parity_mismatch(6);
    for max in [0, 1, 3, 10, 40] {
        agree(
            &f,
            Config {
                max_depth: 2,
                max_branches: Some(max),
                ..Config::default()
            },
        );
    }
}

#[test]
fn shared_vectors_agree() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("shared/test-vectors");
    let manifest = std::fs::read_to_string(dir.join("manifest.tsv")).unwrap();
    let mut checked = 0;
    for line in manifest.lines().filter(|l| !l.starts_with('#')) {
        let file = line.split('\t').nth(5).unwrap();
        let tri = dir.join(file).with_extension("tri");
        let t = Triples::parse_text(&std::fs::read_to_string(tri).unwrap())
            .unwrap();
        if t.node_count() > 200 {
            continue;
        }
        let a = Solver::new(config(1)).check_sat_triples(&t);
        let b = ScryerSolver::new(config(1)).check_sat_triples(&t);
        assert_eq!(
            (a.result, a.relation, a.stats),
            (b.result, b.relation, b.stats),
            "{file}"
        );
        checked += 1;
    }
    eprintln!("{checked} vectors agree");
}
