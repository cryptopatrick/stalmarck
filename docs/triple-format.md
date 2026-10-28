# Input, Triple and Relation Formats

Status: **M1, v1** (2026-10-28). This document gives the concrete syntax for
the decomposition defined in `formal-specification.md` §3, and the
serializations shared by all implementations (PRD §5, §33).

## 1. Formula text format (`.prop`)

UTF-8 text. One file holds exactly one formula. `#` starts a comment that
runs to the end of the line. Whitespace is insignificant.

### 1.1 Grammar

```
formula  := iff
iff      := imp   ( IFF imp )*            # left-associative
imp      := or    ( IMP imp )?            # right-associative
or       := xor   ( OR  xor )*            # left-associative
xor      := and   ( XOR and )*            # left-associative
and      := unary ( AND unary )*          # left-associative
unary    := NOT unary | atom
atom     := CONST | IDENT | "(" formula ")"
```

| Token | Spellings (keywords are case-insensitive) |
|-------|--------------------------------------------|
| NOT | `!` `~` `not` |
| AND | `&` `and` |
| XOR | `^` `xor` |
| OR | `\|` `or` |
| IMP | `->` `implies` |
| IFF | `<->` `iff` |
| CONST | `T` `F` `true` `false` |
| IDENT | `[A-Za-z_][A-Za-z0-9_.]*`, but not a keyword or `T`/`F` |

Precedence, from tightest binding to loosest:
`!` › `&` › `^` › `|` › `->` › `<->`. The first three follow C and Rust's
`& ^ |` order. So `a | b & c` is `a | (b & c)`, and `a -> b -> c` is
`a -> (b -> c)`.

The PRD §4 example `((a AND b) -> c) AND (a AND b)` is accepted as written.

### 1.2 Canonical printing

Every binary node is printed in parentheses as `(l op r)`, with
`op ∈ {&, ^, |, ->, <->}`. Negation is `!x` and constants are `T`/`F`.
Parsing the printed form gives back the same formula. Variables whose names
are not valid IDENT tokens cannot be printed this way.

## 2. DIMACS CNF (`.cnf`)

This is the standard `p cnf V C` header followed by clauses ending in `0`.
Lines starting with `c` are comments. A `%` line ends the input, as in the
SATLIB convention.

**Translation (SPEC-D4 for CNF).** Variable `i` (1 ≤ i ≤ V) is given node
`i`. All V variables are allocated in this order before any triple, even if
some never occur. Variable `i` is named `x<i>` in models. Each clause
`l₁ … lₙ` becomes the left fold `((l₁ ∨ l₂) ∨ …) ∨ lₙ`. The formula is the
left fold of `∧` over the clauses, in file order. An empty clause is `⊥`, and
a CNF with no clauses is `⊤`. Sharing (SPEC-D5) applies.

Translating the same clauses through `.prop` with names `x<i>` gives the same
triples up to node numbering. `.prop` numbers variables by first occurrence.

## 3. Triple file format (`.tri`, version 1)

A triple file is what every implementation reads in Track A (PRD §33). Lines
are whitespace-separated. `#` lines are comments. The fields must appear in
this order:

```
stalmarck-triples 1
nodes <N>                 # node count including node 0 (⊤)
vars <V>
v <node> <name>           # V lines, ascending node
triples <T>
t <kind> <out> <a> <b>    # T lines; kind ∈ {and, eq}; a, b are literal codes
root <lit>
```

A literal code is `2·node + negated` (SPEC-D2). For example `0` = ⊤,
`1` = ⊥, `7` = ¬node 3.

**Validity rules.** A reader MUST reject a file that breaks any of these:

1. The `v` and `t` nodes are pairwise distinct, lie in `1..N`, and together
   cover `1..N−1` exactly once.
2. Every `t` line's `out` is greater than every node its operands refer to.
   Equivalently, triples are listed in topological order.
3. `a ≤ b`. For `eq`, both `a` and `b` are even (positive).
4. Names are unique and contain no whitespace.
5. Nothing follows the `root` line.

A file written with sharing contains no two triples with the same
`(kind, a, b)`. One written without sharing may repeat them. Readers do not
check this, because the file does not record which mode was used.

Writers emit exactly this layout, so a file written by the reference
decomposer is byte-identical across runs.

**Example.** `a & (b | c)`, with sharing:

```
stalmarck-triples 1
nodes 6
vars 3
v 1 a
v 2 b
v 3 c
triples 2
t and 4 5 7
t and 5 2 9
root 10
```

Node 4 is `¬b ∧ ¬c`, so `b ∨ c` is literal 9 = ¬4, and node 5 is
`a ∧ (b ∨ c)`. Each output node is created after its operands, in
post-order. A unit test checks that the reference decomposer writes exactly
this text.

## 4. Canonical relation serialization

The canonical form of SPEC-R4 is written as one line:

* `contradiction`, or
* `classes c₀ c₁ … c_{N−1}`, where `c_v = 2·m + p`. Here `m` is the smallest
  node of `v`'s class, and `p = 1` iff `v ≡ ¬m`.

This line is the checkpoint compared in differential tests (PRD §35).

## 5. Result records

Solver results use the JSON schema `shared/schemas/result-v1.schema.json`.
Test vectors use the manifest described in `shared/test-vectors/README.md`.
