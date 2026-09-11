# Tarvos 0.0.1 Changelog

## Highlights

- Re-enabled the native Python AST visitor (`ast.NodeVisitor`) for the supported subset.
- Added a safe source-only pipeline that works without a Rust toolchain for transpile output generation.
- Kept EXE generation as an optional native build step when Rust is available.
- Hardened PATH and command execution for benchmark and compiler tooling.
- Added reproducible benchmark matrix and fairness checks for static numeric workloads.

## What is included

- Python source -> AST export -> IR lowering -> Rust source generation
- runtime detection for CPython, PyPy, Numba, Nuitka, Codon, and Rust
- fair no-fold benchmark workloads to avoid trivial constant folding
- production-safe CLI commands: compile, build, run, export, clean, doctor, benchmark

## Known scope

Tarvos 0.0.1 is a compiler for a statically analyzable subset of Python, not a full Python interpreter replacement. It is optimized for numeric, loop-heavy, deterministic workloads with clear static semantics.

## Validation gates

- CLI build passes
- AST visitor re-enabled and validated
- same-output validation against dynamic benchmark cases passes
- safety checks for external tool execution are in place
