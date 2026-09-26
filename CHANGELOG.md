# Changelog

All notable changes to Tarvos are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Native `list()` and `sorted()` builtins with preserved element types
  (`range`, `str`, list, homogeneous tuple, dict keys for `sorted`), so
  these no longer force the CPython fallback.
- Read-only, loopback-only optional local AI capability probe
  (`tarvos ai-status`, `doctor` integration, installer post-install probe)
  that never starts, downloads, or stops any process.
- Native `os` subset (`getcwd`, `listdir`, `mkdir`, `makedirs`, `chdir`)
  and static `json.dumps`.
- Shared AST normalization pass used by both front ends (subscript-swap
  desugaring with collision-safe temporaries).
- Ruff-based reference front end coverage: tuple/subscript assignment,
  chained comparisons, boolean/comparison/set/dict literals, bitwise and
  floor-division operators.
- Benchmark workloads for matrix multiplication and bitwise/LCG kernels
  with recorded expected outputs.

### Fixed

- Stale native translation cache: the cache epoch now hashes the CLI
  executable contents, so a rebuilt binary can never reuse a translation
  or an `unsupported` verdict produced by a previous build.

### Known limitations

- `list()` over a dict refuses native compilation (HashMap iteration order
  differs from Python insertion order); `sorted(dict)` is supported.
- `set`, `enumerate`, `zip`, dynamic `json.loads`, and arbitrary
  third-party imports still require `--python-fallback`.

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

[Unreleased]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.2...HEAD
[1.1.0-rc.2]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.1...v1.1.0-rc.2
[1.1.0-rc.1]: https://github.com/repo-tech/tarvos/compare/v1.0.0...v1.1.0-rc.1
[1.0.0]: https://github.com/repo-tech/tarvos/releases/tag/v1.0.0