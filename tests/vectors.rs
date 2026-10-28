//! Validates the shared test vectors in `shared/test-vectors` (PRD §51).
//! The directory is not published, so this test does nothing outside the
//! repository.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use stalmarck::{Cnf, Config, Formula, SatResult, Solver, Triples, oracle};

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[test]
fn shared_vectors() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shared/test-vectors");
    let Ok(manifest) = fs::read_to_string(dir.join("manifest.tsv")) else {
        eprintln!("no shared/test-vectors: skipping");
        return;
    };
    let mut checked = 0;
    for line in manifest.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 10, "bad manifest line: {line}");
        let (id, file, sha, expected, source, hardness) =
            (f[0], f[5], f[6], f[7], f[8], f[9]);
        let text = fs::read_to_string(dir.join(file)).unwrap();
        assert_eq!(sha256_hex(text.as_bytes()), sha, "{id}: hash");
        let sat = match expected {
            "SAT" => true,
            "UNSAT" => false,
            e => panic!("{id}: bad expected value {e}"),
        };

        // The triple file matches a fresh decomposition.
        let (triples, formula) = if file.ends_with(".cnf") {
            let cnf = Cnf::parse_dimacs(&text).unwrap();
            if source == "oracle" {
                let o = oracle::satisfying_assignment_cnf(&cnf).unwrap();
                assert_eq!(o.is_some(), sat, "{id}: oracle");
            }
            (Triples::from_cnf(&cnf, true), cnf.to_formula())
        } else {
            let f: Formula = text.parse().unwrap();
            if source == "oracle" {
                let o = oracle::satisfying_assignment(&f).unwrap();
                assert_eq!(o.is_some(), sat, "{id}: oracle");
            }
            (Triples::from_formula(&f, true), f)
        };
        let tri = dir.join(file).with_extension("tri");
        let tri = fs::read_to_string(tri).unwrap();
        assert_eq!(Triples::parse_text(&tri).unwrap().to_text(), tri);
        assert_eq!(triples.to_text(), tri, "{id}: triples");

        // The reference solver agrees: refutes at exactly the recorded
        // depth, and never refutes a satisfiable vector.
        let solve = |k| {
            Solver::new(Config { max_depth: k, ..Config::default() })
                .check_sat_triples(&triples)
                .result
        };
        if let Ok(k) = hardness.parse::<u32>() {
            assert_eq!(solve(k), SatResult::Unsat, "{id}: k={k}");
            if k > 0 {
                assert_ne!(solve(k - 1), SatResult::Unsat, "{id}: k={k}");
            }
        }
        if sat {
            match solve(1) {
                SatResult::Unsat => panic!("{id}: refuted a SAT vector"),
                SatResult::Sat(m) => assert!(
                    formula.evaluate(&|x| m.get(x).unwrap()),
                    "{id}: bad model"
                ),
                SatResult::Unknown(_) => {}
            }
        }
        checked += 1;
    }
    assert!(checked > 0);
    eprintln!("{checked} vectors checked");
}
