# Tarvos 1.0 Subset Contract

## Scope

Tarvos 1.0 is a compiler for a narrow but reliable subset of Python. The supported subset is intentionally limited to code patterns that can be analyzed statically and lowered predictably to Rust.

The product is designed for deterministic, loop-heavy, numeric, and transformation-oriented workloads. It is not a full Python interpreter and it does not try to support arbitrary dynamic behavior.

## Supported constructs

The 1.0 contract will support the following categories:

- literals: int, float, bool, string, None
- variable declarations and reassignment
- arithmetic operators: +, -, *, /
- comparison operators: ==, !=, <, <=, >, >=
- `print(...)` with string, numeric, bool, and mixed arguments
- `if / elif / else` control flow
- `while` loops with compile-time-safe conditions
- `for ... in range(...)` loops
- simple function definitions with typed parameters and return annotations
- function calls within the supported subset
- list literals and simple index reads/writes
- statically typed dictionary literals, lookups, and string-key updates
- deterministic iteration and accumulator patterns

## Supported type model

The 1.0 type system supports:

- `int` -> mapped to Rust `i64`
- `float` -> mapped to Rust `f64`
- `bool` -> mapped to Rust `bool`
- `string` -> mapped to Rust `String`
- `list[T]` -> mapped to Rust `Vec<T>` for supported element types
- `dict[K, V]` -> mapped to Rust `HashMap<K, V>` for scalar, homogeneous keys and values
- `None` -> unsupported as a value in executable lowering and treated as a rejected pattern

## Explicitly unsupported constructs

The following are considered out of scope for 1.0 and must fail with clear diagnostics:

- arbitrary dynamic Python features
- imports and module loading
- classes and object-oriented constructs
- lambdas and closures
- comprehensions and generator expressions
- `try / except / finally`
- `async / await`
- decorators and metaprogramming
- monkey patching and runtime reflection
- nested dynamic scopes beyond the supported typed functions
- untyped or unknown runtime values that cannot be lowered safely
- dynamic dictionary methods, dictionary unpacking, and heterogeneous dictionaries

## Supported semantics

For supported features, the compiler guarantees:

- deterministic lowering to Rust
- direct output parity against CPython for the audited workload set
- explicit type checking for typed function signatures and variable assignment
- fail-fast behavior when a construct falls outside the supported subset

## Validation policy

The 1.0 release will only claim correctness for workloads that meet the supported subset contract. Any unsupported pattern must produce a clear compiler error rather than silently miscompiling.

This ensures that the product remains honest and production-credible.

## Implementation expectation

The compiler must preserve these rules:

1. supported features compile and run predictably
2. unsupported features are rejected early
3. behavior stays deterministic for the audited subset
4. benchmark claims are scoped to the subset, not to all Python programs

## Release gate

Version 1.0 is not ready until:

- every supported construct has tests
- every unsupported construct fails with a clear error
- the subset is documented and stable
- benchmark and validation work is restricted to the supported subset
