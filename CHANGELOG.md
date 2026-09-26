# Changelog

All notable changes to Tarvos are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

Nothing yet. Work merged after the 1.1.0-rc.3 release candidate is recorded here.

## [1.1.0-rc.3] - release candidate

Stabilization release on the 1.1.0 line. It extends the native subset with the
`list()`/`sorted()` builtins, closes the cross-front-end gaps that forced the
CPython fallback, and removes a class of stale-cache bugs. No new version was
declared in the binary until this release: `tarvos --version` now reports
`1.1.0-rc.3`, and the whole release metadata set is verified by
`scripts/check_version_consistency.py`.

### Native Compilation

- `list()` and `sorted()` are compiled natively with preserved element types
  for `range()`, `str`, list, homogeneous tuple, and dict keys (`sorted`).
  Lowering tags the source kind (`__tarvos_list_from_range`,
  `__tarvos_sorted_from_str`, ...) so the emitter never re-derives a bare
  name's type; a homogeneous tuple is desugared to element reads and an empty
  `list()` emits a typed vector instead of `Vec<()>`.
- `os` subset (`getcwd`, `listdir`, `mkdir`, `makedirs`, `chdir`) and static
  `json.dumps` for compile-time literals.
- Bitwise (`&`, `|`, `^`, `~`), shift (`<<`, `>>`), floor division (`//`), and
  power operators lower natively on both front ends, including `augmented`
  forms.

### Python Compatibility

- Chained comparisons (`a <= b <= c`) desugar to short-circuiting `and`, so a
  middle operand is evaluated once.
- Tuple assignment, nested subscript assignment (`grid[i][j]`, `grid[i][j] +=`),
  list comprehensions over strings, and boolean/comparison/set/dict literals
  bridge from the Ruff front end.
- String methods (`lower`, `upper`, `title`, `strip`, `split`, `join`,
  `replace`, `zfill`, ...), list methods (`append`, `extend`, `sort`, `pop`,
  ...), and dict methods (`get`, `update`, `keys`, `values`, `items`) dispatch
  on the receiver's static type through one registry shared by both front ends.
- `print` reproduces Python's `str`/`repr` split, and iteration follows Python
  semantics for `str` (by character) and `dict` (by key).

### Compiler

- Shared AST normalization pass in `tarvos-core` used by both front ends, so a
  new rewrite is implemented once instead of twice.
- IR widening: `Destructure`, chained `IndexAssign`, `Invert`, `BitXor`,
  `BitAnd`, `BitOr`, `LShift`, `RShift`, `FloorDiv`.
- Type inference improvements: variable type collection before lowering,
  iterable element types, empty-list element hints from the first `append`,
  comprehension element types.

### Fixed

- Stale native translation cache: the cache epoch now hashes the CLI
  executable's contents as well as its version/size/mtime, so a rebuilt binary
  can never reuse a translation or an `unsupported` verdict from a previous
  build. Previously a fast relink could preserve size and mtime granularity and
  silently reuse stale generated Rust.
- `list()` over a dict is rejected with an actionable diagnostic instead of
  emitting an order-dependent result (see Known limitations).

### Tooling

- `list()` over a dict, unknown element types, heterogeneous tuples, and
  keyword arguments produce explicit `--python-fallback` diagnostics rather
  than a `Vec<()>` type error.
- Benchmark workloads for matrix multiplication and a bitwise/LCG kernel with
  recorded expected outputs.
- `scripts/check_version_consistency.py` fails the build when any release
  metadata drifts from the Cargo workspace version; the workspace version is
  now the single source of truth (`version.workspace = true`).

### AI / Ollama

- `tarvos ai-status` (plus a `doctor` line and an installer post-install step)
  records the optional local AI capability in
  `~/.tarvos/ai-capability.json`. The probe is read-only, loopback-only, and
  timeout-bounded: it never starts, stops, downloads, or configures anything,
  and only a `TarvosProvisioned` ownership is sticky. Compilation works
  without any local AI capability.

### Security

- `SECURITY.md` replaced the unfilled GitHub template (including a fabricated
  version table) with the real supported-version policy and private advisory
  reporting. No secrets are uploaded by the CLI, and the sandbox/gateway
  boundary is unchanged.

### Validation

- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`, `cargo build --workspace --release`,
  `python scripts/check_version_consistency.py`, and the native-vs-CPython
  differential gate in `crates/tarvos-cli/tests/compatibility_gate.rs`.

### Known limitations

- `list()` over a dict refuses native compilation because `HashMap` iteration
  order differs from Python insertion order; `sorted(dict)` is supported
  because sorting is deterministic.
- `set`, `enumerate`, `zip`, dynamic `json.loads`, arbitrary third-party
  imports, and dynamic JSON values still require `--python-fallback`.
- Cross-platform release artifacts and the Windows installer are produced by
  the release workflow on each platform's runner; the installer is not built
  or validated on a developer machine without Inno Setup.

## [1.1.0-rc.2] - release candidate

### Added

- Cross-target CI that separates native-host test execution from
  cross-target artifact builds.
- Type-preserving native lowering for branches, loops, classes, lists,
  strings, and dictionaries with explicit diagnostics for incompatible
  reassignment.

### Changed

- Hardened the native compatibility boundary after the 1.0.0 baseline;
  see `RELEASE_NOTES.md` for the full release-candidate notes.

## [1.1.0-rc.1]

- Hardened native Python compatibility (native lowering, compatibility
  gate, actionable fallback diagnostics).

## [1.0.0] - first stable release

### Added

- Native AST export, typed lowering, IR optimization, and Rust code
  generation.
- Standalone native executable generation for the supported Python subset.
- Explicit compatibility diagnostics for unsupported dynamic Python
  features.
- Production gateway, health endpoint, sandbox configuration, and
  Docker/Render deployment assets.

### Compatibility boundary

Tarvos is not a drop-in CPython replacement; see `docs/compiler-status.md`
for the exact supported subset.

[Unreleased]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.3...HEAD
[1.1.0-rc.3]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.2...v1.1.0-rc.3
[1.1.0-rc.2]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.1...v1.1.0-rc.2
[1.1.0-rc.1]: https://github.com/repo-tech/tarvos/compare/v1.0.0...v1.1.0-rc.1
[1.0.0]: https://github.com/repo-tech/tarvos/releases/tag/v1.0.0