//! Input and triple formats (`docs/triple-format.md`).

use stalmarck::{Cnf, Formula, Triples, generate};

#[test]
fn doc_example_is_exact() {
    let f: Formula = "a & (b | c)".parse().unwrap();
    let expected = "stalmarck-triples 1\nnodes 6\nvars 3\nv 1 a\nv 2 b\n\
                    v 3 c\ntriples 2\nt and 4 5 7\nt and 5 2 9\nroot 10\n";
    assert_eq!(Triples::from_formula(&f, true).to_text(), expected);
}

#[test]
fn display_parse_round_trip() {
    for seed in 0..300 {
        let f = generate::random_formula(seed, 5, 6);
        let back: Formula = f.to_string().parse().unwrap();
        assert_eq!(back, f, "seed {seed}");
    }
}

#[test]
fn triple_text_round_trip() {
    for seed in 0..300 {
        let f = generate::random_formula(seed, 5, 6);
        for sharing in [true, false] {
            let t = Triples::from_formula(&f, sharing);
            let text = t.to_text();
            let back = Triples::parse_text(&text).unwrap();
            assert_eq!(back.to_text(), text, "seed {seed}");
        }
    }
}

#[test]
fn triple_reader_rejects_invalid_files() {
    let good = "stalmarck-triples 1\nnodes 4\nvars 2\nv 1 a\nv 2 b\n\
                triples 1\nt and 3 2 4\nroot 6\n";
    assert!(Triples::parse_text(good).is_ok());
    let bad = [
        ("stalmarck-triples 1", "stalmarck-triples 2"), // version
        ("t and 3 2 4", "t or 3 2 4"),                  // kind
        ("t and 3 2 4", "t and 3 4 2"),                 // a <= b
        ("t and 3 2 4", "t eq 3 2 5"),                  // eq polarity
        ("t and 3 2 4", "t and 3 2 6"),                 // operand order
        ("v 2 b", "v 2 a"),                             // duplicate name
        ("v 2 b", "v 3 b"),                             // node defined twice
        ("root 6", "root 8"),                           // root range
        ("nodes 4", "nodes 5"),                         // node 4 undefined
        ("root 6\n", "root 6\nroot 6\n"),               // trailing content
    ];
    for (from, to) in bad {
        let text = good.replace(from, to);
        assert!(Triples::parse_text(&text).is_err(), "accepted: {to}");
    }
}

#[test]
fn cnf_decomposition_preserves_meaning() {
    for seed in 0..100 {
        let cnf = generate::random_kcnf(seed, 6, 1 + seed as u32 % 12, 3);
        let t = Triples::from_cnf(&cnf, true);
        assert_eq!(
            Triples::parse_text(&t.to_text()).unwrap().to_text(),
            t.to_text()
        );
        // Variable i is node i.
        for (i, (name, node)) in t.variables().iter().enumerate() {
            assert_eq!(
                (name.clone(), *node),
                (format!("x{}", i + 1), i as u32 + 1)
            );
        }
        for bits in 0u32..1 << 6 {
            let a: Vec<bool> = (0..6).map(|i| bits >> i & 1 == 1).collect();
            let val = t.evaluate(|n| a[n[1..].parse::<usize>().unwrap() - 1]);
            let root = t.root();
            assert_eq!(
                val[root.node() as usize] != root.is_negated(),
                cnf.evaluate(&a)
            );
        }
    }
    // Empty CNF and empty clause.
    let t = Triples::from_cnf(&Cnf { variables: 1, clauses: vec![] }, true);
    assert_eq!(t.root(), stalmarck::Lit::TRUE);
    let t =
        Triples::from_cnf(&Cnf { variables: 1, clauses: vec![vec![]] }, true);
    assert_eq!(t.root(), stalmarck::Lit::FALSE);
}

#[test]
fn canonical_relation_line() {
    let f: Formula = "a & !b".parse().unwrap();
    let r = stalmarck::Solver::default().check_sat(&f).relation;
    // Nodes: 0 = T, 1 = a, 2 = b, 3 = a & !b; a = T, b = F, node 3 = T.
    assert_eq!(r.to_string(), "classes 0 0 1 0");
    let f: Formula = "a & !a".parse().unwrap();
    let r = stalmarck::Solver::default().check_sat(&f).relation;
    assert_eq!(r.to_string(), "contradiction");
}

#[test]
fn deep_formulas_do_not_overflow() {
    // PRD §40 C: deep implication chains. Every path must be iterative.
    let n = 100_000;
    let f = generate::implication_chain(n);
    let text = f.to_string();
    let back: Formula = text.parse().unwrap();
    assert!(!back.evaluate(&|_| true));
    assert_eq!(back.variables().len(), n as usize);
    let t = Triples::from_formula(&back, true);
    assert_eq!(
        Triples::parse_text(&t.to_text()).unwrap().node_count(),
        t.node_count()
    );
    let deep_not =
        format!("{}a{}", "!(".repeat(n as usize), ")".repeat(n as usize));
    let g: Formula = deep_not.parse().unwrap();
    assert!(g.evaluate(&|_| true));
    let r = stalmarck::Solver::default().check_sat_triples(&t);
    assert_eq!(r.result, stalmarck::SatResult::Unsat);
}
