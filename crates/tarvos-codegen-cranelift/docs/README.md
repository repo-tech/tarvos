# Cranelift Backend

This crate provides a Cranelift code generation backend for the Tarvos
compiler. It compiles Tarvos IR into Cranelift CLIF (Cranelift's
human-readable intermediate representation) text.

## Module Overview

The crate is split into two main public functions:

- `compile_to_clif(module)` — returns a `String` containing the CLIF text
- `compile_to_cranelift(module)` — returns a `cranelift_codegen::Context`

## Public API

```rust
pub fn compile_to_clif(module: &Module) -> Result<String>
pub fn compile_to_cranelift(module: &Module) -> Result<Context>
```

## Architecture

The compiler traverses the Tarvos IR module statement by statement:

1. Collect variable declarations from all reachable statements.
2. Declare variables in the Cranelift function.
3. Lower each statement (`Let`, `Print`, `If`, `While`, `For`, `Function`,
   `Return`, `Break`, `Continue`, `Expr`, `Assign`, `StructDef`, etc.).
4. Lower each value expression (`Int`, `Float`, `String`, `Bool`, `Name`,
   `Binary`, `Unary`, `Call`, `Index`, `Slice`, `MethodCall`, etc.).
5. Finalize the Cranelift function.

### Statement Lowering

| Tarvos IR statement | CLIF output |
|---|---|
| `Let { name, value }` | sets `name` to the lowered value |
| `Assign { name, value }` | re-assigns `name` to the lowered value |
| `Print(values)` | emits `print(` … `)` |
| `If { test, body, orelse }` | branches on the test value |
| `While { test, body }` | emits a loop header / body / exit block |
| `For { target, iter, body }` | lowers to a `while` loop |
| `Function { body }` | lowers `body` as a nested function |
| `Return(value)` | emits `return` |
| `Break`, `Continue`, `Expr`, `Assign`, etc. | lowered to corresponding CLIF |

### Value Lowering

| Tarvos IR value | CLIF / emitted IR |
|---|---|
| `Int(n)` | `iconst(I64, n)` |
| `Float(f)` | `iconst(I64, f.to_bits())` |
| `Bool(b)` | `iconst(I64, b as i64)` |
| `None`, `String(_)`, `Name(_)`, … | tagged placeholders |

### Build Profiles

The build profile flag (`--build-profile`) controls the optimisation level
used when compiling Rust output from Tarvos:

| Profile | `opt-level` | `lto` | `codegen-units` |
|---|---|---|---|
| `default` | 3 | fat | 1 |
| `speed` | 3 | fat | 1 |
| `minify` | z | fat | 1 |
| `native-cpu` | 3 | fat | 1 (with `target-cpu=native`) |

## Local Development

### Build

```bash
cargo build -p tarvos-codegen-cranelift
```

### Test

```bash
cargo test -p tarvos-codegen-cranelift
```

### Formatting

The crate uses the same formatting rules as the rest of the Tarvos
workspace. Run `cargo fmt --all` before committing.
