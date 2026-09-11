# Tarvos 0.0.1 Release Checklist

Status: ready for a narrowly scoped release as a subset compiler for static numeric Python workloads.

## Product definition

- [ ] Release is explicitly described as a Python-to-Rust subset compiler, not a full Python runtime.
- [ ] Supported subset is documented and limited to deterministic, statically analyzable workloads.
- [ ] Unsupported features are rejected with clear errors rather than hidden fallbacks.

## Build and validation

- [ ] `cargo build --workspace` succeeds cleanly.
- [ ] `cargo test --workspace` passes without regressions.
- [ ] AST export pipeline executes through the native `ast` module and prints the re-enabled AST confirmation.
- [ ] Fresh generated Rust output is produced in a temporary directory for each benchmark invocation.
- [ ] Benchmark harness compiles the generated Rust with `rustc` on every run and does not rely on stale cache shortcuts.
- [ ] Differential validation against CPython passes for the audited workload corpus.

## Security gates

- [ ] Input files are restricted to safe project-local paths.
- [ ] Output files are restricted to safe project-local directories.
- [ ] External tool discovery uses resolved, trusted executable paths instead of arbitrary PATH hijacking.
- [ ] Benchmark and build scripts run with sanitized PATH and controlled environment variables.
- [ ] The release notes clearly state that the compiler executes user code and should be used in a trusted environment only.

## Documentation and communication

- [ ] README clearly states the compiler subset, supported semantics, and known limitations.
- [ ] Release notes include the difference between Tarvos and CPython/PyPy/Numba/Nuitka/Codon.
- [ ] Compatibility boundaries are explicit: not a general interpreter, not a universal package runner.

## Distribution and operations

- [ ] Binary/package is built from a pinned toolchain state.
- [ ] Installation instructions are reproducible and environment-aware.
- [ ] Benchmark output is recorded in a transparent format with actual runtime names and reasons for unavailability.
- [ ] Known unsupported behaviors are documented in release docs and docs/compiler-status.md.

## Go / no-go criteria

Go only if all mandatory items above are checked and there are no unresolved correctness or security blockers for the supported subset.

If any block remains, the release should remain pre-0.1 and be labeled as an engineering preview rather than a stable product release.
