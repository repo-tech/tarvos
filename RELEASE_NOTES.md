# Tarvos 1.1.0-rc.4

## Summary

`1.1.0-rc.4` is a release candidate whose headline is that **exceptions and the
`statistics` module are now genuinely native**. In `1.1.0-rc.3` a Python program
containing `try` was rejected as a dynamic construct and silently fell back to the
Python compatibility launcher, and `import statistics` did not resolve at all. In
this release both compile to real Rust and run as native executables with no
Python present.

This is a **pre-release**. The supported-subset contract has not widened beyond
what is listed under [Python Compatibility](#python-compatibility); read that
section before assuming a feature is available.

## Highlights

- Native `try` / `except` / `else` / `finally` and `raise`, with real exception
  propagation out of functions and through nested `try` blocks.
- `except` matches the real Python exception hierarchy, so `except ValueError`
  catches a `StatisticsError`, which is a `ValueError` subclass in CPython.
- `statistics.StatisticsError` is catchable rather than a fatal abort.
- The `statistics` module is complete for its documented surface: `mean`,
  `fmean`, `geometric_mean`, `harmonic_mean`, `median`, `median_low`,
  `median_high`, `median_grouped`, `mode`, `multimode`, `quantiles`, `pvariance`,
  `pstdev`, `variance`, `stdev`, `covariance`, `correlation`, and
  `linear_regression`.
- Correctness fixes found by differential testing rather than assumed, including
  a silent drop of keyword arguments that produced wrong native binaries.

## Compiler

### Exceptions are `Result` values, not panics

The previous lowering wrapped a `try` body in `std::panic::catch_unwind`. That was
wrong in three independent ways: a panic cannot carry a Python exception class, it
cannot run `finally` on a non-local exit, and under the `panic = "abort"` release
profile it does not catch at all. Exceptions are now ordinary `Result` values
carrying a class name and a message.

A `try` lowers to a labelled block rather than a closure. A closure was rejected
because its `let` bindings leave scope, so a variable assigned inside the `try`
would have been missing after it.

`return` inside a `try` is deferred into a slot and the real `return` is emitted
after `finally`, matching Python. A function that can raise returns a `Result` and
its call sites unwrap it, so an error raised in a function reaches the caller's
`try` instead of terminating at the point of the raise.

Previously only the *first* handler was ever emitted and the bound name was the
literal string `"Tarvos exception"`. Handlers are now matched in order against
the real class name.

## Python Compatibility

The supported Python subset is unchanged in shape and is documented
machine-readably in `docs/COMPATIBILITY.md`, which is generated and checked by CI.
At this tag: **95 features — 52 supported, 18 partial, 24 unsupported, 1 planned**.

Known limitations that are deliberately not papered over:

- `quantiles` supports only the default `n=4, method="exclusive"`. CPython
  declares both keyword-only, and keyword arguments are not lowered natively yet.
- `linear_regression` returns `(slope, intercept)` rather than a
  `LinearRegression` named tuple, so `result.slope` is not available.
- An uncaught native exception prints `Class: message` on stderr and exits 1. It
  does not print a Python traceback, because a native binary does not carry the
  source-level frame information a traceback needs.
- `break` and `continue` inside a `try` inside a loop are reported rather than
  lowered, because a plain Rust `break` would skip `finally`.
- `random`, HTTP/`requests`, `re`, `datetime`, and the wider standard library are
  not implemented. They are marked unsupported, not stubbed.

## Correctness

Fixed in this release, each found by comparing against CPython rather than by
inspection:

- Keyword arguments were silently discarded by the front end, so `f(x, n=2)`
  compiled as `f(x)` and produced a native binary that quietly computed something
  else. They are now reported and the program takes the explicit compatibility
  path instead of producing a wrong answer.
- `harmonic_mean` returned an error for a zero input. CPython returns `0`.
- `mean`, `mode`, `median`, `median_low`, and `median_high` now preserve CPython's
  int-versus-float result kind, so `mode([1, 2, 2, 3])` is the int `2`, not `2.0`.
- `geometric_mean` reduces through logarithms. The n-th-root-of-product form gave
  `3.9999999999999996` where CPython gives exactly `4.0`.
- `median_grouped` follows the CPython 3.13 algorithm. The older "nudge the two
  central values" formulation disagrees whenever the median value repeats.
- `quantiles` reproduces CPython's exact integer rescaling, which deliberately
  extrapolates outside the observed range: `quantiles([1.0, 2.0])` is
  `[0.75, 1.5, 2.25]`.

## CI

- The Rust toolchain is pinned to `1.98.0` instead of floating on `stable`, which
  could turn every commit red on a toolchain release with no source change.
- Cargo and `target/` are cached with first-party `actions/cache`; there was no
  caching at all before.
- The Rust gates run through `scripts/ci_gates.py`, the same entry point used
  locally, so a local pass and a CI pass cannot drift apart.
- `scripts/check_toolchain_pin.py` fails if the toolchain is declared
  inconsistently anywhere, or starts floating again.
- Job names are static, because GitHub renders a `${{ matrix.os }}` expression in
  `name` literally and produces an unreadable failure notification.
## Breaking Changes

None. No supported construct changed behaviour in a way that requires source
changes, and the supported-subset contract is unchanged from `1.1.0-rc.3`.

Two behaviours that were previously *wrong* are now corrected, and a program that
relied on the wrong answer would change:

- Keyword arguments are no longer silently discarded. A call such as
  `f(x, n=2)` used to compile as `f(x)`. Such a program now takes the explicit
  compatibility path instead of producing a native binary that quietly computed
  something else.
- An uncaught exception is now reported as `Class: message` on stderr with exit
  code 1. It previously aborted through the runtime's panic path.

## Installation

Full platform coverage: https://github.com/repo-tech/tarvos/releases

From a source checkout:

```bash
cargo build --release -p tarvos-cli
```

The Windows installer is built by the release workflow from the tagged commit and
attached to the release. Linux and macOS artifacts are published by the same
workflow; if a platform artifact is missing from a release, that platform was not
built and its absence is recorded here rather than implied to be supported.


- A failing gate emits a workflow annotation, which is visible to anyone who can
  read the repository, instead of only appearing in a log that requires write
Platform status: Windows x86_64 verified locally. Linux and macOS are exercised by
the CI matrix; their result for this tag is the CI run, not this document.

Full changelog: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.3...v1.1.0-rc.4


  access to fetch.

## Validation

Run against this tag on Windows x86_64:

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo check --workspace --all-targets` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace --no-fail-fast` | 116 passed, 0 failed |
| Differential suite (CPython vs native) | 14/14 |
| CLI audit | 20/20 |
| Project acceptance | 6/6 |
| Compatibility matrix check | current |
| Generated Rust audit | 0 `unsafe`, 0 `transmute`, 0 `catch_unwind`, 0 `panic!` |

Platform status: Windows x86_64 verified locally. Linux and macOS are exercised by
the CI matrix; their result for this tag is the CI run, not this document.

---



# Tarvos 1.1.0-rc.3

## Summary

`1.1.0-rc.3` is a stabilization release candidate on the 1.1.0 line. It extends
the natively compiled Python subset with the `list()` and `sorted()` builtins,
closes the front-end gaps that used to force the CPython fallback, and removes a
class of stale-cache bugs. It also makes the release itself reproducible: one
workspace version drives every crate, the binary, the installers, and the
packaging metadata, and CI fails if any of them drift.

This is a **pre-release**. It is not the stable 1.1.0 line: the supported-subset
contract is unchanged from `1.1.0-rc.2`, so nothing in this release requires
rewriting code, but the artifacts are still release candidates.

The gates listed under [Validation](#validation) are the ones actually run
against the tagged commit `cb1c333`, including the ones that did not pass.

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
  (`scripts/check_version_consistency.py`). The Cargo workspace version is the
  single source of truth: all twelve member crates inherit
  `version.workspace = true`.
- This release page is generated from one section of `RELEASE_NOTES.md` by
  `scripts/extract_release_notes.py` and published by the release workflow.
  GitHub's auto-generated notes are deliberately disabled, and the extractor
  refuses to emit more than one comparison link, because auto-generation
  previously appended a second comparison link to the hand-written body.
- The release workflow verifies before it publishes: the release tag must exist,
  must be an ancestor of `main`, must match the workspace version, and must have
  a non-empty body.
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
- Full dynamic Python semantics (reflection, dynamic dispatch, monkey patching)
  are outside the native subset by design; Tarvos is not a drop-in CPython
  replacement.
- Tarvos only publishes Windows x86_64 binaries and the Windows installer. Linux
  and macOS artifacts are built by the release workflow on their own runners and
  are distributed from `repo-tech/tarvos-engine`; on those platforms `install.sh`
  builds the CLI from source.
- The tagged commit has one known non-functional defect: a single
  `rustfmt`-only line-wrap deviation in `crates/tarvos-optimizer/src/lib.rs`
  (see [Validation](#validation)). It is corrected on `main` after the tag.

## Validation

Every result below was produced against the tagged commit `cb1c333`, not against
a later working tree.

| Gate | Result |
| --- | --- |
| `cargo check --workspace --all-targets` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace --no-fail-fast` | PASS — 25 suites, 110 tests, 0 failed |
| `cargo build --workspace --release` | PASS |
| `cargo fmt --all -- --check` | **FAIL** — see below |
| `python scripts/check_version_consistency.py --require-tag v1.1.0-rc.3` | PASS — 12/12 declarations plus the tag |
| `python scripts/diff_test.py` (release binary from the tag) | PASS — 13/13 workloads |
| `tarvos --version` | `tarvos 1.1.0-rc.3` |
| `crates/tarvos-cli/tests/compatibility_gate.rs` | PASS — native path, fallback path, range step semantics, string/list methods, matrix kernel, `list()`/`sorted()`, dict-order refusal |
| GitHub Actions `Production CI Pipeline` (`cb1c333`) | PASS — five cross-target builds, workspace tests, production validation suite |
| GitHub Actions `Tarvos 1.1.0-rc.3 release validation` (`cb1c333`) | PASS — benchmark matrix plus a compile/build smoke test asserting the program output |

The `cargo fmt` failure is a single line-wrap deviation in
`crates/tarvos-optimizer/src/lib.rs` that the Clippy cleanup in the same commit
introduced. It changes no behaviour, it is not covered by the CI gates that ran
for this tag, and it is fixed on `main` after the tag, where `cargo fmt --all --
--check` passes. The tag was deliberately not moved, so the shipped binary is
built from exactly the commit the tag names.

Not run for this commit: the Windows installer build and install (Inno Setup,
produced by the release workflow), Linux/macOS artifact builds, and a fresh
timing benchmark.

## Release Artifacts

Release notes and the Windows CLI live in this repository; the cross-platform
binaries and the installer live in the distribution repository.

Attached to this release in `repo-tech/tarvos`:

- `tarvos-windows-x86_64.exe` — CLI built from the tagged commit `cb1c333` with
  `cargo build --release`; `tarvos --version` reports `1.1.0-rc.3`.
  SHA-256: `e5dfff0eb04e827122db4bd966ade56bb005b5d6b5e2d7088c92c9b8733a4208`.
- `tarvos-windows-x86_64.exe.sha256` — checksum file for the binary above, in
  `sha256sum` format, verified by `install.ps1` before installation.

Published to `repo-tech/tarvos-engine` by `.github/workflows/release.yml` on
their own runners: `tarvos-linux-x86_64`, `tarvos-macos-x86_64`,
`tarvos-windows-x86_64.exe`, and `Tarvos-Setup-Windows-x86_64.exe`, each with a
`.sha256` companion. That repository holds the v1.0.0, v1.1.0-rc.1, and
v1.1.0-rc.2 artifact history.

## Installation

- Windows: `install.ps1 -Version v1.1.0-rc.3` downloads
  `tarvos-windows-x86_64.exe` from this repository's release, verifies its
  SHA-256, and installs it to `%USERPROFILE%\.tarvos\bin\tarvos.exe` without
  administrator rights. Pass `-Repository repo-tech/tarvos-engine` to install
  from the distribution repository instead.
- Linux/macOS: `./install.sh`, which builds and installs the CLI from source
  with `cargo install --locked --path crates/tarvos-cli`, or download the
  platform binary from `repo-tech/tarvos-engine`.
- From source: `cargo build --workspace --release`, then
  `target/release/tarvos --version` (prints `1.1.0-rc.3`).

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
