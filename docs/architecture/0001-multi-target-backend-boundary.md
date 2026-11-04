# ADR 0001: Multi-Target Backend Boundary

- Status: Proposed
- Date: 2026-11-04
- Scope: Compiler architecture; no language-semantics change

## Context

Tarvos currently compiles a statically analyzable subset of Python to native
executables through a Rust code-generation path. The existing native pipeline
is valuable and must remain the correctness baseline while additional targets
are introduced.

A source inspection of the current `main` branch found:

1. `tarvos-ir` represents a module as nested `Stmt` values. Control flow is
   represented with structured `If`, `While`, and `For` statements. Values
   include typed expressions, calls, collections, indexing, slices, and
   format-string parts. The inspected IR file does not define basic blocks,
   terminators, CFG edges, or SSA values.
2. `tarvos-core::CompilePipeline::finish_from_ir` invokes
   `Optimizer::optimize` and then calls `RustCodegen::generate` directly.
   This is the immediate backend-selection coupling point.
3. `tarvos-optimizer::Optimizer::optimize` owns a fixed sequence of
   transformations: loop-induction reduction, copy propagation, constant
   folding, a second propagation/folding round, type specialization, and dead
   code elimination.
4. `tarvos-codegen-rust` depends on `tarvos-ir` and `tarvos-types`, not on
   the AST frontend. This is a useful existing separation to preserve.
5. CI already has meaningful quality gates, including formatting, workspace
   checks, Clippy, tests, a compatibility-matrix check, and native differential
   validation against pinned CPython on Windows and Linux.

These observations are limited to the files inspected for this proposal. They
are not a claim that every crate, test, or release path has been audited.

## Decision

Introduce a small, explicit backend boundary in the compiler orchestration
layer. Do not rewrite the frontend, IR, optimizer, or working Rust backend as
part of the first change.

The first backend contract should describe:

- A stable target identity (for example, native host target or a future WASI
  target).
- Target capabilities and explicit unsupported-feature diagnostics.
- The backend's accepted IR level and required runtime/ABI assumptions.
- The artifact kind produced (source, object, executable, or module).
- Backend options as typed configuration, not ad-hoc CLI strings.

The existing Rust code generator remains the first implementation of this
contract. Its output and native differential-test behaviour must remain
unchanged during the extraction.

## Proposed implementation sequence

### Step 0 — Baseline and invariants

- Record the exact compiler version, toolchain pin, and current CI gates.
- Run the complete workspace and native differential suites before refactoring.
- Add/retain end-to-end tests for representative language features and error
  cases.
- Treat the existing native backend as the behaviour oracle for refactoring.

### Step 1 — Target model

Add a target model in the orchestration layer, initially supporting only the
existing native Rust path. Keep target triples, operating-system identity,
architecture, ABI, and execution environment conceptually distinct. Do not
pretend that selecting a target makes cross-compilation available unless the
toolchain and linker/runtime requirements are actually satisfied.

### Step 2 — Backend contract

Define a backend interface and a compilation request/result model. Keep
backend-specific implementation details out of frontend and analysis crates.
The first adapter delegates to the existing Rust code generator.

### Step 3 — Capability checks

Represent target support explicitly. Reject unsupported language/runtime
features with actionable diagnostics before code generation where possible.
Do not silently stub features or claim universal Python compatibility.

### Step 4 — WASM feasibility vertical slice

Only after the boundary is tested, select one initial WASM environment (likely
WASI Preview 1 or Preview 2 after toolchain/runtime evaluation). Compile a small
supported program end-to-end, run it in a real WASM runtime, and compare its
observable behaviour against the native result and CPython oracle.

## Non-goals for the first change

- No immediate SSA rewrite.
- No LLVM migration.
- No claim of browser, AWS Lambda, IoT, or embedded support.
- No broad language expansion.
- No optimizer performance claims without reproducible benchmarks.
- No changes to existing Python semantics merely to simplify a backend.

## Acceptance criteria

The backend-boundary extraction is acceptable only when:

1. Existing native output and behaviour remain compatible for the differential
   test corpus.
2. The native backend is selected through the new contract, not a direct
   hard-coded call in the orchestration path.
3. Target selection is validated and unsupported targets fail clearly.
4. Backend-independent crates do not acquire dependencies on a concrete
   backend.
5. CI covers the new contract and preserves all existing gates.

## Risks and mitigations

- **Premature abstraction:** Keep the first interface small and driven by the
  actual Rust backend requirements.
- **False portability:** Require explicit target/runtime capability checks.
- **Semantic drift:** Keep native differential testing as a release gate.
- **IR redesign too early:** Evaluate the structured IR against concrete WASM
  lowering needs before introducing CFG/SSA. Add those representations when a
  measured requirement justifies them.
- **Runtime coupling:** Inventory generated runtime helpers and host APIs before
  defining the portable runtime ABI.

## Open technical questions

1. Which runtime helpers are emitted by Rust codegen, and which rely on host
   facilities such as filesystem, process, clocks, or environment variables?
2. Which IR constructs can be lowered directly to WASM, and which require a
   portable runtime operation?
3. What is the minimum supported WASI profile and runtime matrix?
4. How should target capabilities be derived from language features and
   runtime imports?
5. Which existing tests are semantic tests, code-shape tests, or full native
   execution tests?

## Source files inspected

- `crates/tarvos-core/src/lib.rs`
- `crates/tarvos-ir/src/lib.rs`
- `crates/tarvos-optimizer/src/lib.rs`
- `crates/tarvos-codegen-rust/Cargo.toml`
- `.github/workflows/ci.yml`
- `Cargo.toml`
- `README.md`


## Addendum: existing embedded path discovered during follow-up audit

The CLI already exposes `tarvos compile --target embedded`, and
`CompilePipeline::transpile_file_embedded` calls
`RustCodegen::generate_embedded`. This is a real existing implementation path,
so the project should not describe embedded support as wholly absent.

The inspected code generator describes this path as a deliberately small,
freestanding `no_std` entry point for integer programs. The implementation
accepts a narrow set of integer bindings/expressions and emits
`tarvos_entry() -> i64`. It rejects printing, reassignment, and unsupported
statements. Its own comment explains that the normal backend remains
`std`-based because Python printing, collections, timing, and exception
handling need it.

This is a useful prototype, but it is not yet evidence of a general MCU/IoT
toolchain. It does not by itself establish target-triple support, linker and
memory-layout integration, startup/runtime support, peripheral access, device
flashing, or hardware-in-the-loop tests. The next embedded audit should inspect
the full code-generation tests and actual target build/link path before making
any product claim.
