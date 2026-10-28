## Unreleased
 - `.prop` formula syntax: `Formula: FromStr`, with the precedence
   `! & ^ | -> <->` and keyword spellings.
 - DIMACS CNF input (`Cnf`), and `Triples::from_cnf`.
 - The `.tri` triple file format: `Triples::to_text` and
   `Triples::parse_text`, which checks validity.
 - Canonical relations print as `contradiction` or `classes …`.
 - `generate`: seeded instance generators (random formulas, random k-CNF,
   pigeonhole, parity, implication chains).
 - `oracle`: truth-table satisfiability oracles.
 - Parsing, printing, evaluation, decomposition and dropping no longer
   recurse, so deeply nested formulas (tested at 100 000 levels) cannot
   overflow the stack. *Note*: `Formula` now implements `Drop`, so its
   fields can no longer be moved out by pattern matching. Match on a
   reference instead. *Breaking*

## v0.1.0 - 2026-10-28
 - First release: a sequential pure-Rust implementation of Stålmarck's proof
   procedure. It covers the formula API, decomposition into AND/EQ triples
   with structural sharing, a union-find with parity, semantic local rules,
   0-saturation, the Dilemma rule and k-saturation.
 - `Solver::check_sat` and `Solver::prove` return verified models,
   `Unknown` results with a reason, solver counters, and a canonical final
   relation.
 - *Note*: Dilemma branches use state cloning. The rollback trail of
   ADR-0004 is planned.

Flags: *Note*, *Breaking*
