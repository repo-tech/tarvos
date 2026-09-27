# Changelog

All notable changes to Tarvos are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

CI/CD and release-pipeline work. No compiler behaviour changes. The
`v1.1.0-rc.3` tag was not moved and its published release description was not
rewritten.

### Changed - workflow responsibilities

- `ci.yml` is now the only "is the source healthy?" workflow. It runs
  `fmt`, `check`, `clippy`, and `cargo test --no-fail-fast` on a single Linux
  runner, on pull requests and pushes to `main`. It no longer runs on tags and
  no longer builds release artifacts.
- Cross-platform verification moved to a `cross-target` job that runs on demand
  and on a weekly schedule instead of on every commit. A normal commit no longer
  pays for five cross-target builds.
- `.github/workflows/release-ci.yml` was deleted. Its release-grade validation
  (workspace tests, the production validation suite, the compile/build smoke
  test) moved into `release.yml` as the `validate` job, and its benchmark
  matrix became the non-gating `benchmarks` job. Previously the same tests ran
  on every push to `main` while releases skipped them.
- `release.yml` is the only workflow that may create or edit a release. It runs
  on a pushed `v*` tag or a manual dispatch that names an existing tag, never
  on a branch push.

### Fixed - release safety

- The release gate checked out the default branch, so a `workflow_dispatch`
  publish validated the metadata of `main` instead of the commit being
  released. Every job in the pipeline now checks out the release tag.
- The pipeline asserted that the tag exists and matches the version, but never
  that the checked-out tree was the tagged commit. It now fails with
  `Wrong source commit` when they differ.
- A blank, `main`, or malformed `release_tag` is refused with a specific error
  instead of resolving to a branch. The dispatch input no longer carries a
  hard-coded version default, which was a footgun that republished a stale
  release.
- Release binaries were built with `-C target-cpu=native`, producing a binary
  tuned to the runner's CPU that can fault on older hardware. Release builds are
  now portable; size and stripping are unchanged.
- The built CLI's reported version is checked against the release tag, and each
  release asset is required to exist together with its checksum before anything
  is published.
- A leftover draft release for the tag is refused rather than silently edited.
- `fail_on_unmatched_files` and `overwrite_files` were set on the distribution
  upload, so a re-run replaces assets by name instead of accumulating
  duplicates.
- `build` depended only on `verify`, so it ran in parallel with `validate`.
  Binaries and a published release could therefore be produced from a tree whose
  tests, validation suite, smoke build, and CPython differential had all failed.
  `build` and both publication jobs now require `validate` to succeed.
- The distribution job checked that each `.sha256` file existed but never
  recomputed it, so a stale checksum, or a binary corrupted in artifact
  transfer, would still have been published. Every digest is now recomputed and
  compared before upload.
- `extract_release_notes.py` required only a minimum length. The section for the
  current workspace version must now also carry `## Highlights`,
  `## Breaking Changes`, `## Validation`, and `## Installation`, and every
  section may contain at most one comparison link. The structural rule applies
  only to the release being cut, so re-running an older tag still works and
  published history is not re-validated.

### Changed - permissions, concurrency, and reporting

- `ci.yml` declares `permissions: contents: read` explicitly, as does
  `release.yml` at workflow level. Only the `publish-source-release` job holds
  `contents: write`.
- Release concurrency is keyed on the release tag, so two different tags do not
  block each other, and `cancel-in-progress` stays false: a publication is never
  cancelled by a newer run.
- CI cancels only superseded pull request runs, never push, schedule, or manual
  runs.
- Guard failures emit `::error title=...` annotations, and each job prints the
  tag, commit, target, and version it is working on, so a failure identifies
  itself instead of exiting silently. No step suppresses a failure with
  `|| true`.
- `check_version_consistency.py` now runs in exactly one place, the release
  gate, against the tagged commit. The two workflow-internal version sites were
  removed, leaving 12 version sites that are all shipped metadata.

### Known limitation

- The `repo-tech/tarvos-engine` release for `v1.1.0-rc.3` is still
  outstanding. It depends on GitHub Actions runners, and every job on this
  account currently fails before its first step with no log. The compiler source
  release in `repo-tech/tarvos` is unaffected and complete.

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