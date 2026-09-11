#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ ! -f "$ROOT/target/release/tarvos" && ! -f "$ROOT/target/release/tarvos.exe" ]]; then
  cargo build --release --bin tarvos
fi

python benchmarks/benchmark_matrix.py --repeats 3 --runtime all

GPP_BIN="$ROOT/dummy_no_fold_out"
RUST_BIN="$ROOT/fair_no_fold_rust"

g++ -O3 -march=native -std=c++17 dummy_no_fold.cpp -o "$GPP_BIN"
rustc -O -C target-cpu=native benchmarks/workloads/fair_no_fold.rs -o "$RUST_BIN"

CPP_OUT=$(BENCH_LIMIT=10000000 BENCH_BIAS=0 "$GPP_BIN")
RUST_OUT=$(BENCH_LIMIT=10000000 BENCH_BIAS=0 "$RUST_BIN")

printf 'C++ stdout: %s\n' "$CPP_OUT"
printf 'Rust stdout: %s\n' "$RUST_OUT"

[[ "$CPP_OUT" == "$RUST_OUT" ]] || { echo 'same-output: FAIL'; exit 1; }
[[ "$CPP_OUT" == "50000005000000" ]] || { echo 'expected integer: FAIL'; exit 1; }

echo 'same-output: PASS'
echo 'expected integer: PASS'
