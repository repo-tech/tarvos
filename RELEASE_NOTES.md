# Tarvos 1.1.0-rc.3

Stabilization release candidate on the 1.1.0 line. It adds the native
`list()`/`sorted()` builtins, closes the front-end gaps that forced the CPython
fallback, and removes a class of stale-cache bugs. This is a **pre-release**:
the installer and cross-platform artifacts are produced by the release
workflow, and the gates listed under *Validation* are the ones actually run for
this commit.

## Highlights

- `list()` and `sorted()` compile natively with preserved element types
  (`range`, `str`, list, homogeneous tuple, dict keys for `sorted`).
- Explicit, actionable fallback instead of wrong native output: `list()` over a
  dict refuses native compilation because `HashMap` iteration order differs
  from Python insertion order.
- Rebuilt binaries can no longer reuse stale cached translations (the cache
  epoch now hashes the executable's contents).
- One version source of truth: every crate inherits the workspace version, and
  `scripts/check_version_consistency.py` gates the rest of the release metadata.

## Compiler

- Shared AST normalization in `tarvos-core` runs on both front ends, so a
  desugaring is written once (subscript-swap temporaries, collision-safe
  `__tarvos_tmp`).
- IR widening: `Destructure`, chained `IndexAssign`, `Invert`, `BitXor`,
  `BitAnd`, `BitOr`, `LShift`, `RShift`, `FloorDiv`.
- Type inference collects variable types before lowering, resolves iterable
  element types, infers empty-list element types from the first `append`, and
  fixes comprehension element types.
- The Ruff-based reference front end (`tarvos-ruff`) bridges tuple/subscript
  assignment, chained comparisons, boolean/comparison/set/dict literals, and
  the bitwise/floor-division operators; the CPython AST exporter covers the
  same operator set.

## Python Compatibility

**Native**

- `list()` over `range()`, `str`, list, and homogeneous tuple; `sorted()` over
  the same plus dict keys.
- `str` methods (`lower`, `upper`, `title`, `capitalize`, `swapcase`, `strip`,
  `lstrip`, `rstrip`, `ljust`, `rjust`, `zfill`, `center`, `replace`, `join`,
  `split`, `splitlines`, `startswith`, `endswith`, `is*`, `count`, `find`,
  `rfind`, `index`), `list` methods (`append`, `extend`, `insert`, `remove`,
  `pop`, `clear`, `sort`, `reverse`, `index`, `count`), `dict` methods
  (`get`, `update`, `setdefault`, `pop`, `keys`, `values`, `items`).
- Bitwise, shift, floor-division, and power operators including augmented
  forms; chained comparisons; nested subscript reads/writes; list
  comprehensions over strings; dictionary literals with scalar keys.
- Selected `os` (`getcwd`, `listdir`, `mkdir`, `makedirs`, `chdir`),
  `os.path`, `math`, `time`, and static `json.dumps`.

**Fallback (`--python-fallback`)**

- `list()` over a dict, heterogeneous tuple element types, unknown element
  types, `set`, `enumerate`, `zip`, dynamic `json.loads`/dynamic JSON values,
  and arbitrary third-party imports.

**Unsupported**

- Full dynamic Python semantics (reflection, dynamic dispatch, monkey
  patching) remain outside the native subset by design.

## Native Compilation

- `list(x)` emits a clone for an existing vector, an iterator collect for a
  lazy `range()`, a character split for a `str`, and key clones for a dict.
- `sorted(x)` materializes then sorts in place, using `total_cmp` for `f64`
  (which is not `Ord`) and the element-typed helper for `i64`/`String`/`bool`.
- Empty `list()` emits a typed vector so the display helper resolves without
  guessing the element type.

## Correctness

- Chained comparisons evaluate a middle operand once and short-circuit like
  Python.
- `print` keeps Python's `str`/`repr` split; `str` iterates by character and
  `dict` by key.
- Rejecting `list(dict)` prevents emitting an order-dependent result that
  would silently differ from CPython.
- The cache epoch fix prevents a stale generated program from being executed
  after a compiler change.

## Tooling

- Version drift between the workspace, CLI, installer, launcher, README,
  gateway banner, workflow defaults and Python packaging is now a CI failure
  (`scripts/check_version_consistency.py`).
- New benchmark workloads: `matrix_multiply.py`, `bitwise_xorshift.py` with
  recorded expected outputs.
- `SECURITY.md`, `CONTRIBUTING.md`, `SUPPORT.md`, `CHANGELOG.md`, issue
  templates and a PR template were added; `SECURITY.md` previously contained
  an unfilled placeholder with a fabricated version table.

## AI / Ollama

Implemented: read-only capability detection (`tarvos ai-status`), a `doctor`
line, an installer post-install probe, and `~/.tarvos/ai-capability.json`
recording with `TarvosProvisioned`/`External`/`Absent` ownership.

Not implemented in this release: automatic provisioning/installation, model
preparation, inference integration, and Tarvos-owned runtime lifecycle. The
probe is loopback-only and timeout-bounded and never starts, stops, downloads,
or configures anything. Compilation never depends on it.

## Performance

Not measured for this release candidate. The repository ships benchmark
workloads and a differential harness, but no new timing claim is made here
because no reproducible measurement was run for this commit. Previous
`benchmarks/results/*.json` files record older builds and must not be read as
results for 1.1.0-rc.3.

## Security

- `SECURITY.md` now documents supported versions, private reporting through
  GitHub Security Advisories, and the sandbox/threat boundary.
- No source code, telemetry, or secrets are uploaded by the CLI.
- Uploaded code is never executed in the gateway process; the terminal runs
  only inside the constrained Docker sandbox (non-root, `--network=none`,
  dropped capabilities, memory/CPU/pids limits).

## Breaking Changes

None. `1.1.0-rc.3` is a release candidate on the 1.1.0 line: the CLI surface,
project layout, and supported-subset contract are unchanged from
`1.1.0-rc.2`. The changes are additive (more constructs compile natively) or
stricter (an order-dependent construct now requests the fallback instead of
compiling).

## Known Limitations

- `list()` over a dict requires `sorted()` or `--python-fallback`.
- `set`, `enumerate`, `zip`, dynamic JSON, and third-party imports require
  `--python-fallback`.
- Cross-platform binaries and the Windows installer are built by the release
  workflow on their own runners; they are not produced by this local
  validation run.

## Validation

Run against this release commit:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --no-fail-fast`
- `cargo build --workspace --release`
- `python scripts/check_version_consistency.py`
- `python scripts/diff_test.py` (native vs CPython output parity)
- `crates/tarvos-cli/tests/compatibility_gate.rs` (native path, fallback path,
  range step semantics, string/list methods, matrix kernel, `list()`/`sorted()`
  builtins, dict-order refusal)

Not run for this commit: Windows installer build/install (requires Inno Setup
and is produced by the release workflow), macOS/Linux artifacts, and a fresh
timing benchmark.

## Installation

- Windows: `install.ps1 -Version v1.1.0-rc.3` (user-local, no administrator
  rights) or the release artifacts from the GitHub release.
- Linux/macOS: `install.sh` or the platform binary attached to the release.
- From source: `cargo build --workspace --release` then
  `target/release/tarvos --version` (prints `1.1.0-rc.3`).

## Release Artifacts

Attached to the GitHub pre-release for this candidate:

- `tarvos-windows-x86_64-msvc.exe` — CLI built from this commit with
  `cargo build --release`.
- `tarvos-windows-x86_64-msvc.exe.sha256` — SHA-256 checksum of that binary.

Produced by `.github/workflows/release.yml` on demand and **not** part of this
local run: Linux/macOS binaries and `Tarvos-Setup-Windows-x86_64.exe`
(installer).

## Full Changelog

https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.2...v1.1.0-rc.3

# Tarvos 1.1.0-rc.2

This release candidate fixes cross-target CI by separating native-host test
execution from cross-target artifact builds. It also synchronizes the public
Tarvos Engine distribution milestone.

This release candidate hardens the native compatibility boundary after the
1.0.0 baseline.

## Highlights

- Type-preserving native lowering for branches, loops, classes, lists, strings,
  and dictionaries, with explicit diagnostics for incompatible reassignment.
- Native imports for selected local modules and standard-library operations,
  plus a CLI compatibility gate covering strict native and CPython fallback.
- Large-integer transport without floating-point rounding and actionable
  diagnostics for native `range()` limits.
- Expanded optimizer, vectorization analysis, benchmark fixtures, and
  workspace regression coverage.
- Ruff parser/compiler prototype coverage for tuple assignment, list
  repetition, range loops, dynamic values, and safe unsupported-node
  diagnostics. The production CLI still uses its established AST exporter.

## Compatibility boundary

This is not a full CPython replacement. Arbitrary dynamic Python, TensorFlow,
PyTorch, unrestricted NumPy/Pandas usage, and unsupported third-party imports
still require explicit fallback or remain outside the native subset.

## Validation

The release candidate passes the workspace tests, 27 production pipeline tests,
3 CLI compatibility tests, Ruff prototype tests, formatting checks, Python
exporter syntax checks, and diff hygiene checks.

# Tarvos 1.0.0

Tarvos 1.0.0 is the initial stable product release of the Tarvos
Python-to-native Rust compiler.

## Highlights

- Native AST export, typed lowering, IR optimization, and Rust code generation.
- Standalone native executable generation for the supported Python subset.
- Explicit compatibility diagnostics for unsupported dynamic Python features.
- Native f-string, control-flow, basic exception, collection, and arithmetic
  support where statically analyzable.
- Conservative vectorization analysis with deterministic scalar fallback.
- Optional embedded target emitting strict `#![no_std]` arithmetic Rust with a
  custom panic handler.
- Size-oriented release profile using `opt-level = "z"`, LTO, one codegen unit,
  aborting panics, and symbol stripping.
- Production gateway, health endpoint, sandbox configuration, and Render/Docker
  deployment assets.

## Compatibility boundary

Tarvos is not a drop-in replacement for CPython. GUI frameworks, arbitrary
third-party imports, reflection, dynamic dispatch, and runtime-heavy APIs
remain outside the native subset and must be rejected or handled explicitly.

## Validation

The 1.0.0 baseline passes the workspace test suite, compiler capability matrix,
native parity fixtures, and release build validation.
