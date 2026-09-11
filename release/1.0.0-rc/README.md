# Tarvos 1.0.0 Release Candidate

## Release gate

This bundle is for validating the 1.0 compiler contract before stable release.

```powershell
cargo test --workspace
cargo build --release --bin tarvos
python scripts\diff_test.py --all
python benchmarks\benchmark_matrix.py --runtime all --repeats 3
```

## Developer install

From the repository root:

```powershell
cargo build --release --bin tarvos
cargo run --release --bin tarvos -- install
tarvos doctor
```

Source-only compilation does not require Rust for the user workload:

```powershell
tarvos compile .\examples\simple.py --output .\output.rs --source-only
```

Native executable mode requires Rust:

```powershell
tarvos build .\examples\simple.py --output .\app.exe
```

## External benchmark files

Benchmark paths are resolved from the caller's current directory, so this works
from any folder:

```powershell
tarvos benchmark C:\work\dummy.py C:\work\dummy.rs
```

The CLI stages read-only benchmark inputs inside its controlled workspace,
executes the benchmark, and removes the staging directory afterward.

## Fairness rules

- report execution runtime separately from transpile and Rust compilation time
- use runtime-fed workload bounds for no-fold comparisons
- compare identical output before comparing timing
- never claim a universal Python speedup from a single workload

## 1.0 acceptance

- workspace tests pass
- every supported workload matches CPython output
- unsupported constructs fail clearly
- benchmark output distinguishes warm execution from cold-start compilation
- source-only and native build flows are both documented

## Trusted-input and packaging policy

Tarvos is distributed as a native CLI for trusted local workloads. Release
artifacts must be built per target by CI and tested on that target; binaries
must not be copied between operating systems or CPU architectures. The
compatibility runtime executes CPython and is not a sandbox. Use a container,
restricted account, and a minimal environment when processing untrusted source.

For a reproducible release check, record:

- target triple and OS version
- `tarvos --version`
- `rustc --version` and Python version
- artifact SHA-256
- validation and differential-test report paths
