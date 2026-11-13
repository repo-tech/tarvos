# Tarvos — Python `None` Support Implementation

**Status:** ✅ Complete & Verified  
**Date:** 11/4/2026  
**Branch:** `tarvos_v1.2`

---

## 1. Executive Summary

Added Python `None` as a first-class value/type across Tarvos IR, analysis, codegen, and optimizer crates. The `None` type now lowers correctly, is treated as a compile-time constant, and emits as `__TarvosValue::None` in generated Rust code. End-to-end validation (`x = None; x = "hello"; print(x)`) passes with correct output.

---

## 2. What Was Implemented

### 2.1 IR Layer (`tarvos-ir`)

| Component | Change |

### 2.3 Codegen Layer (`tarvos-codegen-rust/src/lib.rs`)

| Function | Change |
|----------|--------|
| `emit_value()` | Emits `"__TarvosValue::None".to_string()` for `Value::None` |
| `value_type()` | Returns `Type::None` for `Value::None` |
| `require_display()` | Added `Value::None => {}` arm (no-op: `None` needs no display impl beyond default) |
| `value_uses_set()` | Added `\| Value::None` to constant catch-all |
| `value_uses_hash_map()` | Added `\| Value::None` to constant catch-all |
| Brace mismatch fix | Fixed premature closure of `match function.as_str()` block in `Value::Call` arm |

### 2.4 Optimizer Layer (`tarvos-optimizer/src/lib.rs`)

| Function | Change |
|----------|--------|
| `value_type_of()` | Returns `Type::None` for `Value::None` |
| `is_pure_value()` | Marks `Value::None` as pure/constant (foldable) |

---

## 3. Verification Results

### 3.1 Compilation

```
cargo check -p tarvos-ir -p tarvos-analysis -p tarvos-optimizer -p tarvos-codegen-rust
-> Finished dev profile [unoptimized] target(s)
   (1 pre-existing dead_code warning, unrelated to None)
```

### 3.2 Unit Tests

| Crate | Tests | Passed | Failed |
|-------|-------|--------|--------|
| `tarvos-ir` | 0 | 0 | 0 |
| `tarvos-analysis` | 12 | 12 | 0 |
| `tarvos-optimizer` | 19 | 19 | 0 |
| `tarvos-codegen-rust` | 10 | 10 | 0 |
| **Total** | **41** | **41** | **0** |

### 3.3 End-to-End Runtime Test

**Input** (`test_dynamic.py`):
```python
x = None
x = "hello"
print(x)
print(x)
print(x)
```

**Generated Rust:**
```rust
let mut x = __TarvosValue::None;
x = __TarvosValue::None;
x = __TarvosValue::Str("hello".to_string());
println!("{}", __tarvos_display(&x));
println!("{}", __tarvos_display(&x));
println!("{}", __tarvos_display(&x));
```

**Runtime Output:**
```
hello
hello
hello
```
✅ `None` reassignment to `String` works; `__tarvos_display` renders correctly.

---

## 4. Files Modified

| File | Description |
|------|-------------|
| `tarvos-analysis/src/lower.rs` | `Expr::None` lowering; `Type::None` in `value_type()` |
| `tarvos-codegen-rust/src/lib.rs` | `emit_value`, `value_type`, `require_display`, `value_uses_set`, `value_uses_hash_map`, brace fix |
| `tarvos-optimizer/src/lib.rs` | `value_type_of`, `is_pure_value` for `None` |
| `test_dynamic.py` | End-to-end `None` test script |

---

## 5. Roadmap: What Needs to Be Done Next

### Phase 1 — Dynamic Semantics (Immediate)

| # | Feature | Description | Priority |
|---|---------|-------------|----------|
| 1 | **Type Joins / Union Types** | When `x` is assigned `None` then `"hello"`, infer `None \| Str` instead of widening to `Dynamic` | High |
| 2 | **Dynamic Containers** | Support `list`, `dict`, `set` with mixed/unknown element types | High |
| 3 | **Arithmetic Dispatch** | `__tarvos_add`, `__tarvos_sub`, etc. for mixed int/float/str operations at runtime | High |

### Phase 2 — Compiler Robustness

| # | Feature | Description | Priority |
|---|---------|-------------|----------|
| 4 | **Match Exhaustiveness Audit** | Scan all `Value`/`Type` match arms across crates for missing variants | Medium |
| 5 | **Source-Located Error Messages** | Attach line/column info to compile errors for better DX | Medium |
| 6 | **Incremental Compilation** | Cache transpiled output per module to avoid full rebuilds | Medium |

### Phase 3 — Performance & Scale

| # | Feature | Description | Priority |
|---|---------|-------------|----------|
| 7 | **JIT / LLVM Backend** | Alternative to Rust transpilation for faster compilation cycles | Low |
| 8 | **Parallel Compilation** | Multi-module compilation across CPU cores | Low |
| 9 | **Memory Management** | Ownership analysis to minimize unnecessary `clone()` calls | Low |

### Phase 4 — Ecosystem & Developer Experience

| # | Feature | Description | Priority |
|---|---------|-------------|----------|
| 10 | **LSP / VS Code Extension** | Type hints, error squiggles, go-to-definition for `.tarvos` / `.py` files | Low |
| 11 | **Package Manager** | `tarvos install` / `tarvos publish` for module distribution | Low |
| 12 | **Benchmarking Suite** | Automated regressions vs CPython with fairness checks | Low |

---

## 6. Known Limitations

1. **No union type inference yet:** `x = None; x = "hello"` currently works via dynamic typing, but the type system does not yet infer `None \| Str`.
2. **`value_uses_set` / `value_uses_hash_map` marked `dead_code`:** These helpers exist but are not yet called from the main pipeline; ready for set/dict support integration.
3. **GNU toolchain required:** Windows MSVC linker (`link.exe`) not available; builds use `stable-x86_64-pc-windows-gnu`.

---

## 7. How to Run

### Build
```bash
cd tarvos
cargo build -p tarvos-cli
```

### Run End-to-End Test
```bash
tarvos run test_dynamic.py
```

### Run Unit Tests
```bash
cargo test -p tarvos-ir -p tarvos-analysis -p tarvos-optimizer -p tarvos-codegen-rust
```

---

## 8. Contact

Maintained as part of the Tarvos dynamic-Python-semantics mission.  
Next upgrade target: **Type Joins + Dynamic Containers + Arithmetic Dispatch**.
