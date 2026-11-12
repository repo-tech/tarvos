# Tarvos v1.2 - Ultra-fast Python to Native Rust Compiler

Tarvos is a high-performance Python compiler that transpiles a statically analyzable subset of Python into optimized Rust and compiles it into a single native executable. The produced binary is a standalone program: it does not embed CPython and does not require pyo3 or a Python installation at run time.

## Features

### Core Language Support
- **Statically Analyzed Subset**: Python subset with compile-time analysis
- **Type Inference**: Full type system with unions, generics, and inference
- **Control Flow**: if/else, loops (for/while), match statements
- **Data Structures**: Lists, Dicts, Sets, Tuples with native optimization
- **Functions**: First-class functions, closures, decorators

### Optimization Pipeline
- **Constant Folding**: Compile-time expression evaluation
- **Common Subexpression Elimination (CSE)**: Remove redundant calculations
- **Strength Reduction**: Replace expensive operations with cheaper ones
- **Dead Code Elimination**: Remove unused code paths
- **Loop Optimization**: Loop invariant code motion, vectorization
- **Inline Optimization**: Function inlining for hot paths

### Code Generation Backends
- **Native Rust** (`tarvos-codegen-rust`): Native x86_64 code generation
- **Cranelift** (`tarvos-codegen-cranelift`): SSA-based backend
- **WebAssembly** (`tarvos-codegen-wasm`): Browser and WASM targets
- **Cloud Lambda** (`tarvos-codegen-cloud`): AWS Lambda compatible code

### CLI Commands
- `tarvos build` - Build native executable
- `tarvos compile` - Compile to Rust source
- `tarvos run` - Build and run in one step
- `tarvos serve` - HTTP API gateway for AI/LLM integration
- `tarvos validate` - Run test suite against reference outputs
- `tarvos doctor` - Check toolchain and environment
- `tarvos analyze` - Static analysis and complexity profiling
- `tarvos benchmark` - Python vs Tarvos comparison
- `tarvos export` - Export as standalone Cargo project

## Quick Start

```powershell
# Build an executable from Python
tarvos build app.py -o myapp

# Run a Python file
tarvos run script.py

# Benchmark performance
tarvos benchmark kernel.py

# Validate against test suite
tarvos validate
```

## Project Structure

```
tarvos/
├── crates/              # Rust workspace crates
│   ├── tarvos-cli/      # Main CLI application
│   ├── tarvos-core/     # Core compiler infrastructure
│   ├── tarvos-ast/      # AST definitions
│   ├── tarvos-parser/   # Python AST parser
│   ├── tarvos-lexer/    # Lexical analyzer
│   ├── tarvos-ir/       # Intermediate representation
│   ├── tarvos-analysis/# Static analysis passes
│   ├── tarvos-optimizer/# Optimization passes
│   ├── tarvos-types/    # Type system
│   ├── tarvos-codegen-rust/ # Native Rust backend
│   ├── tarvos-codegen-cranelift/ # Cranelift backend
│   ├── tarvos-codegen-wasm/ # WebAssembly backend
│   ├── tarvos-codegen-cloud/ # AWS Lambda backend
│   └── tarvos-server/   # HTTP API gateway
├── docs/                # Documentation
├── examples/            # Example Python programs
├── python/              # Python runtime
├── tests/               # Test suite
└── scripts/             # Build and utility scripts
```

## Architecture

```
Python Source (.py)
        ↓
┌──────────────────────────┐
│  Lexer → Parser → AST    │
└──────────────────────────┘
        ↓
┌──────────────────────────┐
│  Type Checker            │
└──────────────────────────┘
        ↓
┌──────────────────────────┐
│  IR Lowering             │
└──────────────────────────┘
        ↓
┌──────────────────────────┐
│  Optimizer Passes        │
│  (CSE, Strength Reduction,│
│   DCE, Constant Folding) │
└──────────────────────────┘
        ↓
┌──────────────────────────┐
│  Codegen Backend         │
│  (Rust / Cranelift / WASM / Cloud) │
└──────────────────────────┘
        ↓
   Native Executable
```

## Build & Test

```powershell
# Build the CLI
cargo build -p tarvos-cli --release

# Run tests
cargo test --workspace

# Run benchmarks
cargo run --bin tarvos -- benchmark benchmarks/python/
```

## Requirements

- Rust 1.98+
- CMake (for some dependencies)
- Git (for version info)

## License

MIT
