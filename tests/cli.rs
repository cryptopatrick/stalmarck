//! The `stalmarck` command-line tool, checked against the shared vectors.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stalmarck"))
        .args(args)
        .output()
        .expect("run stalmarck")
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).expect("JSON record")
}

#[test]
fn vectors_through_the_cli() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shared/test-vectors");
    let manifest = fs::read_to_string(dir.join("manifest.tsv")).unwrap();
    for line in manifest.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        let (id, file, sha, expected, hardness) =
            (f[0], f[5], f[6], f[7], f[9]);
        let input = dir.join(file);
        let tri = input.with_extension("tri");
        let (input, tri) = (input.to_str().unwrap(), tri.to_str().unwrap());
        let common = ["--json", "--relation", "--max-branches", "20000"];

        let out = cli(&[&common[..], &[input]].concat());
        let rec = json(&out);
        assert_eq!(rec["input"]["sha256"], sha, "{id}: sha256");
        assert_eq!(rec["input"]["id"], id);
        let verdict = rec["result"]["verdict"].as_str().unwrap();
        let code = out.status.code().unwrap();
        match verdict {
            "UNSAT" => assert_eq!((expected, code), ("UNSAT", 20), "{id}"),
            "SAT" => {
                assert_eq!((expected, code), ("SAT", 10), "{id}");
                assert!(rec["result"]["model"].is_object(), "{id}");
            }
            "UNKNOWN" => {
                assert_eq!(code, 0, "{id}");
                assert!(rec["result"]["unknown_reason"].is_string(), "{id}");
            }
            v => panic!("{id}: verdict {v}"),
        }
        if let Ok(k) = hardness.parse::<u32>()
            && k <= 2
        {
            assert_eq!(verdict, "UNSAT", "{id}: hardness {k}");
        }

        // The .tri file gives the same run.
        let out_tri = cli(&[&common[..], &[tri]].concat());
        let rec_tri = json(&out_tri);
        assert_eq!(rec_tri["result"], rec["result"], "{id}: .tri result");
        assert_eq!(
            rec_tri["counters"], rec["counters"],
            "{id}: .tri counters"
        );

        // --emit-triples reproduces the committed .tri file.
        let emitted = cli(&["--emit-triples", input]);
        assert_eq!(
            String::from_utf8(emitted.stdout).unwrap(),
            fs::read_to_string(tri).unwrap(),
            "{id}: emitted triples"
        );
    }
}

#[test]
fn json_record_shape() {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-test.prop");
    fs::write(&tmp, "p & !q").unwrap();
    let rec = json(&cli(&["--json", tmp.to_str().unwrap()]));
    assert_eq!(rec["schema_version"], 1);
    assert_eq!(rec["implementation"]["name"], "rust");
    assert_eq!(rec["config"]["track"], "A");
    assert_eq!(rec["result"]["verdict"], "SAT");
    assert_eq!(rec["result"]["model"]["p"], true);
    assert_eq!(rec["result"]["model"]["q"], false);
    for key in [
        "parse_ms",
        "decompose_ms",
        "solver_init_ms",
        "solve_ms",
        "merge_ms",
        "total_ms",
    ] {
        assert!(rec["timing"][key].is_number(), "{key}");
    }
    assert!(rec["memory"]["peak_rss_bytes"].is_null());
    assert!(rec["error"].is_null());

    // Assuming p | q false fixes both variables: a counter-model.
    fs::write(&tmp, "p | q").unwrap();
    let out = cli(&["--json", "--mode", "valid", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(10));
    assert_eq!(json(&out)["result"]["verdict"], "INVALID");

    // Errors: exit 1, and an ERROR record with --json.
    fs::write(&tmp, "p & & q").unwrap();
    let out = cli(&["--json", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let rec = json(&out);
    assert_eq!(rec["result"]["verdict"], "ERROR");
    assert_eq!(rec["error"]["kind"], "parse");
    assert_eq!(cli(&["--bogus"]).status.code(), Some(1));
    assert_eq!(cli(&["no-such-file.prop"]).status.code(), Some(1));
}

#[test]
fn branch_strategies_agree_in_the_cli() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("shared/test-vectors/unsat/pigeonhole-4.cnf");
    let p = dir.to_str().unwrap();
    let a = json(&cli(&["--json", "--relation", "-k", "3", p]));
    let b = json(&cli(&[
        "--json",
        "--relation",
        "-k",
        "3",
        "--branch-state",
        "clone",
        p,
    ]));
    assert_eq!(a["result"], b["result"]);
    assert_eq!(a["counters"], b["counters"]);
    assert_eq!(a["config"]["variant"], Value::Null);
    assert_eq!(b["config"]["variant"], "branch-state=clone");
}
