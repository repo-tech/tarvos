# Tarvos — Project Origin

**Tarvos was created and is primarily developed by Himanshu Gupta.**

Tarvos began as an engineering exploration into compiling a statically
analyzable subset of Python into standalone native executables through Rust.

The project evolved into a compiler architecture consisting of:

- Python frontend and parsing integration
- Tarvos AST and semantic analysis
- Intermediate Representation (IR)
- Type and compatibility analysis
- Optimization passes
- Rust code generation
- Native executable generation
- CPython differential testing
- Compatibility tracking and diagnostics
- CLI, tooling, validation, and release infrastructure

The compiler architecture, implementation direction, IR design, optimization
strategy, code-generation pipeline, compatibility work, testing approach,
and overall project direction are maintained by the author.

## Open-Source Components

Tarvos builds on established open-source technology where appropriate,
including the Ruff parser/frontend and the Rust toolchain.

These components provide underlying capabilities such as parsing and native
code compilation. Tarvos's compiler architecture, intermediate
representation, lowering, optimization pipeline, compatibility layer,
diagnostics, Rust code generation, and surrounding tooling are part of the
Tarvos project.

Tarvos is therefore not a wrapper around an existing Python compiler.
It is an independent compiler project that integrates established
open-source components as parts of its toolchain.

## Development Philosophy

Tarvos is developed incrementally around one principle:

> Compile Python semantics that can be analyzed reliably into native code,
> and explicitly diagnose features that cannot yet be compiled safely.

Unsupported Python features are not silently treated as supported.
Compatibility is tracked, tested, and documented as the compiler evolves.

The project remains open source and is actively developed toward broader
Python compatibility, stronger optimization, cross-platform native builds,
and reproducible performance evaluation.
