# ADR-0004: Rust branch-state strategy

Status: Accepted (M0). Implemented in M2 as `BranchState::Rollback`, the
default. `BranchState::Clone` keeps Strategy B for comparison. Both give
identical results, final relations and counters, which a test checks.

## Context
Each dilemma (SPEC-M1) saturates two branches from the same parent `R`, then
installs their meet. PRD §10 asks for rollback, cloning and persistent state
to be evaluated.

| Strategy | Time per branch | Memory | Parallel-friendly | Union-find fit | Risk |
|----------|-----------------|--------|-------------------|----------------|------|
| A. Rollback trail | O(changes) undo | O(changes) trail | No: one mutable state | Requires no path compression, or trailing compressions | Undo bugs. P7 tests them. |
| B. Clone | O(N) copy | O(N) per live branch, O(N·k) at depth k | Yes: independent states | Any | Low |
| C. Persistent (HAMT/AVL) | O(log N) per update | Shared structure | Yes | Persistent union-find costs O(log N) per access | Constant factors |

## Decision
**Baseline (Track A): Strategy A, a rollback trail.** Each mutating operation
(union, worklist flag) pushes an undo record. A checkpoint is the trail length
at that moment. The dilemma runs as follows:

```
cp := checkpoint
assume v≡⊥; sat_{k−1}; R₁ := canonical-or-compact snapshot; rollback(cp)
assume v≡⊤; sat_{k−1}; R₂ := snapshot;                      rollback(cp)
install (R₁ ⊓ R₂) − R as unions                            # SPEC-M3: no re-propagation needed
```

The snapshot stores only the classes of nodes that changed since `cp`, found
from the trail. The meet computation (ADR-0007) is then O(changed nodes), not
O(N).

**Track B experiments:** Strategy B with `rayon::join` to run both branches in
parallel, each on its own clone. Path compression with trailed compressions.

## Consequences
* There is no shared mutable state between branches, so the baseline stays
  sequential (PRD §18).
* Rollback correctness is covered by property P7: the canonical form after
  rollback equals the canonical form before the branch.

## M2 notes

The merge snapshot (ADR-0007, option 3) holds `(node, root, parity)` for the
nodes changed in the first branch. It is merge-joined with the nodes changed
in the second branch, then grouped by sorting. Indicative release-build
timings on one M2 (median of 5, not a benchmark):

| Vector | k | Rollback | Clone |
|--------|---|----------|-------|
| `pigeonhole-4` | 3 | 16.5 ms | 20.6 ms |
| `pigeonhole-5` | 2 | 49.1 ms | 58.5 ms |
| `parity-mismatch-32` | 2 | 25.7 ms | 90.9 ms |

Merge cost dominates when branches change few nodes, which is where
rollback's delta merge wins most.
