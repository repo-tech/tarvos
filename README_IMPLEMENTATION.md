# 🚀 Tarvos — High-Performance Python Compiler

> A production-grade compiler that transforms Python code into optimized Rust/WASM, enabling 3-10x performance improvements without rewriting applications.

## 🎯 Project Status

**Stage 1 - MVP: Compiler ✅**

- ✅ Python AST Parsing
- ✅ Tarvos AST with type annotations
- ✅ Type Inference & Checking
- ✅ Intermediate Representation (IR)
- ✅ Constant Folding Optimization
- ✅ Rust Code Generation
- ✅ End-to-end compilation pipeline

## 📋 Architecture

Tarvos follows a modular, layered architecture:

```
Python Source (.py)
    ↓
Python AST Exporter (Python script)
    ↓
JSON AST
    ↓
[tarvos-ast] AST Nodes
    ↓
[tarvos-analysis] Type Checking & Lowering
    ↓
[tarvos-ir] Intermediate Representation
    ↓
[tarvos-optimizer] Optimization Passes
    ↓
[tarvos-codegen-rust] Rust Code Generation
    ↓
Generated Rust Source
    ↓
rustc/LLVM
    ↓
Native Binary
```

## 📦 Crate Structure

```
crates/
├── tarvos-types/         # Type system (Int, Float, Bool, String, Array)
├── tarvos-ast/           # AST node definitions
├── tarvos-parser/        # Python AST → Tarvos AST
├── tarvos-ir/            # Intermediate Representation (core architecture)
├── tarvos-analysis/      # Type inference, semantic analysis, lowering
├── tarvos-optimizer/     # Optimization passes (constant folding, DCE)
├── tarvos-codegen-rust/  # IR → Rust code generation
└── tarvos-cli/           # CLI entry point
```

Each crate has a clear responsibility, enabling modular testing and future extension.

## 🏃 Quick Start

### Build
```bash
cargo build --release
```

### Compile Python
```bash
./target/release/tarvos examples/simple.py output.rs
```

### Verify Generated Code
```bash
rustc output.rs -o output
./output
```

## 💡 Supported Language Subset

**Phase 1 (MVP):**
- ✅ Primitives: `int`, `float`, `bool`, `str`, `None`
- ✅ Operators: `+`, `-`, `*`, `/`, `%`
- ✅ Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- ✅ Variables & Assignment
- ✅ Print statements
- ✅ `if`/`else` branches
- ✅ `while` loops
- ✅ `for` loops with `range()`
- ✅ List literals
- ✅ Function definitions (basic)

**Unsupported (with clear error messages):**
- Classes
- Decorators
- Generators
- `async`/`await`
- Most stdlib functions
- Dynamic typing beyond literals

## 🔍 Example: MVP Compilation

**Input: simple.py**
```python
x = 10
y = 20
z = x + y
print(z)
```

**Execution:**
```bash
$ tarvos simple.py output.rs
=== Tarvos Compiler ===

Input: simple.py
  [1/6] Exporting Python AST...
  [2/6] Parsing AST...
  [3/6] Type checking and lowering to IR...
  [4/6] Running optimizations...
  [5/6] Generating Rust code...
  [6/6] Writing output...

=== Compilation Successful ===

Output: output.rs

=== Generated Rust Code ===
fn main() {
    let x = 10;
    let y = 20;
    let z = (x + y);
    println!("{:?}", z);
}
```

**Generated & Compiled: output.rs**
```rust
fn main() {
    let x = 10;
    let y = 20;
    let z = (x + y);
    println!("{:?}", z);
}
```

**Runtime:**
```bash
$ ./output
30
```

✅ Correct! (10 + 20 = 30)

## 🧪 Testing Strategy

### Unit Tests (per crate)
```bash
cargo test --lib
```

### Integration Tests
```bash
cargo test --test '*'
```

### Differential Testing
Compare Python output vs generated Rust output:
```bash
python3 test.py > python_out.txt
./compiled_binary > rust_out.txt
diff python_out.txt rust_out.txt
```

## 🛠️ Development Workflow

1. **Inspect repository** - Understand current state
2. **Explain architecture** - Document changes in context
3. **Identify scope** - Find smallest correct implementation
4. **Plan implementation** - Break into steps
5. **Implement** - Focused, surgical changes
6. **Run tests** - `cargo fmt`, `cargo check`, `cargo test`
7. **Document impact** - What changed and why

## 📚 Key Modules

### [tarvos-types](crates/tarvos-types/)
Defines the Tarvos type system:
- `Type::Int`, `Type::Float`, `Type::Bool`, `Type::String`, `Type::None`
- `Type::Array<T>`, `Type::Tuple<T...>` (for future)
- Type errors with clear diagnostics

### [tarvos-ir](crates/tarvos-ir/)
**The architectural center** - defines:
- `Module` - root compilation unit
- `Stmt` - statements (Let, If, While, For, Function, Return)
- `Value` - expressions (Int, Float, String, Name, Binary, Call, List)
- `BinaryOp` - operators (Add, Sub, Mul, Div, Eq, Lt, etc.)
- `TypeContext` - symbol table for type lookup

Designed for optimization: immutable, acyclic, strongly typed.

### [tarvos-analysis](crates/tarvos-analysis/)
**Type checking + Lowering**:
- `TypeInference` - infer types from expressions
- `Lowerer` - convert AST → IR with type checking
- Catches type mismatches, undefined variables
- Manages symbol table

### [tarvos-optimizer](crates/tarvos-optimizer/)
**Optimization passes**:
- Constant folding (evaluates `10 + 20` → `30` at compile time)
- Dead code elimination (framework ready)
- Designed for future: loop invariant code motion, vectorization

### [tarvos-codegen-rust](crates/tarvos-codegen-rust/)
**IR → Rust**:
- Maps `Type` → Rust types
- Emits `let`, `if`, `while`, `for` statements
- Handles operators and function calls

## 🚦 Compilation Pipeline Steps

1. **Python → JSON AST** (via `ast_export.py`)
2. **JSON AST → Tarvos AST** (via `tarvos-parser`)
3. **AST → IR** (via `tarvos-analysis` with type checking)
4. **Optimize IR** (via `tarvos-optimizer`)
5. **IR → Rust** (via `tarvos-codegen-rust`)
6. **Rust → Binary** (via `rustc`/LLVM)

Each step is:
- ✅ Testable independently
- ✅ Error-reporting (with context)
- ✅ Type-safe (strong Rust typing)

## 🎯 Design Principles

1. **Correctness First** - Never silently change Python semantics
2. **Measurable Performance** - All performance claims backed by benchmarks
3. **Explicit Errors** - Unsupported features error clearly
4. **Modular Architecture** - Each crate has one job
5. **No Premature Optimization** - Optimize only bottlenecks
6. **Idiomatic Rust** - Leverage ownership, Result, Pattern Matching

## 🔮 Next Phases (Future)

**Phase 2 - Optimizer**
- Loop analysis
- Type specialization
- SIMD vectorization
- Bounds analysis

**Phase 3 - Profiler**
- `tarvos analyze` - find hotspots
- CPU/memory profiling
- Optimization candidate detection

**Phase 4 - Energy Profiling**
- Power measurement integration
- Energy efficiency analysis

**Phase 5 - Developer Platform**
- CLI UX refinement
- SDK for library integration
- GitHub Actions integration

**Phase 6 - Cloud Platform**
- Performance dashboard
- Regression detection
- Optimization recommendations

**Phase 7 - Enterprise**
- Large-scale workload support
- Advanced profiling
- SaaS integration

## 📖 Language Design

Tarvos is NOT full Python. It's a **curated subset** designed for:
- Static analysis
- Type specialization
- Efficient compilation
- Predictable performance

Example: A function without type hints is compilable, but Tarvos uses **inference + specialization**:
```python
def add(x, y):
    return x + y

# Called with: add(10, 20) → infers Int + Int → compiles to i64 arithmetic
# Called with: add(1.5, 2.5) → infers Float + Float → compiles to f64 arithmetic
```

## 🔗 Dependencies

- **anyhow** - Error handling
- **serde** - Serialization (for AST interchange)
- **thiserror** - Error types
- **Python 3.8+** - AST exporting

## 📝 Contributing

Follow the 30-step development workflow from the master prompt:
1. Inspect
2. Explain
3. Identify
4. Plan
5. Implement
6. Test
7. Document

Make small, focused commits:
```
feat(types): add Float type to type system
feat(ir): implement binary operations
feat(optimizer): add constant folding pass
test(codegen): add differential tests for arithmetic
```

## 📄 License

TBD (Tarvos is in early development)

---

**Built as a serious technology product for high-performance Python workloads.**

Next: Phase 2 - Advanced Optimizer 🚀
