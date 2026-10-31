## Unreleased

## v0.4.0 - 2026-10-31
 - *Breaking*: `Config`, `Stats`, `Timing`, `Report`, `ParseError`, `Unknown`
   and `BranchState` are `#[non_exhaustive]`, so later additions need no
   breaking release. To migrate, replace
   `Config { max_depth: 1, ..Config::default() }` with
   `Config::default().with_max_depth(1)`, and add a `_` arm to matches on
   `Unknown` and `BranchState`.
 - `Config` gains `with_max_depth`, `with_sharing`, `with_max_branches`,
   `with_timeout` and `with_branch_state`.
 - *Breaking*: `Unknown` is no longer `Copy`. It gains
   `BackendError(String)`: a Scryer failure now gives an unknown result
   instead of a panic. `unknown_reason` in `result-v1` gains `backend_error`.
 - `Unknown` implements `Display`. `SatResult` gains `is_sat`, `is_unsat` and
   `model`; `ProofResult` gains `is_valid`, `is_invalid` and `counter_model`;
   `Model` gains `len` and `is_empty`.
 - *Breaking*: functions that return a value are `#[must_use]`, so ignoring
   one is now a warning.
 - All errors and panics are documented, and tests guarantee that the main
   public types are `Send + Sync`.
 - `scryer`: much faster. Local closure is cached by pattern (median 1.8×),
   and a branch exports only its merged classes (up to 15×). Results and
   counters are unchanged (ADR-0009).
 - The published package now includes the shared test vectors and passes
   its own tests. Previously `json_record_shape` failed there, and the
   vector tests skipped silently.

## v0.3.0 - 2026-10-28
 - Implementation B, behind the new `scryer` feature:
   `scryer::ScryerSolver` runs Dilemma branches in embedded Scryer Prolog
   0.10. Its results, final relations and counters equal `Solver`.
 - The CLI gains `--backend scryer` when built with the feature. JSON
   records then report `rust-scryer` and the Rust–Prolog boundary timings.
 - *Breaking*: `Timing` gains `prolog`, `to_prolog` and `from_prolog`.
 - *Note*: in debug builds, `scryer-prolog` 0.10.0 needs
   `[profile.dev.package.scryer-prolog] debug-assertions = false` (see the
   README).

## v0.2.0 - 2026-10-28
 - `.prop` formula syntax: `Formula: FromStr`, with the precedence
   `! & ^ | -> <->` and keyword spellings.
 - DIMACS CNF input (`Cnf`), and `Triples::from_cnf`.
 - The `.tri` triple file format: `Triples::to_text` and
   `Triples::parse_text`, which checks validity.
 - Canonical relations print as `contradiction` or `classes …`.
 - `generate`: seeded instance generators (random formulas, random k-CNF,
   pigeonhole, parity, implication chains).
 - `oracle`: truth-table satisfiability oracles.
 - The `stalmarck` command-line tool reads `.prop`, `.cnf` or `.tri` input
   and prints a human-readable line or a `result-v1` JSON record
   (`--json`). Exit status: 10 SAT/INVALID, 20 UNSAT/VALID, 0 unknown.
 - Dilemma branches now use a rollback trail by default (ADR-0004), with a
   delta merge. `Config::branch_state` selects `BranchState::Clone`
   instead; both give identical results.
 - `Report::timing` gives the init, solve and merge times. *Breaking*:
   `Report` and `Config` have new public fields.
 - A resource limit hit inside a Dilemma branch now always leaves the
   reported relation sound.
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
