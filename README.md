# stalmarck

**Stålmarck's proof procedure for propositional logic, in Rust.**

[![Crates.io](https://img.shields.io/crates/v/stalmarck.svg)](https://crates.io/crates/stalmarck)
[![Documentation](https://docs.rs/stalmarck/badge.svg)](https://docs.rs/stalmarck)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](https://github.com/cryptopatrick/stalmarck)

## Overview

Stålmarck's method decides whether a propositional formula is satisfiable,
or valid, by reasoning about *equivalences between subformulas* instead of
searching assignments. It works in three steps.

1. **Triples.** The formula is decomposed into triples `x ↔ (a ∧ b)` and
   `x ↔ (a ↔ b)` over literals. Identical subformulas are shared.
2. **0-saturation.** A *formula relation*, which records equivalences between
   literals, is extended with every equivalence that one triple entails. This
   repeats until nothing changes or a contradiction appears.
3. **The Dilemma rule.** The procedure assumes a node is ⊥, saturates,
   assumes it is ⊤, saturates, and keeps *only the equivalences both branches
   agree on*. Nesting this to depth `k` is k-saturation.

Formulas that occur in practice often have low *hardness*: they are refuted
at depth 0, 1 or 2.

This crate follows the procedure of M. Sheeran and G. Stålmarck,
*A Tutorial on Stålmarck's Proof Procedure for Propositional Logic*
(FMCAD '98, LNCS 1522). It implements a written formal specification exactly.

## Key Features

- Satisfiability (`check_sat`) and validity (`prove`), each with a verified
  model or counter-model.
- Results are `Sat` / `Unsat` / `Unknown` (or `Valid` / `Invalid` /
  `Unknown`). The answer is never a guess. `Unknown` says why: depth
  exhausted, branch limit, or timeout.
- Configurable Dilemma depth, structural sharing, branch limit and timeout.
- Solver counters, and a canonical form of the final relation for
  differential testing.
- Input as Rust values, as text (`"a & (b | c) -> d".parse()`), or as
  DIMACS CNF. A documented triple-file format is shared with other
  implementations.
- Seeded instance generators and a truth-table oracle for testing.
- Pure safe Rust with no dependencies.

## Quick Start

```toml
[dependencies]
stalmarck = "0.4"
```

```rust
use stalmarck::{Config, Formula, ProofResult, SatResult, Solver};

let (a, b, c) = (Formula::var("a"), Formula::var("b"), Formula::var("c"));

// (a ∧ b) ∨ (a ∧ c) → a ∧ (b ∨ c) is valid, with hardness 1.
let f = ((a.clone() & b.clone()) | (a.clone() & c.clone()))
    .implies(a.clone() & (b.clone() | c));
let solver = Solver::new(Config::default().with_max_depth(1));
assert_eq!(solver.prove(&f).result, ProofResult::Valid);

// a ∧ ¬b is satisfiable.
match solver.check_sat(&(a & !b)).result {
    SatResult::Sat(model) => assert_eq!(model.get("b"), Some(false)),
    other => panic!("{other:?}"),
}
```

Run `cargo run --example basic` for a small demo that prints solver
counters.

### Command line

```text
$ cargo install stalmarck
$ echo '!((a & b) | (a & c) -> a & (b | c))' | stalmarck --format prop -
s UNSAT
c depth=2 branches=1 merges=12 solve=0.007ms
$ stalmarck --json --relation -k 3 problem.cnf     # result-v1 JSON record
```

The exit status is 10 for SAT/INVALID, 20 for UNSAT/VALID and 0 for
unknown. See `stalmarck --help` for all options.

## Rust + Scryer Prolog

With the `scryer` feature, `stalmarck::scryer::ScryerSolver` runs every
Dilemma branch in embedded [Scryer Prolog](https://github.com/mthom/scryer-prolog).
It gives the same answers, final relations and counters as `Solver`. It
exists to compare implementation paradigms, not for speed.

```toml
stalmarck = { version = "0.4", features = ["scryer"] }

# scryer-prolog 0.10.0 trips a Rust safety check in debug builds:
[profile.dev.package.scryer-prolog]
debug-assertions = false
```

## Status

`0.4` contains the sequential pure-Rust reference implementation and the Rust + Scryer variant. Planned:

- optional parallel branch exploration;

The API may change before 1.0.

The procedure is complete only when the depth is at least the number of
variables. At small depths it is a fast semi-decision procedure for easy
formulas, by design.

## Contributing

Contributions are welcome!
Please see our [contributing guidelines](CONTRIBUTING.md) for details on:
- Code style and testing requirements
- Submitting bug reports and feature requests
- Development setup and workflow

## License

This project is licensed under the GNU Affero General Public License v3.0
only (AGPL-3.0-only). See [LICENSE](LICENSE) for details.
