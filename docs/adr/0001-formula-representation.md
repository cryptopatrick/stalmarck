# ADR-0001: Formula representation

Status: Accepted (M0)

## Context
PRD §4 requires a canonical internal representation, ideally an
integer-indexed DAG, and asks for repeated subexpressions to be handled
explicitly. Hash-consing changes proof hardness (formal spec SPEC-D5, §12), so
this choice affects semantics as well as performance.

## Options
1. Pointer-based tree (`Box<Expr>`), with sharing as a separate pass.
2. An arena of nodes addressed by `u32`, with a hash-cons table. Negation is
   stored as a literal bit, not as a node.
3. Direct streaming from the parser to triples, with no AST.

## Decision
Use option 2 for decomposition. The parser builds a small owned AST, used only
for error reporting, and lowers it in a single pass (SPEC-D4) into a literal
DAG:

* `Lit(u32) = 2·node + neg`; node 0 is ⊤;
* hash-consing on `(kind, a, b)` with sorted operands, enabled by default and
  switchable off (SPEC-D5);
* no constant folding or other simplification.

Node ids are `u32`. Ids above `2³¹ − 1` are rejected, so a literal fits in a
`u32`. Bend 1, if chosen, adds a lower limit of `2²⁴` (ADR-0006).

## Consequences
* The same input always yields the same triples, independent of the
  implementation, because only the Rust decomposer produces Track A triple
  files.
* Hardness results must state whether sharing was on.
* Implementations B and C may reuse A's decomposer through the triple file
  (PRD §33), so only Rust needs a parser.
