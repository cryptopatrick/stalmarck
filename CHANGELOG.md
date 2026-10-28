## Unreleased

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
