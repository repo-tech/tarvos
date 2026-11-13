# Tarvos v1.2 Installation

Tarvos is a hybrid Python-to-Rust compiler. Statically supported Python is
translated to a native Rust executable; unsupported or dynamic behavior uses
the CPython compatibility path.

## Requirements

- Windows, Linux, or macOS
- Python 3.10 or newer
- Rust stable with `rustup`, `cargo`, and `rustc`
- A native Rust target supported by the host toolchain

Python's `ast` module is part of the standard library. No separate `ast`
package is required.

## Fresh checkout

```powershell
git clone <repository-url>
cd tarvos
```

## Install and verify Rust

```powershell
rustup toolchain install stable --profile minimal
rustup default stable
rustup component add rustfmt clippy
rustup target list --installed
rustc --version
cargo --version
```

On Windows, the stable MSVC target is recommended:

```powershell
rustup target add x86_64-pc-windows-msvc
```

On Linux or macOS, install the host target reported by
`rustc -vV | Select-String host` (PowerShell) or `rustc -vV | grep host`.

## Python environment

Create an isolated environment for optional benchmark tooling:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
```

The compiler itself uses only Python's standard library, including `ast`.
Optional NumPy/Pandas workloads may install their own dependencies:

```powershell
python -m pip install numpy pandas
```

## Build the production binary

Tarvos v1.2 uses the release profile with optimization level 3, thin LTO,
panic abort, and symbol stripping:

```powershell
cargo build --release --bin tarvos
```

The resulting Windows executable is:

```text
target\release\tarvos.exe
```

For a direct compiler invocation with the same production flags:

```powershell
rustc -C opt-level=3 -C strip=symbols -C panic=abort -o target\release\tarvos.exe path\to\generated.rs
```

## Verify the installation

```powershell
.\target\release\tarvos.exe doctor
.\target\release\tarvos.exe validate
.\target\release\tarvos.exe run examples\hello.py
```

Native translation cache files are stored under `.tarvos_cache/` and are
ignored by Git. Remove that directory when troubleshooting stale generated
output.

## Development checks

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --bin tarvos
```
