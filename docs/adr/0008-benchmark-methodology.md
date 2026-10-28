# ADR-0008: Benchmark methodology

Status: Proposed. Detailed in `_bench/` during Phase 6 (amendment A4).

## Decision (framework)
* **Two tracks, never mixed (PRD §39):**
  * Track A: shared triple file, SPEC-Z4/K3 canonical order, sequential, the
    same `k` and the same limits.
  * Track B: implementation-specific optimizations, each one named.
* **Two timing boundaries (PRD §42):**
  * `solver_only`: from the triple file being loaded into memory to the
    result. Measured inside each process with a monotonic clock, or externally
    for Bend if Bend 2 has no clock.
  * `end_to_end`: process wall time, including runtime start-up (the Scryer
    machine build takes ≈ 58 ms; HVM heap allocation).
* **Memory:** OS peak RSS, measured by the runner from the child's resource
  usage (`wait4`/`getrusage`). The solvers themselves print
  `peak_rss_bytes: null`, which keeps them free of platform-specific code.
  Each implementation also reports an empty-input baseline run, which is
  never silently subtracted. GPU device memory is
  reported as `unavailable` unless measured (amendment A5).
* **Statistics:** at least 10 measured runs after 2 warm-up runs, with
  implementation order randomized and interleaved per instance. Report median,
  mean, sd, min, max, and a bootstrap 95 % CI on the median.
* **Correctness gate:** a result is discarded if its logical answer disagrees
  with the oracle. Where `sat_k` completes, its canonical form must equal the
  Rust reference.
* **Storage:** JSON Lines in `_bench/results/raw/`, append-only and
  versioned by git commit and hardware id (PRD §47).

## Consequences
The runner (PRD §46) is a Rust binary or a Python script inside `_bench/`. It
is never part of the published crate.
