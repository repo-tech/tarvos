default:
    @echo "Available targets: bench-dev, bench-release, bench-cranelift, build-dev, build-release, check"

build-dev:
    cargo build --bin tarvos

build-release:
    cargo build --release --bin tarvos

check:
    cargo check --bin tarvos

bench-dev:
    python benchmarks/run_benchmarks.py --preset dev

bench-release:
    python benchmarks/run_benchmarks.py --preset release

bench-cranelift:
    python benchmarks/run_benchmarks.py --preset cranelift
