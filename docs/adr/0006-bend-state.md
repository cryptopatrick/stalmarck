# ADR-0006: Bend state representation

Status: Accepted for **Bend 2** (2.0.39) on 2026-10-28. The spike passed
(feasibility report §4.2). Note that Bend 2's `Array` is a balanced tree with
O(log N) access, so option 1 below has a persistent-tree cost profile, not a
flat-array one.

## Context
Bend has no conventional shared mutable structures. The main Bend research
question is parallel dilemma exploration (PRD §30). Because `sat_k` is
order-independent (SPEC-K2), parallel evaluation must produce the same
canonical relation as sequential evaluation.

## Options
1. **Affine array union-find (Bend 2).** `Array<U32>` stores
   `parent << 1 | parity`, plus a size array, with in-place writes. Each
   dilemma's two branches run as a balanced parallel call on
   `Array.clone`d states (Strategy B). Saturation inside a branch is
   sequential with a worklist.
2. **Persistent tree map (Bend 1 or Bend 2).** `Map` keyed by node, storing a
   union-find with parity (Strategy C). This costs O(log N) per access but
   needs no cloning.
3. **Label propagation.** Iterate class labels to a fixed point in parallel
   over all triples. This is a Jacobi-style 0-saturation that converges to
   `sat₀` by SPEC-Z1, but the number of rounds can approach O(N).

## Proposed decision
* **If Bend 2:** use option 1 as the baseline. Parallelism comes only from the
  two dilemma branches, which are naturally balanced fork-joins. Within a pass,
  the Track B variant evaluates all candidate nodes' dilemmas in parallel
  against the same `R`, joins the results and re-closes with `sat₀`. By SPEC-K2
  this gives the same `sat_k`. GPU: the same program with `!` on the
  branch-level call, reported separately (PRD §31).
* **If Bend 1:** use option 2. Node ids must stay below 2²⁴. RSS is reported
  together with an empty-program baseline.
* Option 3 is a Track B experiment for parallel 0-saturation, with convergence
  checked against the canonical `sat₀`.

Excessive branching (PRD §30) is controlled by the depth limit `k` and by a
branch-granularity threshold. Below `min_parallel_nodes` undetermined nodes,
branches run sequentially.

## Consequences
* All Bend sources live in `bend/`, which is git-ignored (amendment A2).
* The Bend implementation reads the shared triple file, so it needs no parser.
