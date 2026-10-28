# Technology Feasibility Report (Phase 0 / M0)

Date: 2026-10-28. Everything below was checked on the development machine on
that date unless it is marked *unverified*. Throwaway probes were built
outside the repository.

## 1. Development hardware and OS

| Item | Value |
|------|-------|
| Machine | Apple M2, 8 cores (4P + 4E), 16 GiB unified memory |
| OS | macOS 15.7.4 (24G517) |
| GPU | Apple M2 GPU (Metal). No CUDA. |

Benchmark hardware requirements (PRD §44) are as follows.

* Primary CPU benchmarks run on this machine, on AC power, with no other
  heavy load.
* macOS cannot pin a CPU governor, and P/E-core scheduling is not
  controllable. Both MUST be recorded as a known noise source. Thread-scaling
  results above 4 threads mix P and E cores and MUST be labelled as such.
* Peak RSS is measured with `getrusage`/`/usr/bin/time -l`. On macOS these
  report bytes. On Linux, `ru_maxrss` is in KiB, which matters for the cloud
  GPU later (A5).

## 2. Rust (Implementation A)

| Item | Finding |
|------|---------|
| Toolchain | rustc/cargo 1.99.0 stable (2026-09-28), host `aarch64-apple-darwin`, edition 2024. **OK** |
| `rayon` | 1.12.0, MSRV 1.80. Candidate for parallel branch exploration (Track B). |
| `proptest` | 1.12.0, MSRV 1.88. Property tests (PRD §36). |
| `smallvec` | Latest stable is 1.x. The newest published release is 2.0.0-beta.3. If used, pin 1.x. |
| SAT oracle crates | `batsat` 0.6.0 (MIT), `rustsat` 0.7.5 (MIT), `splr` 0.19.0 (MPL-2.0, MSRV 1.98), `varisat` 0.2.2 (unmaintained since 2021). |
| External solvers | `kissat`, `cadical` and `minisat` are **not installed**. |

**Oracle decision (proposal).** Use an in-crate truth-table oracle for
formulas with up to 20 variables. For larger instances, add `batsat` as a
**dev-dependency only**, so it never enters the published library's
dependency tree. An external `kissat` MAY be used inside `_bench/` to label
corpus instances. The oracle never replaces the Stålmarck engine (PRD §36).

Feasibility: **no blockers.**

## 3. Scryer Prolog (Implementation B, feature `scryer`)

Version: **`scryer-prolog` 0.10.0** (BSD-3-Clause, MSRV 1.85). The CLI on
PATH reports git `126d7bb-modified`, which is not the crate. Use the crate.

### 3.1 Verified embedding API (read from the 0.10.0 source and exercised in a probe)

```rust
use scryer_prolog::{MachineBuilder, StreamConfig, LeafAnswer, Term, Machine};

let mut m: Machine = MachineBuilder::new()
    .with_streams(StreamConfig::in_memory())
    .build();
m.consult_module_string("stal", PROGRAM_TEXT);   // returns ()
m.load_module_string("stal", PROGRAM_TEXT);      // also available
for answer in m.run_query("goal(X).") {          // QueryState: Iterator
    match answer {                               // Result<LeafAnswer, Term>
        Ok(LeafAnswer::True) | Ok(LeafAnswer::False) => {}
        Ok(LeafAnswer::Exception(t)) => {}
        Ok(LeafAnswer::LeafAnswer { bindings, .. }) => {} // BTreeMap<String, Term>
        Err(t) => {}
    }
}
```

`Term` has the variants `Integer`, `Rational`, `Float`, `Atom`, `String`,
`List`, `Compound(String, Vec<Term>)` and `Var`. It is `#[non_exhaustive]`.
`LeafAnswer::LeafAnswer` is also `#[non_exhaustive]`.

Use `default-features = false`. The defaults pull in `ffi`, `repl`, `tls`,
`http` and `crypto-full`, none of which the solver needs. The probe built
with no default features.

### 3.2 Measured (probe, release build)

| Measurement | Value |
|-------------|-------|
| Clean release build of the dependency tree | ≈ 59 s |
| `MachineBuilder::build()` | ≈ 58 ms |
| First small query after consult | ≈ 0.3 ms |
| 1 000 trivial queries (`X is i+1.`) on one machine | ≈ 1.8 ms total |
| Probe binary size | 6.2 MB |

### 3.3 Constraints found (these must shape the design)

1. **Queries are strings.** `run_query` parses Prolog text and panics on a
   parse error (`expect("Failed to parse query")`). All queries MUST be
   generated mechanically. Large data SHOULD be loaded as facts through
   `consult_module_string`, not inlined in queries.
2. **Uncaught exceptions poison the machine.** After `throw(oops)` escapes a
   query, the next queries (`fail.`, `X = 2.`) all returned the *same stale
   exception*. The state recovered only after a query that ran `catch/3`.
   **Every query MUST be wrapped as `catch(Goal, E, true)` or similar, and
   errors returned as data.** This should be reported upstream.
3. **Module scoping.** `use_module(library(lists))` inside a consulted module
   does not make `member/2` visible to top-level queries. Queries MUST call
   module-qualified entry points that the solver program exports.
4. **`Machine` is not `Send`**, because of `NonNull` stream pointers. Each
   machine lives on the thread that built it. Parallel Prolog branches would
   need one machine per worker thread, built on that thread.
5. **No host callback API.** The 0.10.0 public API has no way to register a
   Rust predicate that Prolog can call. The `ffi` feature uses libffi to call
   C functions and is not a Rust callback. Therefore any step Prolog performs
   (for example branch-local saturation) must be written in Prolog.
6. **No resource-limit API** on `Machine`. Limits must be enforced in Prolog,
   with a step budget threaded through the solver, or with
   `call_with_inference_limit/3`. M3 confirmed that this predicate exists
   in `library(iso_ext)`.
7. `QueryState::drop` carries an upstream FIXME about iterators that are not
   fully consumed. The solver MUST always drain the iterator.
8. **Debug builds abort** (found in M3). `MachineBuilder::build()` calls
   `NonNull::new_unchecked` on a null pointer, inside `Heap::clear`. Rust's
   debug-mode precondition checks catch it, and a three-line program
   reproduces it. Release builds run, but the code relies on undefined
   behaviour. Workaround: `[profile.dev.package.scryer-prolog]
   debug-assertions = false` in `Cargo.toml`. This should be reported
   upstream.
9. **`xor` is not an infix operator** in Scryer. Write `xor(A, B)` inside
   `is/2`.
10. **Module quirks:** consulting a module that imports `library(lists)`
    prints `error(syntax_error(invalid_module_declaration),load/1)`, yet the
    module loads and works. A source consisting of only a module declaration
    hung the machine. After a failed load, a later query panicked with an
    out-of-bounds index inside Scryer.

Feasibility: **feasible, with design constraints.** See ADR-0005.

## 4. Bend (Implementation C, git-ignored `bend/`)

There are two incompatible Bends.

### 4.1 Bend 1 (`bend-lang` 0.2.38 + `hvm` 2.0.22, crates.io, Apache-2.0)

Installed from crates.io into a scratch directory and probed.

| Item | Finding |
|------|---------|
| CLI | `check`, `run-rs`, `run-c`, `run-cu`, `gen-c`, `gen-cu`, `gen-hvm`. `-s` prints interaction count and time. |
| Backends on this Mac | `run-rs` (sequential Rust interpreter) and `run-c` (multithreaded C). `run-cu` needs CUDA, so it is unavailable here. |
| Parallel probe (binary-tree sum, 2²² leaves) | `run-rs` 2.87 s; `run-c` 0.33 s wall with 2.26 s user (≈ 6.8× CPU parallelism). |
| Memory | ≈ 1.6–1.8 GB peak RSS for a trivial program. HVM preallocates its heap, which swamps any solver-size signal. |
| Numbers | **24-bit** (`u24`/`i24`/`f24`). The probe's sum overflowed silently: it printed 14680064. Node ids above 2²⁴ ≈ 16.7 M are impossible, and arithmetic wraps. |
| Data structures | No mutable arrays. Trees and `Map` (a persistent tree keyed by u24). |
| Status | The upstream repo now says Bend 1 programs and HVM "do not carry over" to Bend 2. Bend 1 is **no longer the current language**. |

### 4.2 Bend 2 (github.com/bendlang/bend, current): verified on the M2

**Bend 2.0.39** was installed from the darwin-arm64 release. The installer
script was read first; it checks the archive's sha256. It was installed into a
scratch `BEND_HOME` with `BEND_NO_TELEMETRY=1`, and was exercised on
2026-10-28. Full measurements are in `_bench/bend-baseline/README.md`, which
is git-ignored.

| Item | Finding |
|------|---------|
| Platform | Native arm64 binary. Builds with Apple clang 17.0.0, even though the README says "19+" is needed for `!`. **Works.** |
| Targets verified | Native C, multicore, `+RTS --threads N`: works. **Metal GPU (`f!(…)`, `file.gpu` beside the binary): works on the M2.** `+RTS --gpu off` works. |
| Parallel scaling (tree-sum probe, d=28) | 1 thread 869 ms; 4 threads 230 ms (3.8×); 8 threads 158 ms (5.5×) |
| Metal (same probe) | Fixed overhead ≈ 70–90 ms (≈ 400 ms on the first launch). 266 ms at d=28, so slower than 8 CPU threads for this branchy integer workload. |
| Memory | 1.4–1.9 MB peak RSS on CPU, ≈ 11 MB with Metal. Bend 1 used 1.8 GB. |
| Arrays | `Array<T>` is a **balanced binary tree**: `get`/`set` cost O(log N), and `Array.clone` costs O(N). Writes reuse nodes because each array has one owner. |
| Union-find spike | Parity union-find on `Array<U32>`, cloned, with two branches as one parallel call: 4 M unions per branch take 69 ms on 1 thread and 38 ms on 2 threads (1.8×). Correct answers. |
| Numbers | Nat, U32, F32. U32 node ids are fine (spec limit 2³¹). |
| IO | `IO.args`, `IO.now()` (milliseconds, so in-process timing *is* possible), `File.open/read/read_at/size` |
| Language constraints | `match` and destructuring work only on parameters or fields, so helper defs are needed. The termination checker wants the decreasing `Nat` as the first argument, and unbounded loops use `def f?`. Operators need type annotations. |
| Maturity | Upstream: "BEND IS YOUNG. EXPECT BUGS." The compiler is AI-written and not fully audited. Differential testing (PRD §35) is the safeguard. |

**Impact on amendment A5.** GPU execution is available on this machine through
Metal. The project owner approved narrowing A5 on 2026-10-28: Metal results
on the M2 are in scope, and only CUDA results stay deferred.

### 4.3 Decision

The project owner chose **Bend 2** on 2026-10-28. Spike items 1 and 2
(union-find on `Array<U32>` on C and Metal, and a parallel two-branch fork with
`Array.clone`) passed. Item 3 (reading a triple file) moves to Phase 4; the
`File` API it needs was confirmed in Base. Bend 1 is kept only as the
historical baseline in `_bench/bend-baseline/`.

## 5. Implementation risks found in Phase 0

| Risk | Mitigation |
|------|------------|
| Sharing changes hardness (SPEC-D5) and could make implementations disagree | Track A uses one shared triple file. Sharing on/off is a reported configuration. |
| Scryer exception poisoning (§3.3.2) | `catch/3` wrapper on every query, plus a regression test |
| Scryer per-branch serialization dominating runtime | This is what PRD §24 wants measured. Batching experiments follow PRD §25. |
| Bend 2 immaturity | Differential tests against Rust. Pin the Bend version, 2.0.39 for now. |
| Bend 2 arrays are trees (O(log N) access) | Expected constant-factor cost. Report it, and do not hide it behind "in-place". |
| Metal overhead ≈ 70–90 ms per launch | Report it separately. Use the GPU only at branch level, where the work per launch is large. |
| Order-dependent counters | SPEC-Z4/K3 canonical order. Counters are compared only in Track A sequential mode. |
| macOS P/E-core noise | Record core type. Interleave and repeat runs (PRD §44). |
