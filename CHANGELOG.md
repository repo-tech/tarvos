# Changelog

All notable changes to Tarvos are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

Release-engineering follow-up to the `v1.1.0-rc.3` tag. No product behaviour
changes; the tag was not moved.

### Fixed

- The release workflow checked out the default branch instead of the release
  tag, so a `workflow_dispatch` publish would have built `main` HEAD and
  uploaded it under the release tag. The build jobs now check out
  `inputs.release_tag || github.ref`.
- The workflow published release assets only to the distribution repository
  `repo-tech/tarvos-engine`, so the compiler repository never received a
  GitHub release for its own tag, and `install.ps1` (which downloads
  `tarvos-windows-x86_64.exe` from this repository) had nothing to install.
  A `publish-source-release` job now creates or updates the release here,
  refuses to run unless the tag already exists, and titles it `Tarvos <tag>`.
- `scripts/extract_release_notes.py` now rejects a section that would emit more
  than one comparison link, so a duplicated "Full Changelog" block cannot be
  published again.
- `scripts/extract_release_notes.py` writes UTF-8 bytes instead of text, so
  generating a release body on Windows no longer emits console-code-page bytes
  that mangle the notes.
- The release-validation workflow is no longer named after a specific release
  candidate. Its title was a version site that had to be edited on every
  release, so it is now version neutral and the check was removed from
  `scripts/check_version_consistency.py` (13 sites remain, all still gated).
- `cargo fmt` is now a CI gate (`formatting` job), fixing the single
  line-wrap deviation the `v1.1.0-rc.3` Clippy cleanup introduced.
- The obsolete `v1.2.0` draft release in this repository, which referenced no
  existing tag and carried a duplicated "Full Changelog" body, was deleted.
  The `v1.1.0-rc.3` release was published in its place, carrying the notes from
  `RELEASE_NOTES.md` and the Windows CLI built from the tagged commit.

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

- `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast` (25 suites, 110 tests),
  `cargo build --workspace --release`, `python
  scripts/check_version_consistency.py --require-tag v1.1.0-rc.3`, and the
  native-vs-CPython differential gate in
  `crates/tarvos-cli/tests/compatibility_gate.rs` all pass against the tagged
  commit `cb1c333`.
- `python scripts/diff_test.py` reports 13/13 workloads for the release binary
  built from that commit.
- One gate does not pass: `cargo fmt --all -- --check` reports a single
  line-wrap deviation in `crates/tarvos-optimizer/src/lib.rs` introduced by the
  Clippy cleanup in the same commit. It is behaviour-neutral, the tag was not
  moved, and the deviation is fixed on `main`, where formatting is now a CI
  gate.

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