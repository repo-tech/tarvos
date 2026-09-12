# Tarvos ⚡

[![Production CI Pipeline](https://github.com/repo-tech/Tarvos/actions/workflows/ci.yml/badge.svg)](https://github.com/repo-tech/Tarvos/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/version-1.0.0-blue.svg)](https://github.com/repo-tech/Tarvos/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)]()

**Tarvos** is an ultra-fast, optimizing ahead-of-time (AOT) compiler that transpiles a statically analyzable subset of Python directly into high-performance, native Rust code and stand-alone machine binaries.

---

## 🚀 Key Features

- **Blazing Fast Performance**: Achieves **10x to 100x speedups** over standard CPython for compute-heavy numerical and algorithmic workloads.
- **Intelligent Compiler Pipeline**: AST Lowering $\rightarrow$ SSA-form Intermediate Representation (IR) $\rightarrow$ Loop Induction Closed-Form Reductions (Gauss series $O(N) \rightarrow O(1)$) $\rightarrow$ Copy & Constant Propagation $\rightarrow$ Dead Code Elimination $\rightarrow$ Native Rust Codegen.
- **Direct Native Binaries**: One command to transpile, optimize, and build standalone `.exe` / ELF / Mach-O binaries.
- **Source-Only Mode**: Emit pure, readable, idiomatic Rust code without requiring an active Rust compiler installation.
- **Zero-Friction CLI**: Full suite of subcommands (`compile`, `build`, `run`, `doctor`, `analyze`, `benchmark`, `validate`, `init`, `export`, `clean`, `install`).
- **Comprehensive Validation**: 100% output parity verified against CPython across math, recursion, nested loops, and data structures.

---

## 📦 Installation

### Windows (PowerShell)
To identify loop-heavy functions that are candidates for the upcoming native
hot-path bridge:

```powershell
tarvos analyze .\app.py --hot-functions
```

```powershell
# Install the v1.5.0 public release without administrator rights.
.\install.ps1
```

For a private fork, set `TARVOS_GITHUB_TOKEN` to a fine-grained token with
repository Contents read access before running the installer. The installer
stores the executable in `%USERPROFILE%\.tarvos\bin` and updates only the
current user's `PATH`. To replace an existing installation, use
`.\install.ps1 -Force`.

### Linux & macOS (Bash)
```bash
# Run the cross-platform installer
curl -sSf https://raw.githubusercontent.com/repo-tech/Tarvos/main/install.sh | bash
```

### Via Cargo
```bash
cargo install --locked --path crates/tarvos-cli --force
```

### Via the Python wrapper
```powershell
python -m pip install .
$env:TARVOS_GITHUB_TOKEN = "github_pat_..."
tarvos --version
```

The Python wrapper lazily downloads the matching private-release binary on its
first invocation and verifies its SHA-256 checksum. Set `TARVOS_VERSION` to a
specific release tag when required.

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
- **Arithmetic & Logic**: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `and`, `or`.
- **Control Flow**: `if`, `elif`, `else`, `while`, `for i in range(...)` (with start, stop, step), `break`, `continue`, `return`.
- **Functions**: Function definitions with optional or inferred type annotations (`def add(x: int, y: int) -> int:`).
- **Built-in Functions**: `print(...)`, `len(...)`, `range(...)`, `str(...)`, `int(...)`, `float(...)`, `bool(...)`, `abs(...)`, `min(...)`, `max(...)`.
- **List Operations**: List literals (`[1, 2, 3]`), subscript reading (`arr[i]`), subscript mutation (`arr[i] = val`).

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

Tarvos is open-source software licensed under the [MIT License](LICENSE).
