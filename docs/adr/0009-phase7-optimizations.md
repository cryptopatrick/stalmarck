# ADR-0009: Phase 7 optimizations

Status: Accepted (Phase 7, first round)

## Context
The pilot (ADR-0008) showed Bend about 65× slower than Rust and Rust + Scryer
about 2 000× slower. PRD §57 requires profiling first, and before-and-after
measurements for every reported optimization.

## Profile
* **Rust + Scryer:** formatting and parsing at the Rust–Prolog boundary cost
  under 1 % of the time. In CPU time on four instances, local closure took
  79–85 % of the Prolog time, the meet 0–6 %, and everything else 14–19 %.
  After the rule cache, exporting the relation took 83–95 % on parity
  chains. On random 3-CNF, loading it took 39 %. So the cost of the hybrid
  design is rebuilding and scanning state inside Prolog on every branch, not
  moving it across the boundary.
* **Bend:** merging sorted a key for every node on every dilemma, against
  Rust's delta merge. This was found by reading the code and the pilot's
  scaling. A later `sample` of the compiled binary on one 3-CNF instance
  attributed about 54 % of active samples to runtime memory management,
  about 26 % to table access and about 6 % to local closure.

## Decisions
1. **Bend delta merge (ADR-0007, option 3).** `St` gains a trail of the
   children linked since the enclosing branch began. A surviving branch's
   trail is joined to its parent's. The meet keys only nodes changed in both
   branches, as Rust's rollback engine does.
2. **Scryer rule cache.** Local closure is computed once per pattern (triple
   kind, node positions, literal signs, and the known relations among the
   nodes) and cached in `memo/2`. Positions ascend with nodes, so derived
   equivalences keep their order. This is PRD §25 item 4 (compact terms)
   applied to rules.
3. **No Prolog delta merge.** The profile gives it at most 6 % to win.
4. **Scryer sparse export.** `canonical/2` lists only the live merged
   classes instead of scanning every node.
5. **Bend table walk.** `get` and `put` read the path bits on the way down
   instead of building a bit list for each access.

All four optimizations keep the final relation and all eight counters identical
to the Rust reference, so processing order is unchanged. They are named
optimizations and are reported as such (PRD §39).

## Results
Solver-only medians on one M2, interleaved before/after runs. Ratios are over
instances where the unoptimized build needs at least 20 ms.

| Optimization | n | Median speedup | Range |
|--------------|---|----------------|-------|
| Bend delta merge, sequential | 48 | 0.98× | 0.87–19.9× |
| Bend delta merge, 4 threads | 48 | 0.99× | 0.84–18.7× |
| Bend table walk, sequential | 47 | 1.33× | 1.27–1.43× |
| Scryer rule cache | 15 | 1.83× | 1.07–2.74× |
| Scryer sparse export | 15 | 1.08× | 1.01–15.4× |

* The delta merge removes Bend's worst cases, with the largest gap to Rust
  falling from 1 356× to 145×, but not its typical gap, which stays about
  65×. It costs up to about 15 % on random 3-CNF, where the trail does not
  pay for itself.
* The table walk speeds up every instance, and takes Bend's median gap to
  Rust to about 48×. The largest gap is now 109×.
* The rule cache takes the median Scryer gap to Rust from about 2 000× to
  about 1 200×, and sparse export takes it to about 650×. Export is what
  held back `C-parity-1024`: 33.6 s became 2.2 s. The largest gap is now
  about 1 900×.

Every run in all four sets passed the correctness gate, with no relation or
counter mismatch (1 644, 816, 816 and 1 032 runs). The shared vectors agree with Rust in
Bend (552/552, sat, seq and par) and in the Scryer test suite. Bend validity
mode: 514 agree, 0 differ, 38 timed out at 60 s.

## Limitations
Median load was 1.7 to 3.7 across the four runs, on an 8-core machine shared
with other work. Differences of a few percent are not
meaningful.
