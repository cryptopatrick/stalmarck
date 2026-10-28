# ADR-0003: Equivalence-class representation

Status: Accepted (M0)

## Context
PRD §6 asks for at least two representations to be evaluated:

1. union-find with Boolean parity;
2. explicit positive and negative literals.

Formula relations must be complement-closed (SPEC-R1).

## Options
1. **Union-find over nodes with a parity bit** (`parent[v]`, `parity[v]`,
   `size[v]`). Complement-closure is automatic, `find` returns `(root, p)`,
   and contradiction means "same root, wrong parity".
2. **Union-find over 2N literals**, where every merge of `a, b` also merges
   `¬a, ¬b`. This doubles memory and work, and the two halves can drift apart
   if a bug updates only one.
3. Explicit class lists with a `class_of[lit]` map. Merges cost
   O(size of smaller class) when relabelling, and the class lists are
   materialized.

## Decision
* **Rust (A):** option 1. Use union by size, with **no path compression** in
  the baseline (ADR-0004). Keep a per-class member list linked through
  `next[v]`, because the affected-triple scan (SPEC-Z3) needs class members.
  Path compression with a recorded trail is a Track B experiment.
* **Scryer (B):** option 1 on a persistent AVL map (`library(assoc)`). See
  ADR-0005.
* **Bend (C):** option 1 on an affine U32 array (Bend 2), or option 3 on a
  persistent map (Bend 1). See ADR-0006.

**Cross-implementation comparison** uses only the canonical form of SPEC-R4:
a vector of `(min node of class, parity)`, or `CONTRADICTION`. Every
implementation MUST be able to emit it. Internal arrays are never compared.

## Consequences
* With no path compression, `find` costs O(log N) under union by size. That
  is acceptable and keeps rollback trivial.
* Producing the canonical form costs O(N) after one pass that computes each
  class's minimum. It is produced only at checkpoints, not inside the inner
  loop.
