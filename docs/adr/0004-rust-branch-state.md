# ADR-0004: Rust branch-state strategy

Status: Accepted (M0)

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
