# compiler/ — Archived Prototype

This directory contains an **early standalone prototype** of the Tarvos compiler
written before the multi-crate workspace (`crates/`) was established.

It is **not part of the Cargo workspace** and is not compiled or tested by `cargo build`.

## Why is it still here?

For historical reference — it shows the initial design decisions and the evolutionary path
from a monolithic structure to the current modular crate architecture.

## What to use instead

Use the workspace crates under `crates/`:

```
crates/
├── tarvos-ast/         ← replaces compiler/src/ast/
├── tarvos-ir/          ← replaces compiler/src/ir/
├── tarvos-codegen-rust/ ← replaces compiler/src/codegen/
└── tarvos-analysis/    ← replaces compiler/src/ir/lower.rs
```

Do not add new code here. If this directory becomes confusing, delete it.
