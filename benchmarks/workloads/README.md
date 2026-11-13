# Deterministic benchmark workloads

This directory contains a minimal set of deterministic Python workloads covering the main benchmark categories used for correctness and performance comparison. Each workload is written to be simple, reproducible, and comparable across CPython, PyPy, Numba, Nuitka, Codon, and Tarvos when supported.

## Categories

- arithmetic
- integer loops
- branching
- nested loops
- function calls
- recursion
- list iteration
- list indexing
- comparisons
- mixed arithmetic/control flow

## Expected-output discipline

Every workload should:

- use a fixed input size,
- produce a single deterministic result,
- print a final scalar or simple structure (for example, an integer or list),
- avoid reliance on random or wall-clock behavior.

The workload set is currently validated through `benchmarks/workloads/expected_outputs.json` and the differential harness in `scripts/diff_test.py`. The audited corpus currently includes:

- arithmetic
- branching
- comparisons
- function calls
- integer loops
- list indexing
- list iteration
- mixed arithmetic/control flow
- nested loops
- recursion

The `benchmarks/competitor_env.py` script reports whether PyPy, Numba, Nuitka, Codon, and Rust are available in the current environment without forcing installs.

For actual runtime comparison, use the benchmark matrix script to compare the
fair workload across every available runtime. The current environment reports:

- CPython: available
- Tarvos: available
- PyPy: unavailable
- Numba: unavailable
- Codon: unavailable
- Rust: available as a toolchain, but not paired with a workload-specific Rust reference in this repository

The command is:

```bash
python benchmarks/benchmark_matrix.py --runtime all --repeats 3 --json-output benchmarks/results/runtime_matrix.json
```
