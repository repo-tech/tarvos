#!/usr/bin/env bash
set -euo pipefail

echo "============================================================"
echo "  TARVOS PRODUCTION FAIR BENCHMARK HARNESS (Linux/macOS)"
echo "============================================================"

if ! command -v python3 &> /dev/null; then
    echo "[!] Python 3 is required."
    exit 1
fi

python3 benchmarks/run_benchmarks.py

echo ""
echo "[✓] Fair benchmark suite execution complete!"
