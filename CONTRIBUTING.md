# Contributing to Tarvos

Thanks for your interest in contributing. Tarvos is an ahead-of-time compiler
for a statically analyzable subset of Python that emits native Rust. The
project values **semantic correctness above all else**: a contribution that
makes generated code compile but behave differently from CPython is a
regression, even if the tests pass.

## Development setup

### Compiler / CLI (Rust)

Requirements: a stable Rust toolchain (see `rust-toolchain.toml`), Python
3.11+ (the CPython AST exporter and validation suites run on it).

```bash
cargo fmt --all -- --check     # formatting gate
cargo check --workspace --all-targets
cargo test --workspace         # includes the CLI compatibility gate
cargo build --workspace --release
python validation/run_validation.py   # production validation suite
```

Useful single tests:

```bash
cargo test -p tarvos-cli --test compatibility_gate   # native + fallback behavior
cargo test -p tarvos-codegen-rust                    # emitter unit tests
```

### Frontend (Next.js)

The Studio frontend lives in the separate `tarvos-frontend` checkout.

```bash
npm ci          # or npm install
npm run build   # production build with type checking and lint
npm run dev     # http://localhost:3000
```

The frontend proxies API calls to the local gateway (`tarvos-server`, port
8080). Start the gateway first to exercise real compile/terminal flows:

```bash
cargo run -p tarvos-server
```

## Project layout

| Path | What it is |
| --- | --- |
| `compiler/` | Ruff-based reference front end (`tarvos-ruff`) |
| `crates/tarvos-parser` | CPython AST importer |
| `crates/tarvos-analysis` | Semantic analysis, lowering, stdlib registry |
| `crates/tarvos-ir` | Typed intermediate representation |
| `crates/tarvos-optimizer` | IR optimization passes |
| `crates/tarvos-codegen-rust` | Rust emitter + generated-runtime preludes |
| `crates/tarvos-core` | Compile pipeline, normalize pass, API gateway |
| `crates/tarvos-cli` | The `tarvos` binary |
| `crates/tarvos-server` | Gateway server entry point |
| `benchmarks/`, `validation/` | Differential and validation suites |
| `docs/` | Compiler status and architecture documentation |

## Making a change

1. **Open an issue first** for anything that changes compiler semantics,
   the public CLI surface, or the API contract.
2. Keep changes focused: one logical change per pull request.
3. **Fix root causes**, not generated output. If the emitter produces wrong
   Rust, fix the lowering/analysis that produced the bad IR.
4. Add a regression test that fails before your change (a
   `compatibility_gate` test for native behavior, or a `tarvos-tests`
   pipeline test for lowering/emission).
5. Update `docs/compiler-status.md` when you extend or narrow the supported
   Python subset.

## Python semantics rules

- Never change integer/division/modulo/shift semantics to satisfy the Rust
  compiler. Use explicit checked operations or an actionable
  `--python-fallback` diagnostic instead.
- `list`/`dict` iteration order must match CPython semantics or the
  construct must refuse native compilation (see the `list(dict)` gate).
- If native compilation cannot preserve a behavior, emit a diagnostic that
  names the supported alternatives — never silently emit different
  behavior.

## Commit and PR conventions

Commits use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(compiler): add native list and sorted builtins
fix(cache): invalidate stale native translation cache
docs(release): document v1.1.0-rc.2
```

Before opening a PR, run:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `npm run build` (for frontend changes)
- `git diff --check`

In the PR description, state: what changed, why, compatibility impact, and
the tests you ran. Screenshots help for UI changes.

## Reporting bugs

Use the bug report issue template. For compiler bugs, include a **minimal
Python example**, the expected CPython output, and the actual Tarvos output
or diagnostic. For security issues, see [SECURITY.md](SECURITY.md).

## License

By contributing you agree that your contribution is licensed under the same
license as the project (GNU AGPL v3, see [LICENSE](LICENSE)).