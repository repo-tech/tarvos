# Tarvos Architecture Document

## Core Design Principles

1. **IR-Centric** - The Intermediate Representation is the architectural center
2. **Type-Safe** - All types checked at compile time
3. **Modular** - Each crate has single responsibility
4. **Testable** - Every component can be tested independently
5. **Extensible** - New backends/optimizations without modifying core

## Type System Architecture

```
Type (from tarvos-types)
├── Primitive
│   ├── Int (i64)
│   ├── Float (f64)
│   ├── Bool
│   └── String
├── Composite
│   ├── Array<T>
│   └── Tuple<T...>
└── Special
    └── Unknown (for type inference)
```

## IR Architecture (Core)

```
Module
├── statements: Vec<Stmt>

Stmt (Statement)
├── Let { name, ty, value }          // Variable binding
├── Print(Value)                      // Output
├── If { test, body, orelse }        // Conditional
├── While { test, body }             // Loop
├── For { target, iter, body }       // Iteration
├── Function { name, params, body }  // Function definition
└── Return(Option<Value>)            // Early exit

Value (Expression)
├── Int(i64)                         // Integer literal
├── Float(f64)                       // Float literal
├── String(String)                   // String literal
├── Bool(bool)                       // Boolean literal
├── Name(String)                     // Variable reference
├── Binary { left, op, right, ty }   // Binary operation
├── Call { function, args, return_type }  // Function call
└── List { elements, element_type }  // List literal
```

## Compilation Pipeline

### Stage 1: Python AST Export (python/ast_export.py)
- Parses Python source using `ast` module
- Converts to JSON representation
- Normalizes Python AST nodes

**Input:** Python source code
**Output:** JSON string

### Stage 2: AST Parsing (tarvos-parser)
- Deserializes JSON into `Module` struct
- Validates AST structure via Rust type system
- Caught by serde during deserialization

**Input:** JSON string
**Output:** `tarvos_ast::Module`

### Stage 3: Semantic Analysis & Lowering (tarvos-analysis)

#### 3a: Type Inference
- Literals → their types (10 → Int, "hello" → String)
- Names → lookup in type context
- Binary ops → infer from operand types + operation
- Call sites → known function return types

#### 3b: Lowering & Validation
1. **Check** - Verify assignments target valid names
2. **Type** - Infer and track variable types
3. **Transform** - Convert AST nodes to IR
4. **Validate** - Ensure type consistency

### Stage 4: Optimization (tarvos-optimizer)

#### Constant Folding
Evaluates constant expressions at compile time:
- `10 + 20` → `30`
- `3.14 * 2.0` → `6.28`

#### Dead Code Elimination (framework ready)
Remove unreachable statements

### Stage 5: Code Generation (tarvos-codegen-rust)

Map IR to Rust:

| IR | Rust |
|---|---|
| `Type::Int` | `i64` |
| `Type::Float` | `f64` |
| `Type::Bool` | `bool` |
| `Type::String` | `String` |
| `BinaryOp::Add` | `+` |
| `BinaryOp::Eq` | `==` |

### Stage 6: Rust Compilation (rustc)
- Invoked by user: `rustc output.rs -o binary`
- Further optimized by LLVM
- Produces native machine code

## Type Context & Symbol Table

The `TypeContext` maintains symbol bindings:

```rust
pub struct TypeContext {
    pub symbols: HashMap<String, Type>
}
```

Operations:
- `declare(name, type)` - Register a new variable
- `lookup(name)` - Get type of variable

## Module Dependencies

```
tarvos-cli (main entry point)
├── tarvos-parser
│   └── tarvos-ast
├── tarvos-analysis
│   ├── tarvos-ast
│   ├── tarvos-ir
│   └── tarvos-types
├── tarvos-optimizer
│   └── tarvos-ir
└── tarvos-codegen-rust
    ├── tarvos-ir
    └── tarvos-types

tarvos-ir
└── tarvos-types
```

**Key:** No circular dependencies. Dependency tree is acyclic.

## Adding New Language Features

Example: Support for `bool` AND/OR operators

1. **Update AST** - Already supports binary operators
2. **Update Type System** - Already has `Bool` type
3. **Update IR** - Add to `BinaryOp` enum
4. **Update Lowering** - Parse "and" → BinaryOp::And
5. **Update Type Inference** - (Bool, Bool, And) → Bool
6. **Update Codegen** - Emit "&&" for And
7. **Add Tests**

All changes are **localized** to specific modules.

## Future Extensibility

### New Backends
Add `tarvos-codegen-wasm/` without modifying existing code.

### New Optimizations
Add new passes without affecting others.

### New Frontends
Add `tarvos-frontend-*` for PyTorch, NumPy, etc.

---

**Architecture designed for growth while maintaining simplicity and correctness.**
