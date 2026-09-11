# Security Review 2.0

## Scope

This review audited the actual execution paths used by Tarvos: the CLI, the Python AST exporter, the benchmark harness, and the installation scripts for benchmarking competitors.

## Summary

The codebase does not contain obvious direct shell-injection patterns in the main compiler path, and the hot path uses argument arrays rather than shell strings. This is good practice. The main risk is not classic injection in the Python source itself; the risk is that Tarvos is a code execution engine that invokes Python, Rust, and external toolchains. That means it should be treated as privileged tooling and used only in trusted environments.

## Findings

### 1. Untrusted input executes code

The compiler accepts arbitrary Python files, exports their AST with Python, lowers them into IR, and then invokes `rustc` to compile generated Rust. This is a legitimate design for a compiler, but it is also a privileged execution path.

Risk: a user or automation could feed arbitrary scripts and trigger native compilation/execution.

Mitigation implemented:
- input paths are restricted to safe project-local paths in the CLI
- output paths are restricted to safe project-local directories
- benchmark and build commands are run from controlled repo-local directories

### 2. PATH hijacking risk

The original code used raw `python3`, `python`, `rustc`, `cargo`, `nuitka`, and other names directly from PATH. This is vulnerable to PATH poisoning or malicious local binaries earlier in PATH.

Risk: a malicious executable earlier in PATH could be executed instead of the intended tool.

Mitigation implemented:
- tool resolution now prefers resolved absolute executables in safe roots
- the benchmark environment sanitizes PATH and keeps trusted project and system tool directories
- Python discovery in the CLI now validates the interpreter before execution

### 3. Arbitrary output overwrites

The CLI originally allowed writing generated output to any filesystem location the process had access to.

Risk: a local user or attacker could overwrite arbitrary files by selecting a malicious output path.

Mitigation implemented:
- output paths are checked to remain under the current project-safe root
- the CLI errors out instead of writing outside the allowed workspace

### 4. Benchmark scripts were not environment-isolated

The benchmark and installation scripts launched external processes without a sanitized environment.

Risk: a malicious PATH or environment override could execute unexpected binaries.

Mitigation implemented:
- the benchmark runner now sanitizes environment variables before invoking child processes
- tool resolution is constrained to project-local or trusted runtime directories
- benchmark and install steps use safe subprocess wrappers instead of raw shell execution

## Hardening patch set implemented

1. Safe execution policy for CLI input/output
   - project-local path enforcement
   - disallow writes outside the working project root
   - ensure benchmark and compile operations are confined to the repository-safe workspace

2. Safe tool discovery and PATH handling
   - resolve executables using absolute paths in trusted roots
   - sanitize PATH before launching subcommands
   - avoid raw `python3`/`rustc` execution from arbitrary PATH entries

3. Benchmark harness hardening
   - safe subprocess wrapper centralizes sanitized environment handling
   - tool detection validates resolved path trust before use
   - environment checks and tool availability reporting use the secure path model

4. Release documentation alignment
   - the repo now includes a formal 0.0.1 release checklist
   - the Phase 2 roadmap defines the next hardening milestones

## Final assessment

The project is now safer for local trusted use and is far better aligned with a documented subset-compiler release model. It still should not be considered a general-purpose, untrusted runtime. The correct release posture is: trusted local compiler for a defined subset, not a fully sandboxed general execution engine.

## Production execution policy

Tarvos is a compiler and compatibility runner, not a security sandbox.
Before compiling or running a file:

- treat the Python source and every imported module as trusted code
- run builds in a dedicated workspace with no secrets in its environment
- use a restricted OS account or container for CI jobs processing untrusted input
- keep network access disabled unless the workload explicitly requires it
- do not expose `tarvos run` as a service endpoint without an external
  sandbox, resource limits, and process isolation
- review generated native binaries before distributing them

The `--python-fallback` mode delegates execution to the local CPython
interpreter. It preserves compatibility, but it does not reduce the trust
requirements of the input program.
