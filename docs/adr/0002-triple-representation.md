# ADR-0002: Triple representation

Status: Accepted (M0)

## Context
Published formulations differ (formal spec §1.1). [ST98] uses triplets over
∧, ∨, →. [TR12] uses ∧, ∨, ⇒, ⇔, ⊕. The patent is commonly described as using
implication triplets only.

## Options
1. One kind (implication only, or AND only). ↔ then needs several triplets
   plus auxiliary nodes, which loses proof power on ↔/⊕-heavy formulas.
2. A kind per connective (∧, ∨, →, ↔, ⊕). Five rule tables, and the extra kinds
   add no proof power.
3. **Two kinds, AND and EQ, with literal polarity.** ∨ and → become AND with
   complemented literals, which is exactly equivalent under complement-closed
   relations (SPEC-S2). ⊕ becomes EQ with a negated output.

## Decision
Option 3. A triple is `(kind: u8, x: u32 node, a: Lit, b: Lit)`. Both kinds
use the semantic local closure SPEC-S1. Each implementation stores triples as
a struct of arrays, a tuple list or facts, as it prefers. All of them read the
same text format (`triple-format.md`, Phase 1), and an occurrence index
`node → [triple id]` is built at load time.

## Consequences
* There are two rule tables, and both are generated from SPEC-S1, so they
  cannot drift apart.
* XOR-heavy families (PRD §40 C) keep native EQ strength.
* There is no direct comparison against an implication-only formulation. If
  wanted, that becomes a Track B experiment.
