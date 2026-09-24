# Tarvos compiler status

## Scope and audit basis

The current implementation is a Rust workspace with a Python AST exporter, a typed IR, analysis/lowering, optimization, and Rust code generation. The project targets a carefully bounded subset of Python rather than full Python compatibility.

## Feature matrix

| Feature | Python AST | IR | Semantic Analysis | Type Checking | Optimization | Codegen | Test | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| int | Yes (CPython `ast` exporter) | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| float | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| bool | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| string | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| None | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| variable assignment | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| augmented assignment | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| arithmetic | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| comparisons | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| if | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| while | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| for | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| range | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| functions | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| return | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| lists | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Partially implemented |
| indexing | Yes | Yes | Yes | Partial | Partial | Yes | Yes | Partially implemented |
| function calls | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| nested scopes | Yes | Yes | Partial | Partial | Partial | Partial | Partial | Limited |
| loop variables | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| basic error handling | Partial | Partial | Partial | Partial | Partial | Partial | Partial | Limited |

The native loop lowering is type-aware for `list`/array values, strings (each
Python character is materialized as a one-character string), and dictionaries
(Python key iteration). Integer literals larger than `i64` are transported
without floating-point rounding; native `range()` rejects such bounds with an
actionable fallback diagnostic because the current native loop ABI is `i64`.

## Current implementation notes

- The Python front end is intentionally not a full CPython parser replacement. It uses a CPython AST export step via `python/ast_export.py` and then deserializes the exported JSON into the project's own AST types.
- The IR is a compact typed representation that supports numerical and branch-heavy workloads.
- Code generation is focused on Rust output for a static subset of Python and is not designed to cover arbitrary Python semantics.
- Native locals preserve the inferred primitive/container/object type through
  branch and loop pre-initialization. Reassigning a native variable to an
  incompatible type is rejected with an actionable `--python-fallback`
  diagnostic instead of emitting a guessed `i64` declaration.
- Truly dynamic values still require the Python fallback; this is an explicit
  compatibility boundary, not a claim of a complete dynamic Python runtime.
- The CLI compatibility gate verifies native execution, actionable native
  failures, and `tarvos run ... --python-fallback` through CPython. Runtime
  arguments are forwarded to both execution paths; native programs only observe
  them when their supported subset exposes an argument-reading API.
- Performance benchmarking in this project should be treated as a comparison against explicitly supported subset workloads, not full-language execution.

## Audit status

The current workspace compiles under the GNU Rust toolchain and passes the existing workspace test suite. Remaining gaps are mostly in coverage beyond the supported subset, especially for advanced Python semantics, runtime exceptions, and wider edge-case behaviors.

## Proof harness and environment reporting

The project now includes a reproducible differential harness for the audited workload set:

- `scripts/diff_test.py` compares CPython stdout to Tarvos-generated Rust stdout for each workload in `benchmarks/workloads/`.
- `benchmarks/workloads/expected_outputs.json` records the deterministic expected output for each supported workload.
- `benchmarks/competitor_env.py` detects whether PyPy, Numba, Nuitka, Codon, and Rust are available on the local system without attempting unapproved installs.
- `benchmarks/results/results.json` stores the pass/fail matrix and machine-readable results for the current environment.

In the current environment, the audited workload corpus passes end-to-end, while PyPy, Numba, Nuitka, and Codon are not installed or available on PATH and Rust is available as the native compiler toolchain.
