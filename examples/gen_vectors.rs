//! Regenerates the shared test vectors (not published with the crate).
//!
//! ```text
//! cargo run --release --example gen_vectors -- shared/test-vectors
//! ```
//!
//! Output is deterministic: rerunning it must leave the directory unchanged.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use stalmarck::generate::{self, GENERATOR_VERSION};
use stalmarck::{Cnf, Config, Formula, SatResult, Solver, Triples, oracle};

enum Input {
    Prop(Formula),
    Cnf(Cnf),
}

struct Vector {
    id: String,
    family: &'static str,
    seed: String,
    params: String,
    input: Input,
    /// Known by construction, when the oracle cannot run.
    theory: Option<bool>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Smallest depth <= 3 at which the reference solver refutes the input
/// within a fixed budget.
fn ref_hardness(t: &Triples) -> Option<u32> {
    (0..=3).find(|&k| {
        let s = Solver::new(
            Config::default()
                .with_max_depth(k)
                .with_max_branches(200_000)
                .with_timeout(Duration::from_secs(20)),
        );
        s.check_sat_triples(t).result == SatResult::Unsat
    })
}

fn vectors() -> Vec<Vector> {
    let mut out = Vec::new();
    let mut push =
        |id: String, family, seed: String, params, input, theory| {
            out.push(Vector { id, family, seed, params, input, theory })
        };
    for seed in 1..=80u64 {
        let vars = 2 + (seed % 5) as u32;
        let depth = 3 + (seed % 4) as u32;
        let f = generate::random_formula(seed, vars, depth);
        let params = format!("random_formula vars={vars} depth={depth}");
        push(
            format!("small-{seed:03}"),
            "small",
            seed.to_string(),
            params,
            Input::Prop(f),
            None,
        );
    }
    // Random formulas are rarely UNSAT, so add UNSAT ones by deterministic
    // rejection sampling: the first 40 seeds from 10 000 that the oracle
    // refutes.
    let mut seed = 10_000u64;
    let mut found = 0;
    while found < 40 {
        let vars = 3 + (seed % 4) as u32;
        let depth = 6 + (seed % 3) as u32;
        let f = generate::random_formula(seed, vars, depth);
        if oracle::satisfying_assignment(&f).unwrap().is_none() {
            found += 1;
            let params = format!(
                "random_formula vars={vars} depth={depth} (first UNSAT seeds from 10000)"
            );
            push(
                format!("small-unsat-{found:03}"),
                "small",
                seed.to_string(),
                params,
                Input::Prop(f),
                None,
            );
        }
        seed += 1;
    }
    for seed in 1..=40u64 {
        let vars = 8 + (seed % 7) as u32;
        let clauses = (vars as f64 * 4.26).round() as u32;
        let cnf = generate::random_kcnf(seed, vars, clauses, 3);
        let params = format!("random_kcnf vars={vars} clauses={clauses} k=3");
        push(
            format!("random-{seed:03}"),
            "random",
            seed.to_string(),
            params,
            Input::Cnf(cnf),
            None,
        );
    }
    for n in [4, 8, 12] {
        for p in [false, true] {
            push(
                format!("parity-{n:02}-{}", p as u8),
                "structured",
                "-".into(),
                format!("parity n={n} parity={p}"),
                Input::Prop(generate::parity(n, p)),
                None,
            );
        }
    }
    for n in [2, 4, 8, 16, 32] {
        let theory = (n > oracle::MAX_VARIABLES as u32).then_some(false);
        push(
            format!("parity-mismatch-{n:02}"),
            "structured",
            "-".into(),
            format!("parity_mismatch n={n}"),
            Input::Prop(generate::parity_mismatch(n)),
            theory,
        );
    }
    for n in [10, 100, 1000] {
        let theory = (n > oracle::MAX_VARIABLES as u32).then_some(false);
        push(
            format!("implication-chain-{n:04}"),
            "saturation",
            "-".into(),
            format!("implication_chain n={n}"),
            Input::Prop(generate::implication_chain(n)),
            theory,
        );
    }
    for holes in 1..=5 {
        let theory = ((holes + 1) * holes > oracle::MAX_VARIABLES as u32)
            .then_some(false);
        push(
            format!("pigeonhole-{holes}"),
            "unsat",
            "-".into(),
            format!("pigeonhole holes={holes}"),
            Input::Cnf(generate::pigeonhole(holes)),
            theory,
        );
    }
    let literature = [
        ("st98-distrib", "!((a & (b | c)) -> ((a & b) | (a & c)))"),
        ("st98-reverse", "!(((a & b) | (a & c)) -> (a & (b | c)))"),
        ("tr12-ex1", "!((a & b) | (!a | !b))"),
        ("tr12-ex2", "!((a & (b | c)) <-> ((a & b) | (a & c)))"),
        ("tr12-ex3", "!((p -> q) <-> (!q -> !p))"),
    ];
    for (id, src) in literature {
        push(
            id.to_string(),
            "dilemma",
            "-".into(),
            "negated tautology from the literature".into(),
            Input::Prop(src.parse().unwrap()),
            None,
        );
    }
    out
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: gen_vectors <dir>");
    let dir = Path::new(&dir);
    let mut manifest = String::from(
        "# stalmarck test vectors; see README.md for the columns\n\
         # id\tfamily\tgenerator_version\tseed\tparams\tfile\tsha256\t\
         expected\texpected_source\tref_hardness\n",
    );
    for v in vectors() {
        let sub = dir.join(v.family);
        fs::create_dir_all(&sub).unwrap();
        let (text, ext, triples, expected) = match &v.input {
            Input::Prop(f) => {
                let oracle = v.theory.map(Ok).unwrap_or_else(|| {
                    oracle::satisfying_assignment(f).map(|m| m.is_some())
                });
                (
                    format!("{f}\n"),
                    "prop",
                    Triples::from_formula(f, true),
                    oracle.expect("oracle refused; give a theory answer"),
                )
            }
            Input::Cnf(c) => {
                let oracle = v.theory.map(Ok).unwrap_or_else(|| {
                    oracle::satisfying_assignment_cnf(c).map(|m| m.is_some())
                });
                (
                    c.to_dimacs(),
                    "cnf",
                    Triples::from_cnf(c, true),
                    oracle.expect("oracle refused; give a theory answer"),
                )
            }
        };
        let file = format!("{}/{}.{ext}", v.family, v.id);
        fs::write(dir.join(&file), &text).unwrap();
        fs::write(sub.join(format!("{}.tri", v.id)), triples.to_text())
            .unwrap();
        let hardness = if expected {
            "-".to_string()
        } else {
            ref_hardness(&triples).map_or("-".into(), |k| k.to_string())
        };
        let source = if v.theory.is_some() { "theory" } else { "oracle" };
        let _ = writeln!(
            manifest,
            "{}\t{}\t{GENERATOR_VERSION}\t{}\t{}\t{file}\t{}\t{}\t{source}\t{hardness}",
            v.id,
            v.family,
            v.seed,
            v.params,
            sha256_hex(text.as_bytes()),
            if expected { "SAT" } else { "UNSAT" },
        );
        eprintln!(
            "{} {} k={hardness}",
            v.id,
            if expected { "SAT" } else { "UNSAT" }
        );
    }
    fs::write(dir.join("manifest.tsv"), manifest).unwrap();
}
