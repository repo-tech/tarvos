# Tarvos 1.1.0-rc.1

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
