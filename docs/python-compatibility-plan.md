# Python Compatibility Plan

## Important distinction

Python has two different implementation goals:

1. **Source compatibility**: parse and accept Python syntax.
2. **Semantic compatibility**: preserve Python's dynamic runtime behavior, standard
   library, exceptions, object model, imports, reflection, and extension modules.

The current Tarvos pipeline provides neither full-language guarantee. It
exports a selected set of CPython AST nodes into a small typed AST, lowers that
AST to a compact IR, and emits Rust. Adding more AST node visitors alone cannot
make arbitrary Python compatible.

## Target architecture

Tarvos should support two explicit modes:

### Native subset mode

- statically analyze a bounded Python subset
- reject unsupported constructs before code generation
- generate Rust/native binaries
- provide deterministic performance and output guarantees

### Python compatibility mode

- execute general Python through an embedded or delegated CPython runtime
- preserve imports, objects, exceptions, generators, classes, reflection, and
  standard-library behavior
- optionally accelerate verified hot functions through the native subset compiler
- fall back to the Python runtime when a function cannot be safely lowered

This hybrid design is the realistic way to provide broad Python support without
pretending that a small Rust code generator is a full Python implementation.

## Compatibility runtime available now

Tarvos now exposes an explicit compatibility-runtime command:

```powershell
tarvos python path\to\program.py arg1 arg2
```

This runs the program with the local CPython interpreter, so programs using
imports, classes, exceptions, standard-library modules, and other dynamic
Python features can still be used through the Tarvos product surface.
Native compilation remains opt-in through `compile`, `build`, and `run` for the
verified subset.

This is a compatibility execution path, not a claim that those features are
already lowered to Rust.

The native subset currently lowers numeric `**` (integer operands use checked
`i64` exponentiation and floating-point operands use `powf`), `break` inside
`while`/`for`, `list.append(value)` for statically typed lists, and `len(list)`.
Integer powers must have a non-negative exponent and fit in `i64`; overflow
panics in the generated native program rather than silently wrapping. Method
calls other than `list.append` and unsupported operand types remain explicit
compile-time errors.

For an explicit hybrid attempt, use:

```powershell
tarvos run path\to\program.py --python-fallback
```

Tarvos first attempts native subset compilation. If that compilation is
rejected, it reports the native error and runs the complete program through
CPython. This is whole-program fallback today; function-level hot-path
acceleration is a later milestone and is not claimed as implemented yet.

The current acceleration foundation can inspect candidates:

```powershell
tarvos analyze program.py --hot-functions
```

This reports whether the module contains user-defined functions and
loop/arithmetic hotspots. It does not replace the function or claim a speedup;
the next milestone is the typed function ABI and correctness-checked bridge.

## Feature expansion order

1. expressions and operators: unary, boolean, conditional, tuples, sets, dicts
2. statements: imports, `elif`, `break`, `continue`, `pass`, `with`
3. functions: defaults, keyword arguments, varargs, closures, nested scopes
4. runtime values: objects, attributes, methods, iterators, exceptions
5. modules and standard library interop
6. classes, generators, async execution, decorators, reflection
7. extension-module and packaging compatibility

Every phase needs parser coverage, semantic tests, generated-code tests, and
CPython differential tests. A feature is not supported merely because the AST
exporter accepts its syntax.

## Dynamic runtime input milestone

The first safe dynamic-input milestone should avoid unsupported imports:

- add typed command-line argument support to the generated program
- add a documented `tarvos.runtime` input API
- lower supported input values to native types
- retain CPython fallback for unsupported dynamic values

Environment-variable access through `os.environ` should not be claimed until
imports, attributes, and runtime object semantics are implemented.

## Benchmark contract

Benchmarks must use identical source semantics across CPython, handwritten Rust,
C++, and generated Tarvos code. Reports must contain separate fields for:

- source translation time
- native compilation time
- first execution time
- warm execution time
- output equality

Constant-folded workloads may be used to test optimizer behavior, but must not be
used as proof that Tarvos is generally faster than C++ or Rust.

## 1.0 and later release boundary

Tarvos 1.0 should remain the reliable native subset compiler. Broad Python
compatibility is a separate major initiative requiring a runtime layer. The
project can continue toward that goal, but it must not advertise full Python
support until the hybrid runtime and differential test suite prove it.
