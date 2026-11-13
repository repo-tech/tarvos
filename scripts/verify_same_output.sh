#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

GPP_BIN="${GPP_BIN:-$ROOT/dummy_no_fold_out}"
RUST_BIN="${RUST_BIN:-$ROOT/fair_no_fold_rust}"

# 1) Build the fair no-fold C++ reference.
g++ -O3 -march=native -std=c++17 dummy_no_fold.cpp -o "$GPP_BIN"

# 2) Compile the equivalent fair Rust reference directly.
rustc -O -C target-cpu=native benchmarks/workloads/fair_no_fold.rs -o "$RUST_BIN"

# 3) Compare exact stdout using runtime-provided limits so the loops are not optimized away.
CPP_OUT=$(BENCH_LIMIT=10000000 BENCH_BIAS=0 "$GPP_BIN")
RUST_OUT=$(BENCH_LIMIT=10000000 BENCH_BIAS=0 "$RUST_BIN")

printf 'C++ stdout: %s\n' "$CPP_OUT"
printf 'Rust stdout: %s\n' "$RUST_OUT"

if [[ "$CPP_OUT" == "$RUST_OUT" ]]; then
  printf '\nstdout match: PASS\n'
else
  printf '\nstdout match: FAIL\n'
  diff -u <(printf '%s\n' "$CPP_OUT") <(printf '%s\n' "$RUST_OUT") || true
  exit 1
fi

# Optional: ensure the value itself is the exact expected integer.
[[ "$CPP_OUT" == "50000005000000" ]] && printf 'expected integer: PASS\n' || { printf 'expected integer: FAIL\n'; exit 1; }
