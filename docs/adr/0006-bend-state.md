# ADR-0006: Bend state representation

Status: Accepted for **Bend 2** (2.0.39). **Revised in M4 (2026-10-29): the
implementation uses option 2, persistent `Data` trees, not option 1.**

Why the M4 revision: Bend 2's `Array` is itself a balanced tree with
O(log N) access, so option 1 has no asymptotic advantage. Its affine
ownership forces every read to thread the array and every branch to
`Array.clone` the whole state in O(N). A complete binary tree of U32 declared
as a `Data` type (`Tab`) has the same O(log N) access. It is reusable (`+`,
reference-counted), so both Dilemma branches start from the parent at no cost
and share structure (PRD §10 Strategy C). This also makes the code much
simpler. The parallel and GPU variants keep this representation.

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

## M4 notes (implementation, 2026-10-29)

* `bend/stalmarck.bend`, which is git-ignored, reads the shared `.tri` files
  and runs the Track A order. It has three modes: `seq`; `par`, where both
  branches of every dilemma run as one parallel call; and `gpu`, where
  saturation and every parallel call inside it run on the M2's Metal GPU.
* **Differential result:** verdicts, canonical final relations **and all
  eight counters** equal the Rust reference on all 184 shared vectors at
  k = 0, 1 and 2, in both `seq` and `par` mode (552 / 552 runs each). In
  parallel mode the counters are summed across branches, minus the parent's
  counts, and the totals match exactly. This confirms SPEC-K2 (order
  independence) in practice.
* **Indicative timings (one M2, median of 3, wall clock including process
  start; not a benchmark):**

  | Vector | k | Rust | Bend seq | Bend par 4 threads | Bend par 8 threads |
  |--------|---|------|----------|--------------------|--------------------|
  | `pigeonhole-4` | 3 | 22 ms | 934 ms | 683 ms | 702 ms |
  | `pigeonhole-5` | 2 | 54 ms | 2 867 ms | 2 052 ms | 2 206 ms |
  | `parity-mismatch-32` | 2 | 33 ms | 1 843 ms | 1 228 ms | 2 056 ms |

  Sequential Bend is about 30–60× slower than Rust on these vectors, and
  parallel branches give about 1.4–1.5× on 4 threads. 8 threads are often
  no better or worse; E-cores and branch imbalance are the likely causes.
* **Metal GPU:** agrees with Rust on all 254 runs checked (127 vectors with
  at most 40 nodes, at k = 0 and 1). Correct, but 20–100× *slower* than one CPU thread on these
  irregular workloads (e.g. `random-001`, k = 2: 0.14 s vs 12.8 s). This
  matches upstream's guidance that divergent work stays faster on the CPU.
