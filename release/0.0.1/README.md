# Tarvos 0.0.1 Release Bundle

This bundle is the clean delivery package for the Tarvos subset compiler release.

## Scope

Tarvos is not a general-purpose Python interpreter. It is a deterministic Python-to-Rust subset compiler focused on static numerical workloads, arithmetic kernels, and loop-heavy compute.

## Installation

Source-only mode (Rust not required):

```bash
cargo build --release --bin tarvos
./target/release/tarvos compile examples/simple.py output.rs --source-only
```

Native EXE mode (Rust required for final binary):

```bash
cargo build --release --bin tarvos
./target/release/tarvos build examples/simple.py app.exe
```

PowerShell:

```powershell
cargo build --release --bin tarvos
.\target\x86_64-pc-windows-msvc\release\tarvos.exe compile .\examples\simple.py .\output.rs --source-only
.\target\x86_64-pc-windows-msvc\release\tarvos.exe build .\examples\simple.py .\app.exe
```

## Release notes

See [CHANGELOG.md](./CHANGELOG.md) for the versioned feature and validation summary.

## Validation

```bash
python benchmarks/benchmark_matrix.py --repeats 3 --runtime all
```

The matrix benchmarks a dynamic `BENCH_LIMIT` / `BENCH_BIAS` workload so the results are not trivially constant-folded by the compiler toolchain.

## Supported runtime matrix

- CPython
- PyPy (if installed)
- Numba (if installed)
- Nuitka (if installed)
- Codon (if installed)
- Tarvos
- Rust reference

## Fairness check

```bash
g++ -O3 -march=native -std=c++17 dummy_no_fold.cpp -o dummy_no_fold_out
rustc -O -C target-cpu=native benchmarks/workloads/fair_no_fold.rs -o fair_no_fold_rust
BENCH_LIMIT=10000000 BENCH_BIAS=0 ./dummy_no_fold_out
BENCH_LIMIT=10000000 BENCH_BIAS=0 ./fair_no_fold_rust
```

Expected output: `50000005000000`

## Release checklist

- build succeeds
- CLI runs on the supported subset
- runtime matrix script executes successfully for available toolchains
- same-output validation passes for the dynamic fairness workload
- security hardening is in place for external command execution and PATH handling
