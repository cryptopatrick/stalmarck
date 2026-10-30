# ADR-0007: Branch-merging algorithm

Status: Accepted (M0)

## Context
The merge is `R₁ ⊓ R₂` (SPEC-R5). A contradictory branch acts as the identity
element, and otherwise the merge is the set intersection. It is the most
error-prone step (PRD §11).

## Options
1. Enumerate all related pairs in both relations and intersect the pair
   sets. This is O(Σ class²) and is used only as a test oracle.
2. **Key grouping (SPEC-R6).** Use the key `κ(v) = (r₁(v), r₂(v), p₁⊕p₂)`.
   This is O(N) with hashing.
3. **Delta grouping.** Apply key grouping only to the nodes whose class
   changed in both branches since the checkpoint. A node unchanged in either
   branch keeps its parent class.

## Decision
Use option 3 in implementations that have a trail (Rust A, and Bend since
ADR-0009). Use option 2 in the others. Option 1 is the property-test oracle (P6).

To install the merge into the parent: for each group that contains two or
more parent classes, union them in ascending `(min node)` order with the
parity derived from κ. By SPEC-M3, **no re-propagation is needed**. Debug
builds assert this by running `sat₀` on the merged state and checking that
nothing changes.

Polarity rules (SPEC-M2): a pair `u ~ w` survives only if it has the *same
parity* in both branches. If `u ~ w` holds in one branch and `u ~ ¬w` in the
other, the relationship between u and w is dropped entirely.

## Consequences
* The merge costs O(changed nodes) in Rust, which is far less than O(N).
* The same algorithm works in Prolog (assoc grouping) and in Bend (sort and
  group on U32 keys).
