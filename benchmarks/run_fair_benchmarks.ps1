# Tarvos Production Fair Benchmark Runner (Windows PowerShell)
param (
    [string]$Workload = "all",
    [int]$Iterations = 10,
    [int]$Warmup = 3
)

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  TARVOS PRODUCTION FAIR BENCHMARK HARNESS" -ForegroundColor Cyan
Write-Host "  Iterations: $Iterations | Warm-up: $Warmup | Target: Native Rust" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

# Check dependencies
$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    Write-Error "Python 3 is required to run benchmarks."
    exit 1
}

# Run benchmark script
python benchmarks/run_benchmarks.py

Write-Host "`n[✓] Fair benchmark suite execution complete!" -ForegroundColor Green
