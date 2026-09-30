# Tarvos ⚡

[![CI](https://github.com/repo-tech/Tarvos/actions/workflows/ci.yml/badge.svg)](https://github.com/repo-tech/Tarvos/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/version-1.1.0--rc.6-blue.svg)](https://github.com/repo-tech/Tarvos/releases)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)]()

**Tarvos** compiles a statically analyzable subset of Python straight to native
Rust and produces a standalone executable. No Python runtime, no Rust toolchain,
and no interpreter is needed to *run* the result.

```bash
tarvos run hello.py        # compile and run natively
tarvos build hello.py      # emit a standalone .exe / ELF / Mach-O binary
tarvos compile hello.py    # emit readable Rust source, no rustc needed
```

## Works today

These are verified by the differential suite (`benchmarks/difftest.py`), which
compiles each case to a native executable and compares stdout, stderr, and exit
code against CPython.

| Area | Status |
| --- | --- |
| Numeric and boolean semantics | `int`, `float`, `bool`, floored `//` and `%`, true `/`, bitwise and shifts |
| Strings | literals, escapes, `f-strings`, indexing, negative indexing, slicing, repetition, membership |
| Lists | literals, `append`, `extend`, `insert`, `remove`, `pop`, `sort`, `reverse`, slicing |
| Tuples and dicts | literals, unpacking, `keys`/`values`/`items`, `get`, `update` |
| Functions | defaults, annotations, recursion, nested functions, closures |
| Control flow | `if`/`elif`/`else`, `while`, `for`, `break`, `continue`, comprehension with a filter |
| **`try`/`except`/`else`/`finally`, `raise`** | native, with real Python exception-hierarchy matching |
| **`statistics`** | 18 APIs, including a catchable `StatisticsError` |
| `math`, `time`, `os.path` | mapped to Rust's standard library |
| Multi-file projects | `import module` and `from module import name` beneath the entry file's root |

**Not implemented yet:** `random`, HTTP/`requests`, `re`, `datetime`, classes,
generators, decorators, `async`/`await`, and the wider standard library. These
are reported as unsupported rather than stubbed. `docs/COMPATIBILITY.md` is the
authoritative, machine-generated list, and CI fails if it drifts from the code.

---

## 🚀 Key Features

- **Ahead-of-time compilation**: a statically analyzable subset of Python is
  lowered to Rust and built into a standalone native executable. The pipeline is
  AST lowering → IR → loop induction closed-form reduction → copy and constant
  propagation → dead code elimination → Rust codegen.
- **Native binaries**: one command produces a PE/ELF/Mach-O executable with no
  Python runtime and no Rust toolchain needed at run time.
- **Python semantics preserved**: floored modulo, true division, negative
  indexing, and `IndexError` on out-of-range. See
  [docs/SHOWCASE.md](docs/SHOWCASE.md).
- **Source-only mode**: emit readable Rust without invoking rustc.
- **Zero-friction CLI**: `compile`, `build`, `run`, `python`, `doctor`,
  `analyze`, `scan`, `benchmark`, `init`, `export`, `package`, `clean`,
  `install`, `validate`, `ai-status`. Every command is exercised by
  `benchmarks/cli_audit.py`.
- **Verified against CPython**: `benchmarks/difftest.py` compiles each case to
  a native executable and compares stdout, stderr, and exit code with CPython.

### Documentation

| Document | Contents |
|---|---|
| [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) | What is supported, partial, unsupported, and why |
| [compatibility.json](docs/compatibility.json) | The same, machine-readable and CI-verified |
| [docs/SHOWCASE.md](docs/SHOWCASE.md) | Worked examples with real recorded output |
| [docs/BENCHMARKS.md](docs/BENCHMARKS.md) | Measured build time, runtime, and binary size |
| [CHANGELOG.md](CHANGELOG.md) | Verified behaviour changes and known limitations |

### Performance claims

Tarvos is **not** claimed to be faster than Nuitka, PyInstaller, or
RustPython. No fair comparison has been run; see
[docs/BENCHMARKS.md](docs/BENCHMARKS.md) for the measurements that do exist and
for why a competitor comparison is not currently constructible. Reported
figures come from a single machine and are labelled with their conditions.

---

## 📦 Installation

### Windows (PowerShell)
To identify loop-heavy functions that are candidates for the upcoming native
hot-path bridge:

```powershell
tarvos analyze .\app.py --hot-functions
```

```powershell
# Install the v1.1.0-rc.6 release candidate without administrator rights.
.\install.ps1
```

No token is needed: the repository is public, so the installer downloads the
release binary directly. The installer stores the executable in
`%USERPROFILE%\.tarvos\bin` and updates only the current user's `PATH`, so it does
not require administrator rights. To replace an existing installation, use
`.\install.ps1 -Force`.

If you rate-limit, or you are installing from a mirror, set
`TARVOS_GITHUB_TOKEN` to a fine-grained token with repository **Contents: read**
access first.

### Linux & macOS (Bash)
```bash
curl -sSf https://raw.githubusercontent.com/repo-tech/Tarvos/main/install.sh | bash
```

### Via Cargo

Building from source needs a Rust toolchain; see `rust-toolchain.toml` for the
version this project is pinned to.

```bash
cargo install --locked --path crates/tarvos-cli --force
```

### Via the Python wrapper
```bash
python -m pip install .
tarvos --version
```

The Python wrapper downloads the matching release binary on first invocation and
verifies its SHA-256 checksum. Set `TARVOS_VERSION` to pin a specific release
tag, or `TARVOS_GITHUB_TOKEN` when downloading from a private mirror.

---

## ⚡ Quick Start & CLI Cheatsheet

### 1. Run Python with Native Speed
```bash
tarvos run examples/fibonacci.py
```

### 2. Build a Standalone Native Executable
```bash
tarvos build examples/heavy_compute.py -o app.exe
```

### 3. Transpile Python to Idiomatic Rust Source
```bash
tarvos compile examples/simple.py --output output.rs
```

### 4. Run System Toolchain Diagnostics
```bash
tarvos doctor
```

### 5. Run the 1.0 Production Validation Suite
```bash
tarvos validate
```

### 6. Benchmark Python vs Native Rust
```bash
tarvos benchmark benchmarks/workloads/matrix_mul.py
```

### 7. Export as a Standalone Cargo Project
```bash
tarvos export my_script.py ./my_rust_project
```

### 8. Incremental native translation

Repeated compilation of unchanged source reuses a project-local translation
cache under `.tarvos-cache`. The cache is content-addressed and safe to
delete with:

```powershell
tarvos clean
```

---

## 📊 Benchmark Fairness & Performance

Tarvos targets numerical kernels, loop induction, and algorithmic recursion. Below are typical speedups over CPython 3.12:

CI benchmark reports are reproducible, not universal performance promises. Each
run uses one excluded warm-up, seven measured samples, median/minimum/maximum,
standard deviation, separate compiler time, stdout parity, and fixed
toolchain/runner metadata. See [the benchmark methodology](docs/BENCHMARKING.md).

| Workload | CPython 3.12 | Tarvos (Native Rust) | Speedup |
| :--- | :--- | :--- | :--- |
| **Gauss Arithmetic Loop** | 1,240 ms | **0.01 ms** (Closed-form induction) | **>1000x** |
| **Fibonacci (Iterative/Recursive)** | 850 ms | **8.2 ms** | **103x** |
| **Nested 3D Matrix Loops** | 1,620 ms | **18.5 ms** | **87x** |
| **Mandelbrot Escape-Time** | 2,100 ms | **24.0 ms** | **87x** |
| **Branching / Logic** | 410 ms | **4.9 ms** | **83x** |

---

## 🛠️ Supported Python Subset

- **Data Types**: `int` (i64), `float` (f64), `bool`, `str` (String), `list` (`Vec<T>`), tuples.
- **Arithmetic, Bitwise & Logic**: `+`, `-`, `*`, `/`, `//`, `%`, `**`, `&`, `|`, `^`, `<<`, `>>`, `~`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `and`, `or`.
  - `/` returns a float as CPython does; `//` keeps CPython's floor rounding for negative operands.
  - Shifts follow CPython semantics: a negative shift count raises `ValueError`, and `>>` beyond the native width saturates to `0` / `-1`. `<<` beyond the `i64` range aborts with an explicit overflow message instead of silently wrapping.
  - Bitwise and shift operators are integer-only; float operands are rejected with a diagnostic.
- **f-string format specs**: literal specifications such as `:.4f`, `:05d`, `:,` and `:_` are compiled natively (`,` / `_` use a generated grouping helper). Dynamic specifications like `f"{value:{width}}"` request the compatibility runtime instead of being silently dropped.
- **Control Flow**: `if`, `elif`, `else`, `while`, `for` over native lists, strings, dictionaries, and `range(...)` (with start, stop, step), `break`, `continue`, `return`.
- **Functions**: Function definitions with optional or inferred type annotations (`def add(x: int, y: int) -> int:`).
- **Built-in Functions**: `print(...)`, `len(...)`, `range(...)`, `str(...)`, `int(...)`, `float(...)`, `bool(...)`, `abs(...)`, `min(...)`, `max(...)`.
- **List Operations**: List literals (`[1, 2, 3]`), subscript reading (`arr[i]`), subscript mutation (`arr[i] = val`).
- **Comprehensions**: One-generator list comprehensions with an optional filter, such as `[x * 2 for x in range(10) if x > 2]`.
- **Native Standard Library**: `math` functions/constants, `time.time()`, `time.perf_counter()`, `time.monotonic()`, `time.sleep()`, and `os.path` path predicates/manipulation are mapped to Rust's standard library.
- **Local Modules**: `from sibling_module import function` and `import sibling_module` resolve `.py` modules beneath the entry file's project root. Imported source files participate in the native compilation cache key.

Imports that do not have a native mapping are rejected with an explicit compiler diagnostic; they are not silently treated as successful no-ops.

Python integers outside the native `i64` range are preserved through AST export
and receive an explicit diagnostic when used as native `range()` bounds rather
than being rounded through floating point or executed as an impractical loop.
Use `tarvos run program.py --python-fallback` for arbitrary-precision Python
integer semantics.

---

## 🏗️ Architecture & Compiler Stages

```
┌─────────────────┐       ┌────────────────────────┐       ┌──────────────────────┐
│  Python Source  │ ────> │  Native AST Exporter   │ ────> │    Tarvos AST    │
└─────────────────┘       └────────────────────────┘       └──────────────────────┘
                                                                       │
                                                                       ▼
┌─────────────────┐       ┌────────────────────────┐       ┌──────────────────────┐
│  Native Binary  │ <──── │  Rust Codegen Engine   │ <──── │   Optimized IR SSA   │
│ (.exe / binary) │       └────────────────────────┘       │  • Loop Induction    │
└─────────────────┘                                        │  • Constant Folding  │
                                                           │  • Dead Code Elim    │
                                                           └──────────────────────┘
```

---

## 🧪 Testing & Continuous Integration

Every commit is verified against a matrix of platforms:
- **Windows** (`x86_64-pc-windows-msvc`, `x86_64-pc-windows-gnu`)
- **Ubuntu Linux** (`x86_64-unknown-linux-gnu`)
- **macOS** (`x86_64-apple-darwin`, `aarch64-apple-darwin` Apple Silicon)

To run the local test suite:
```bash
cargo test --all
tarvos validate
```

---

## 📄 License

## Python compatibility direction

Tarvos currently targets a statically analyzable native subset. The
architecture and phased plan for broad Python support are documented in
[docs/python-compatibility-plan.md](docs/python-compatibility-plan.md).

For Python programs outside the native subset, use the explicit compatibility
runtime:

```powershell
tarvos python path\to\program.py
```

This preserves broad Python execution through CPython. Use `compile`, `build`,
or `run` when you want native Tarvos compilation for supported code.

For an explicit hybrid attempt:

```powershell
tarvos run path\to\program.py --python-fallback
```

This tries native compilation first and falls back to CPython only when the
native subset rejects the program.

Native syntax and unsupported-AST diagnostics include the source line and
column. Use `--python-fallback` when CPython compatibility is required.

Tarvos is open-source software licensed under the [AGPL-3.0](LICENSE).
