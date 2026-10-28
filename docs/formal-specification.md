# Formal Specification of Stålmarck's Proof Procedure

Status: **M0 baseline, v0.1** (2026-10-28)
Scope: PRD §3–§13. This document is the authoritative, language-independent
definition that the Rust, Rust + Scryer and Bend implementations must follow.
The concrete text syntax, node numbering and serialization of triples belong to
`docs/triple-format.md` (Phase 1); this document fixes their meaning.

The words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119. Rule
identifiers (`SPEC-…`) are stable and are referenced by tests and ADRs.

---

## 1. Sources and the selected formulation

| Key | Source | Used for |
|-----|--------|----------|
| [ST98] | M. Sheeran, G. Stålmarck. *A Tutorial on Stålmarck's Proof Procedure for Propositional Logic.* FMCAD '98, LNCS 1522, pp. 82–99, Springer, 1998. | Primary source: formula relations, proper rules, Dilemma rule, k-saturation, triplets. |
| [TR12] | A. Thakur, T. Reps. *A Generalization of Stålmarck's Method.* SAS 2012, LNCS 7460, pp. 334–351, Springer, 2012. | Secondary source: algorithmic presentation (Alg. 1–5), literal-level equivalence relations, worked examples, the join view of branch merging. |
| [Pat] | G. Stålmarck. US Patent 5 276 897 (1994); SE 467 076; EP 0 403 454. | Cited only, not consulted. |
| [Har96] | J. Harrison. *Stålmarck's Algorithm as a HOL Derived Rule.* TPHOLs 1996, LNCS 1125. | Cited only, not consulted. |

Both PDFs ([ST98], [TR12]) are in the git-ignored `_theory/` folder.

### 1.1 How the published formulations differ

1. **Branching.** The Dilemma *proof system* of [ST98 §3.1] branches on
   `A ≡ B` / `A ≡ ¬B` for any two non-equivalent subformulas A, B. The
   *procedure* that [ST98 §4] says is implemented branches on `l ≡ ⊥` /
   `l ≡ ⊤` for one subformula `l`. [TR12, Alg. 3–4] branches on two
   variables `vi ≡ vj` / `vi ≡ ¬vj`, where `vj` may be a constant.
2. **Connectives in triplets.** [ST98 §4.1] builds triplets `x : y ∘ z` over
   ∧, ∨, →. [TR12] uses integrity constraints over ∧, ∨, ⇒, ⇔, ⊕. The
   original patent formulation is often described as using implication
   triplets only. We have not checked that against [Pat] or [Har96].
3. **Simple rules.** [ST98 §2.6] defines the rule set *semantically*, as the
   set of all *proper* rules for a connective. [TR12, Fig. 2] lists only
   example rules.
4. **Pseudo-code.** In [ST98 §4], `saturate(R,k+1)` never assigns `R := R'`
   before testing `until R' = R`. We read this as the intended
   "repeat passes until a pass adds nothing" (SPEC-K3).

### 1.2 Selection

The project implements **[ST98]'s Dilemma procedure** with these choices:

* branching on **node ≡ ⊤ / node ≡ ⊥** (the implemented variant of [ST98 §4]);
* triplets over **two connectives, AND and EQ (↔)**, with literal polarity
  (§3);
* simple rules defined **semantically** as the closure under all proper rules
  of one triplet (§5), which is exactly [ST98 §2.6];
* merge = intersection of formula relations, with a contradictory branch acting
  as identity ([ST98 §3.1], [TR12 §2.1]).

---

## 2. Formulas

**SPEC-F1 (syntax).** Formulas are built from variables, the constants ⊤ and ⊥,
`¬`, and the binary connectives `∧`, `∨`, `→`, `↔`, `⊕`. Precedence and
concrete syntax are fixed in `triple-format.md`. No other constructs are
supported.

**SPEC-F2 (semantics).** The semantics is classical. An assignment `σ` maps
variables to {0,1}, and `⟦φ⟧σ` is defined in the standard way
(`⊕` = inequality, `↔` = equality).

**SPEC-F3 (questions).** An input is a formula φ and a *mode*:

* `sat` asks whether some σ satisfies φ;
* `valid` asks whether every σ satisfies φ (validity of φ = unsatisfiability
  of ¬φ).

---

## 3. Structural decomposition

**SPEC-D1 (nodes).** Decomposition produces nodes `0..N-1`. Node `0` is the
constant **⊤**. Each distinct input variable gets exactly one node. Each
triple gets exactly one fresh *output node*.

**SPEC-D2 (literals).** A literal is a node with a polarity, written `v` or
`¬v`. It is encoded as the integer `2·v + s`, where `s ∈ {0,1}` and 1 means
negated. ⊥ is the literal `¬0`. Negation never creates a node. It only flips
`s`.

**SPEC-D3 (triples).** There are exactly two triple kinds. In each, `x` is a
node and `a`, `b` are literals:

| Kind | Meaning |
|------|---------|
| `AND(x, a, b)` | `x ↔ (a ∧ b)` |
| `EQ(x, a, b)` | `x ↔ (a ↔ b)` |

**SPEC-D4 (translation).** `lit(φ)` is defined recursively. `mk(K, a, b)`
returns a literal, as defined in SPEC-D5.

| φ | lit(φ) |
|---|--------|
| variable p | `node(p)` |
| ⊤ / ⊥ | `0` / `¬0` |
| ¬ψ | `¬lit(ψ)` |
| ψ ∧ χ | `mk(AND, lit ψ, lit χ)` |
| ψ ∨ χ | `¬mk(AND, ¬lit ψ, ¬lit χ)` |
| ψ → χ | `¬mk(AND, lit ψ, ¬lit χ)` |
| ψ ↔ χ | `mk(EQ, lit ψ, lit χ)` |
| ψ ⊕ χ | `¬mk(EQ, lit ψ, lit χ)` |

Operands are translated left before right. That order fixes node numbering
(details in `triple-format.md`).

**SPEC-D5 (normalization and sharing).**

1. EQ operand polarity is pushed to the output: `mk(EQ, ¬a, b) = ¬mk(EQ, a, b)`.
   Afterwards both stored EQ operands are positive.
2. Operands of both kinds are stored with `a ≤ b`, compared as literal
   integers. Both kinds are commutative.
3. **Structural sharing (hash-consing) is on by default.** If a triple with the
   same `(kind, a, b)` already exists, `mk` returns its output literal and makes
   no new triple.
4. Nothing else is simplified. In particular there is no constant folding, and
   `a ∧ a`, `a ∧ ¬a`, `a ↔ a` and triples with constant operands are kept. The
   rules in §5 handle such *degenerate* triples uniformly.

A non-sharing mode (step 3 disabled) MUST be available for experiments.
Sharing changes proof hardness. For example, `(p→q) ↔ (¬q→¬p)` needs depth 1
without sharing but depth 0 with it, because both sides normalize to the same
node (§12). Track A benchmarks MUST therefore feed every implementation the
same triple file (PRD §33).

**SPEC-D6 (size accounting).** Reported sizes are: number of input variables,
number of triples (`N − 1 − #variables`), and number of literal occurrences
(`3 × #triples`).

**SPEC-D7 (initial relation).** Let `r = lit(φ)`. The initial relation is
`R₀ = Id(r ≡ ⊤)` in `sat` mode and `R₀ = Id(r ≡ ⊥)` in `valid` mode. In
`valid` mode the procedure tries to refute ¬φ.

---

## 4. Formula relations

**SPEC-R1 (formula relation).** Following [ST98 §2.8], a formula relation `R`
is an equivalence relation on the literal set `L = {v, ¬v | 0 ≤ v < N}`. It
must be *complement-closed*: `a ~ b` implies `¬a ~ ¬b`. "a ≢ b" is written
`a ~ ¬b`. `Id` is the identity relation. `R(a ≡ b)` is the least formula
relation containing `R` and the pair (a, b).

**SPEC-R2 (contradiction).** `R` is *explicitly contradictory* iff `v ~ ¬v` for
some `v`, equivalently `⊤ ~ ⊥`. Every contradictory relation is identified with
the single top element **⊥⊥**, the universal relation [ST98 §3.1].
Implementations MAY stop at the first contradiction they detect.

**SPEC-R3 (parity view).** A non-contradictory `R` is the same thing as a
partition of the nodes in which each node `v` carries a parity `π(v)` relative
to its class. Then `a ~ b` holds iff the two literals' nodes are in the same
class and the parities, adjusted for each literal's polarity, agree. This is
"union-find with parity". Implementations MAY use any representation, but MUST
expose the canonical form of SPEC-R4.

**SPEC-R4 (canonical form).** For a non-contradictory `R`, the canonical form is
the vector `C(R)[v] = (m, p)` for `v = 0..N-1`. Here `m` is the smallest node
in `v`'s class and `p ∈ {0,1}` is such that `v ~ m` when `p = 0` and `v ~ ¬m`
when `p = 1`. For a contradictory `R`, the canonical form is the token
`CONTRADICTION`. Two relations are equal iff their canonical forms are equal.
Implementations MUST NOT compare internal union-find arrays (PRD §6).

*Consequences:* every node equivalent to ⊤ or ⊥ has `m = 0`. A node `v` is
**determined** in `R` iff `C(R)[v].m = 0`. Its value is then `1 − p`.

**SPEC-R5 (order and intersection).** `R ⊑ S` iff `R ⊆ S` as sets of pairs, and
**⊥⊥** is the top element. The meet of two branch results is:

```
R₁ ⊓ R₂ = R₂            if R₁ = ⊥⊥
        = R₁            if R₂ = ⊥⊥
        = R₁ ∩ R₂       otherwise
```

The intersection of two formula relations is again a formula relation.

**SPEC-R6 (computing ∩).** For each node `v`, take `(r₁, p₁)` and `(r₂, p₂)`,
its class identifier and parity in `R₁` and in `R₂`. Use the key
`κ(v) = (r₁, r₂, p₁ ⊕ p₂)`. Nodes `u` and `v` share a class in `R₁ ∩ R₂` iff
`κ(u) = κ(v)`. In that case `u ~ v` if `p₁(u) = p₁(v)`, and `u ~ ¬v`
otherwise. Grouping by κ, by sort or hash, costs O(N) or O(N log N). The
result MUST equal the set intersection. This is property-tested against a
brute-force pair-set intersection.

---

## 5. Simple rules (one triple)

**SPEC-S1 (local closure, normative).** Take a triple `t` with output node `x`
and operand literals `a`, `b`. Let `V(t) = {0, x, node(a), node(b)}` (a set,
with 2 to 4 elements). Let `M(R, t)` be the set of assignments
`σ : V(t) → {0,1}` such that:

1. `σ(0) = 1`;
2. `σ` satisfies `t` (SPEC-D3, with `σ(¬v) = 1 − σ(v)`);
3. for every pair `u, w ∈ V(t)` related in `R` with parity `p`,
   `σ(u) ⊕ σ(w) = p`.

Then:

* if `M(R, t) = ∅`, the step yields **⊥⊥**;
* otherwise it yields `R` extended with every pair `u ~ w` (or `u ~ ¬w`), for
  `u, w ∈ V(t)`, that holds in *every* σ ∈ M(R, t).

This step is written `step_t(R)`. It depends only on the restriction of `R` to
`V(t)`, which is one of finitely many patterns. Implementations SHOULD
therefore precompute it as a lookup table indexed by (kind, degeneracy shape,
restricted pattern), and MUST generate that table from this definition, not
write it by hand.

**SPEC-S2 (equivalence to [ST98]).** For an AND or EQ triple, applying `step_t`
until it stops changing gives the same relation as closing under all *proper
rules* of [ST98 §2.6]. *Proof sketch:* each proper rule is a minimal entailment
`F₁≡G₁,…,Fₙ≡Gₙ ⊨ F≡G` over `{a, b, x, ¬a, ¬b, ¬x, ⊤, ⊥}`. Its premises lie
in `R`, so σ ∈ M(R,t) satisfies them, so the conclusion holds in all of
M(R,t). Conversely, any equivalence true in all of M(R,t) is entailed by a
finite set of premises in `R`, and a minimal such set is a proper rule. An
empty M(R,t) has a minimal inconsistent premise set S. Pick any e ∈ S. Then
S∖{e} is consistent and entails ¬e, which is again an equivalence, and that
gives ⊥⊥. ∎ Rules for ∨ and → are the AND rules with complemented literals
(SPEC-D4), so the choice of connectives neither gains nor loses proof power.

**Rule tables (informative).** These are generated from SPEC-S1 by the M0
reference model. They show only premises that are a single equivalence. The
full table, including two-premise rules, is a Phase 1 test vector.

`x = a ∧ b`

| Premise | Conclusions |
|---------|-------------|
| x ≡ ⊤ | a ≡ ⊤, b ≡ ⊤ |
| a ≡ ⊥ | x ≡ ⊥ |
| b ≡ ⊥ | x ≡ ⊥ |
| a ≡ ⊤ | x ≡ b |
| b ≡ ⊤ | x ≡ a |
| a ≡ b | x ≡ a |
| a ≡ ¬b | x ≡ ⊥ |
| x ≡ ¬a | x ≡ ⊥, a ≡ ⊤, b ≡ ⊥ |
| x ≡ ¬b | x ≡ ⊥, b ≡ ⊤, a ≡ ⊥ |

Some conclusions need two premises, for example `a ≡ ⊤, b ≡ ⊤ ⟹ x ≡ ⊤`,
`x ≡ ⊥, a ≡ ⊤ ⟹ b ≡ ⊥`, and `x ≡ a, a ≡ ¬b ⟹ a ≡ ⊥, b ≡ ⊤`.

`x = a ↔ b`. Any one equivalence among {x, a, b, ⊤} determines the third
element relative to ⊤:

| Premise | Conclusion | Premise | Conclusion |
|---------|------------|---------|------------|
| x ≡ ⊤ | a ≡ b | x ≡ ⊥ | a ≡ ¬b |
| a ≡ ⊤ | x ≡ b | a ≡ ⊥ | x ≡ ¬b |
| b ≡ ⊤ | x ≡ a | b ≡ ⊥ | x ≡ ¬a |
| x ≡ a | b ≡ ⊤ | x ≡ ¬a | b ≡ ⊥ |
| x ≡ b | a ≡ ⊤ | x ≡ ¬b | a ≡ ⊥ |
| a ≡ b | x ≡ ⊤ | a ≡ ¬b | x ≡ ⊥ |

**SPEC-S3 (forbidden substitutes).** Implementations MUST NOT replace `step_t`
with clause-based unit propagation, which does not derive `x ≡ a` from
`b ≡ ⊤`, or with any other rule set. Additional sound rules are not permitted
in Track A (PRD §72).

---

## 6. 0-saturation

**SPEC-Z1 (definition).** `sat₀(R)` is the least formula relation `S ⊒ R` with
`step_t(S) = S` for every triple `t`. It exists and is unique because each
`step_t` is monotone and extensive on a finite lattice. It is **independent of
the order** in which rules are applied. It equals [ST98]'s 0-saturation and
[TR12]'s Alg. 2.

**SPEC-Z2 (algorithm).** Implementations compute `sat₀` with a
dependency-indexed worklist:

```
sat0(R, W):                       # W: initial worklist of triple ids
  while W non-empty and R ≠ ⊥⊥:
    t := pop(W)
    D := step_t(R) − R            # new pairs, as a list (SPEC-Z4)
    if step_t(R) = ⊥⊥: return ⊥⊥
    for (u, w, p) in D:
      if merge(R, u, w, p) = ⊥⊥: return ⊥⊥
      for t' in affected(merge): push(W, t') unless queued
  return R
```

**SPEC-Z3 (affected triples).** Merging classes `A` and `B` changes the
restriction of `R` to `V(t)` only if `t` mentions a node in `A` and a node in
`B`. ⊤ counts as mentioned by every triple. It is therefore sufficient, and
REQUIRED for Track A, to enqueue every triple that mentions a node of the
**scan class**, defined as:

* the class that does not contain ⊤, if one of them contains ⊤;
* otherwise the class with fewer nodes;
* on a tie in size, the class whose smallest node is larger.

The rule depends only on the relation, not on union-find internals. Every
triple whose restriction changes is enqueued. The total scanning work is
O(Σ occurrences · log N) without ⊤ and O(occurrences) per node for merges with
⊤. A triple already in the worklist is not enqueued again.

**SPEC-Z4 (canonical processing order, Track A).**

* The worklist is FIFO.
* At the root, the initial worklist is all triples in ascending id order.
* After adding equivalences to a relation that is already 0-saturated (a
  dilemma assumption, §7), the initial worklist is the affected triples of
  that merge, in ascending id order.
* `D` is listed in ascending `(u, w)` order, with `u < w` drawn from `V(t)`.
  Pairs already implied by an earlier merge in the same step count as
  *redundant* and change nothing.
* Affected triples of one merge are enqueued in ascending id order.

Order changes only intermediate states and counters, never `sat₀(R)`
(SPEC-Z1). Track B MAY use any order.

**SPEC-Z5 (results and invariants).**

| Result | Invariant |
|--------|-----------|
| `FixedPoint(S)` | `S = sat₀(R)`, S ≠ ⊥⊥, `step_t(S) = S` for all t |
| `Contradiction` | `sat₀(R) = ⊥⊥`. Every derived pair is a sound consequence, so it holds in every model of the starting assumptions. |
| `ResourceLimit(S)` | `R ⊑ S ⊑ sat₀(R)`. S is sound but not saturated. No result may be concluded from S, except that a ⊥⊥ found before the limit is a valid contradiction. |

**SPEC-Z6 (counters).** All implementations MUST report:

| Counter | Definition |
|---------|------------|
| `triples_processed` | pops of the worklist |
| `deductions_attempted` | equivalences in `D` |
| `deductions_successful` | equivalences in `D` that changed R |
| `merges` | class unions, including unions with ⊤ |
| `redundant` | `deductions_attempted − deductions_successful` |
| `worklist_insertions` | pushes |
| `worklist_removals` | pops |

In Track A sequential mode these counters MUST agree exactly across
implementations, because the order is canonical (SPEC-Z4).

---

## 7. The Dilemma rule

**SPEC-M1 (rule).** For a relation `R`, a node `v ≥ 1` and a depth `j ≥ 0`:

```
dilemma_j(R, v) = sat_j(R(v ≡ ⊥)) ⊓ sat_j(R(v ≡ ⊤))
```

`sat_j` is defined in §8, and the branch `v ≡ ⊥` is computed first.
**Soundness:** every model of `R` satisfies one of the two branches, so every
pair common to both branches holds in it [ST98 §3.1]. A branch that reaches
⊥⊥ has no models, which is why ⊥⊥ acts as the identity for ⊓.

**SPEC-M2 (merging retains only common consequences).** The merged relation
contains exactly the literal pairs that are related in both branches. Below,
both branches extend `R = Id` on nodes {⊤, p, q, r} with three different
assumptions:

| Branch v ≡ ⊥ | Branch v ≡ ⊤ | Merge | Valid? |
|---|---|---|---|
| p ≡ ⊤, q ≡ r | p ≡ ⊤, q ≡ ¬r | p ≡ ⊤ | yes |
| p ≡ ⊤ | p ≡ ⊥ | nothing new | yes. Keeping p ≡ ⊤ would be **invalid**: it holds in only one branch. |
| p ≡ q, q ≡ r | p ≡ r | p ≡ r | yes. Common pairs are kept even when the branches reach them by different routes. |
| ⊥⊥ | q ≡ ¬r | q ≡ ¬r | yes (contradictory branch is the identity) |
| p ≡ q | p ≡ ¬q | nothing new | yes. Keeping `p ≡ q` *or* `p ≡ ¬q` is **invalid**. Polarity is part of the pair. |
| ⊥⊥ | ⊥⊥ | ⊥⊥ | yes: R is refuted |

Property tests (PRD §11) MUST check that every pair in a merged state holds in
every model of `R` (brute force on small formulas). They MUST also check that
the merge equals the brute-force pair-set intersection.

**SPEC-M3 (merged state needs no re-propagation).** If both branches are
`sat_j`-closed, so is `R₁ ⊓ R₂`.

*Proof:* the operators are monotone. So
`sat_j(R₁ ∩ R₂) ⊑ sat_j(R₁) = R₁`, and likewise for R₂. Hence
`sat_j(R₁ ∩ R₂) ⊑ R₁ ∩ R₂`. ∎

An implementation that installs the merged relation into the parent state, for
example by applying `(R₁ ⊓ R₂) − R` as unions, MUST NOT need to propagate
again. Debug builds SHOULD assert this by running `sat₀` on the merged state
and checking that nothing changes.

**SPEC-M4 (not backtracking).** Each dilemma explores *both* branches to their
saturated relations, then keeps their *intersection*. It is not a search for
one satisfying branch. Prolog backtracking, or a DPLL-style decision with undo,
MUST NOT stand in for it. Undo or rollback is allowed only as a way to
*restore the parent state* between branches (ADR-0004).

---

## 8. k-saturation and the search strategy

**SPEC-K1 (definition).** For `k ≥ 1`, `sat_k(R)` is the least relation
`S ⊒ sat_{k−1}(R)` such that `dilemma_{k−1}(S, v) = S` for every node
`1 ≤ v < N`. `sat_0` is defined in SPEC-Z1.

**SPEC-K2 (order independence).** `sat_k(R)` is uniquely defined and does not
depend on which node is branched on first.

*Sketch:* by induction on k, `sat_{k−1}` is monotone. Then each `dilemma_{k−1}(·, v)`
is monotone and extensive. By the chaotic-iteration theorem, any fair iteration
of finitely many such operators reaches their least common fixed point above
`sat_{k−1}(R)`. This holds both for sequential iteration and for parallel
iteration that joins several dilemma results and re-closes with `sat₀`. ∎

The M0 reference model checked this empirically on 1 200 random formulas with
k ≤ 2, using random worklist and branch orders (§12). Consequence: the
**canonical form of `sat_k(R₀)` is a differential-test checkpoint**, and it
must be identical across all three implementations, sequential or parallel.

**SPEC-K3 (canonical algorithm, Track A).**

```
sat_k(R):                                  # k ≥ 1
  R := sat_{k−1}(R)
  repeat
    changed := false
    for v in 1 .. N−1 (ascending):         # candidate set Sub(R), [ST98 §4]
      if R = ⊥⊥: return ⊥⊥
      if v is determined in R: skip        # SPEC-R4
      if C(R)[v].m ≠ v: skip               # v is not the smallest node of its class
      R' := dilemma_{k−1}(R, v)            # branch v≡⊥ first, then v≡⊤
      if R' ≠ R: changed := true; R := R'
  until not changed or R = ⊥⊥
  return R
```

Both skip rules preserve the result. Branching on a determined node gives ⊥⊥
on one side and R on the other. Branching on `v` where `v ~ ±u` gives the same
pair of branches as branching on `u`. Termination follows because R strictly
grows on each changed pass and the lattice is finite.

The Track A baseline uses this fixed order: ascending node id, with
determined nodes and non-minimal class members skipped. Other heuristics,
including [ST98]'s suggestion to merge the two largest classes, and branching
on arbitrary `A ≡ B`, are Track B experiments. They are reported separately
(PRD §12).

**SPEC-K4 (depth).** The depth k is a configuration parameter:
`max_depth`, default 2. The total number of branches explored is reported as
`dilemma_branches`. Merges that added at least one pair are reported as
`dilemma_productive`.

---

## 9. Results and completeness

**SPEC-C1 (result conditions).** Let `S` be the final relation (SPEC-D7 initial
relation, saturated to depth k, possibly cut short by a resource limit).

| Result | Condition |
|--------|-----------|
| **UNSAT** (`sat` mode) / **VALID** (`valid` mode) | `S = ⊥⊥`. Sound at every depth and even under a resource limit, because every derived pair is a consequence. |
| **SAT** (`sat` mode) / **INVALID** (`valid` mode) | `S ≠ ⊥⊥`, every input variable is determined in S, and the induced assignment σ is **verified**: every triple holds under σ and `lit(φ)` evaluates to the mode's root value. The model MUST be emitted. |
| **UNKNOWN** | Otherwise. The reason MUST be stated: `depth_exhausted` or `resource_limit:<which>`. |

Running out of dilemma depth without a contradiction MUST NOT be reported as
SAT (PRD §13).

**SPEC-C2 (why SAT is sound).** If every input variable is determined and `S` is
0-saturated and non-contradictory, every node is determined. This follows by
induction on output nodes, using `a,b ≡ const ⟹ x ≡ const` for both kinds.
All triples then hold, so σ is a model. Verification (SPEC-C1) still guards
against implementation bugs, and also covers relations cut short by a resource
limit.

**SPEC-C3 (completeness bound).** Let `m` be the number of input variables not
determined in a non-contradictory `sat_k(R)`. If `k ≥ m`, the relation is
satisfiable.

*Proof:* splitting on those m variables in sequence gives a depth-m Dilemma
derivation in which every leaf is fully determined. By SPEC-C2 every leaf is
either a model or ⊥⊥. If R were unsatisfiable, all leaves would be ⊥⊥. That
would be a refutation of depth ≤ k, which `sat_k` finds because it is closed
under that derivation [ST98 §3.3]. ∎

So the procedure is **complete when `k ≥ #variables`**. It is intended to be
used with small k (0–3), where it is a *semi-decision* procedure for easy
formulas, as in [ST98 §5] and [TR12 Alg. 5]. The default is not complete.

**SPEC-C4 (optional complete mode with witness).** With `complete = true`:

1. Iterate `k = 0, 1, …` until `S = ⊥⊥` or `k ≥ m(S)`.
2. To extract a witness, take the smallest undetermined input variable v. If
   `sat_{k−1}(S(v ≡ ⊤))` is `⊥⊥`, continue with `S(v ≡ ⊥)`; otherwise continue
   with `S(v ≡ ⊤)`. Re-saturate and repeat.

By SPEC-C3 the side that is kept is always satisfiable, so the procedure never
backtracks. This mode is a Track B extension, and its runs MUST be labelled as
such.

**SPEC-C5 (resource limits).** Limits on wall time, merges, branches and memory
MAY be configured. When a limit is hit, the result is UNKNOWN with
`resource_limit:<which>`, unless SPEC-C1 already allows UNSAT/VALID. The state
at the limit satisfies the `ResourceLimit` invariant (SPEC-Z5).

---

## 10. Complexity (informative)

Exhaustively searching for proofs of depth k costs O(n^(2k+1)) for formula size
n [ST98 §5]. A formula has hardness k (SPEC-K1) if `sat_k(R₀) = ⊥⊥` but
`sat_{k−1}(R₀) ≠ ⊥⊥`. Industrial formulas reported in [ST98 §3.3, §6] had
hardness 0–1. These figures are cited, not reproduced. Benchmark Family E
(dilemma-intensive) should include formulas of hardness 2–3.

---

## 11. Required properties for testing

These properties come from the specification. They feed PRD §35–§37.

| Id | Property |
|----|----------|
| P1 | Decomposition preserves meaning: for every assignment of the input variables, evaluating the triples bottom-up gives `lit(φ)` the value `⟦φ⟧σ`. |
| P2 | Soundness: every pair in `sat_k(R₀)` holds in every model of `R₀` (brute force, ≤ 12 variables). |
| P3 | UNSAT/VALID is reported only for unsatisfiable inputs; SAT only with a verified model. |
| P4 | Idempotence: `sat_k(sat_k(R)) = sat_k(R)`. |
| P5 | Order independence: canonical `sat_k` is identical under random worklist and branch orders, and under parallel branch evaluation. |
| P6 | Merge correctness: ⊓ equals brute-force intersection; merged state is `sat_{k−1}`-closed (SPEC-M3). |
| P7 | Rollback restores the exact canonical form of the parent state. |
| P8 | Completeness bound: with `k ≥ #variables`, no result is UNKNOWN for reason `depth_exhausted`. |
| P9 | Monotonicity: `R ⊑ R'` implies `sat_k(R) ⊑ sat_k(R')`. |
| P10 | Metamorphic: renaming variables and reordering commutative operands leave the result unchanged. Sharing on/off leaves SAT/UNSAT unchanged where both runs are definite. |

---

## 12. Validation of this specification (M0)

A throwaway Python model of SPEC-D, R, S, Z, M, K and C was written during M0.
It is not part of the repository. Results:

| Formula | Mode | Literature | Model (shared / unshared) |
|---------|------|------------|---------------------------|
| A∧(B∨C) → (A∧B)∨(A∧C) | valid | hardness 0 [ST98 §3.3] | 0 / 0 |
| (A∧B)∨(A∧C) → A∧(B∨C) | valid | hardness 1 [ST98 §3.3] | 1 / 1 |
| (a∧b) ∨ (¬a∨¬b) | valid | 0-saturation [TR12 Ex. 1] | 0 / 0 |
| (a∧(b∨c)) ↔ ((a∧b)∨(a∧c)) | valid | 1-saturation [TR12 Ex. 2] | 1 / 1 |
| (p→q) ↔ (¬q→¬p) | valid | 1 with equivalences [TR12 Ex. 3] | **0** / 1 (SPEC-D5 sharing) |

Random testing used 1 200 formulas with up to 4 variables and depth 4, all
connectives and constants, and sharing on and off. It ran k = 0, 1, 2 and
k = #variables. P2, P3, P5, P6 (SPEC-M3 assertion) and P8 held for all of
them, with no violations.

**Exit criterion of Phase 0 (PRD §50): no unresolved ambiguity in the core
inference rules.** The rules are fixed by SPEC-S1 and the procedure by
SPEC-Z1/K1. The Track A order is fixed by SPEC-Z3/Z4/K3, and the results by
SPEC-C1. The remaining open items (§13) do not affect inference semantics.

## 13. Open items (non-blocking)

1. Concrete syntax, precedence, DIMACS mapping and node numbering go into
   `triple-format.md` (Phase 1).
2. A JSON result schema (PRD §34) is due in Phase 1.
3. Whether to add EQ-based branching (`A ≡ B`) as a Track B experiment.
4. Consult [Pat] and [Har96] to confirm the description in §1.1(2). This is for
   the final report only. It does not change the selected formulation.
