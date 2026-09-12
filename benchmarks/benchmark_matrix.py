#!/usr/bin/env python3
"""Run a reproducible runtime matrix for CPython, Tarvos, and supported competitors.

This script benchmarks one deterministic workload shared by CPython, Tarvos, and
Rust. Keeping the workload inside Tarvos's supported static subset makes CI
failures represent real regressions instead of unsupported-language features.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH_ROOT = ROOT / "benchmarks"
WORKLOADS_DIR = BENCH_ROOT / "workloads"
SUPPORTED_PY = ROOT / "examples" / "simple.py"
FAIR_PY = WORKLOADS_DIR / "fair_no_fold.py"
NUMBA_WRAPPER = WORKLOADS_DIR / "numba_fair.py"
FAIR_RS = WORKLOADS_DIR / "fair_no_fold.rs"


def run_checked(cmd, **kwargs):
    proc = subprocess.run(cmd, text=True, capture_output=True, **kwargs)
    if proc.returncode != 0:
        raise subprocess.CalledProcessError(proc.returncode, cmd, output=proc.stdout, stderr=proc.stderr)
    return proc


def detect_runtime_status():
    sys.path.insert(0, str(ROOT))
    from benchmarks.run_benchmarks import detect_runtime_status as detector

    return detector()


def measure_once(cmd):
    start = time.perf_counter()
    proc = run_checked(cmd, cwd=str(ROOT), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
    elapsed = time.perf_counter() - start
    return elapsed, proc.stdout.strip()


def summarize(runtime, samples, output, compile_s=None, warmup_s=None):
    values_ms = [sample * 1000.0 for sample in samples]
    return {
        "runtime": runtime,
        "compile_s": compile_s,
        "warmup_s": None if warmup_s is None else warmup_s,
        "execution_ms": {
            "samples": values_ms,
            "median": statistics.median(values_ms),
            "minimum": min(values_ms),
            "maximum": max(values_ms),
            "standard_deviation": statistics.stdev(values_ms) if len(values_ms) > 1 else 0.0,
        },
        "output": output,
    }


def execute_with_warmup(command, cwd, env, repeats):
    warmup_start = time.perf_counter()
    warmup = run_checked(command, cwd=cwd, env=env)
    warmup_s = time.perf_counter() - warmup_start
    samples = []
    output = warmup.stdout.strip()
    for _ in range(repeats):
        start = time.perf_counter()
        result = run_checked(command, cwd=cwd, env=env)
        samples.append(time.perf_counter() - start)
        if result.stdout.strip() != output:
            raise RuntimeError(f"non-deterministic output from {' '.join(map(str, command))}")
    return samples, output, warmup_s


def benchmark_python(script: Path, repeats: int):
    env = {**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"}
    samples, output, warmup_s = execute_with_warmup([sys.executable, str(script)], str(script.parent), env, repeats)
    return summarize("cpython", samples, output, warmup_s=warmup_s)


def benchmark_pypy(script: Path, repeats: int):
    pypy = shutil.which("pypy3") or shutil.which("pypy")
    if not pypy:
        return {"runtime": "pypy", "status": "unavailable", "reason": "PyPy not installed"}
    samples = []
    for _ in range(repeats):
        start = time.perf_counter()
        run_checked([pypy, str(script)], cwd=str(script.parent), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
        samples.append(time.perf_counter() - start)
    return {"runtime": "pypy", "avg_s": sum(samples) / len(samples), "samples_ms": [s * 1000.0 for s in samples]}


def benchmark_numba(script: Path, repeats: int):
    try:
        import numba  # noqa: F401
    except Exception as exc:
        return {"runtime": "numba", "status": "unavailable", "reason": str(exc)}
    samples = []
    for _ in range(repeats):
        start = time.perf_counter()
        run_checked([sys.executable, str(script)], cwd=str(script.parent), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
        samples.append(time.perf_counter() - start)
    return {"runtime": "numba", "avg_s": sum(samples) / len(samples), "samples_ms": [s * 1000.0 for s in samples]}


def benchmark_nuitka(script: Path, repeats: int):
    nuitka = shutil.which("nuitka")
    if not nuitka:
        return {"runtime": "nuitka", "status": "unavailable", "reason": "Nuitka not installed"}
    with subprocess.TemporaryDirectory() as tmpdir:
        outdir = Path(tmpdir)
        compile_cmd = [nuitka, str(script), "--output-dir", str(outdir), "--assume-yes", "--remove-output"]
        run_checked(compile_cmd, cwd=str(script.parent), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
        exe = next(iter(outdir.rglob(script.stem + "*")), None)
        if exe is None:
            return {"runtime": "nuitka", "status": "failed", "reason": "No Nuitka executable was generated"}
        samples = []
        for _ in range(repeats):
            start = time.perf_counter()
            run_checked([str(exe)], cwd=str(script.parent), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
            samples.append(time.perf_counter() - start)
        return {"runtime": "nuitka", "avg_s": sum(samples) / len(samples), "samples_ms": [s * 1000.0 for s in samples]}


def benchmark_codon(script: Path, repeats: int):
    codon = shutil.which("codon")
    if not codon:
        return {"runtime": "codon", "status": "unavailable", "reason": "Codon not installed"}
    samples = []
    for _ in range(repeats):
        start = time.perf_counter()
        run_checked([codon, "run", str(script)], cwd=str(script.parent), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
        samples.append(time.perf_counter() - start)
    return {"runtime": "codon", "avg_s": sum(samples) / len(samples), "samples_ms": [s * 1000.0 for s in samples]}


def benchmark_tarvos(script: Path, repeats: int):
    root_bin = ROOT / "target" / "release" / ("tarvos.exe" if os.name == "nt" else "tarvos")
    if not root_bin.exists():
        root_bin = ROOT / "target" / "debug" / ("tarvos.exe" if os.name == "nt" else "tarvos")
    if not root_bin.exists():
        return {"runtime": "tarvos", "status": "unavailable", "reason": "Tarvos binary not built"}
    tmpdir = ROOT / ".build-tmp" / "matrix-run"
    tmpdir.mkdir(parents=True, exist_ok=True)
    out_rs = tmpdir / "matrix_bench.rs"
    out_bin = tmpdir / ("matrix_bench.exe" if os.name == "nt" else "matrix_bench")
    compile_start = time.perf_counter()
    run_checked([str(root_bin), "compile", str(script), "--output", str(out_rs)], cwd=str(ROOT), env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
    compile_s = time.perf_counter() - compile_start
    rust_compile_start = time.perf_counter()
    rustc = subprocess.run(["rustc", "-O", "-C", "target-cpu=native", "-o", str(out_bin), str(out_rs)], capture_output=True, text=True, env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
    if rustc.returncode != 0:
        return {"runtime": "tarvos", "status": "failed", "reason": rustc.stderr.strip() or rustc.stdout.strip()}
    compile_s += time.perf_counter() - rust_compile_start
    env = {**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"}
    samples, output, warmup_s = execute_with_warmup([str(out_bin)], str(ROOT), env, repeats)
    return summarize("tarvos", samples, output, compile_s=compile_s, warmup_s=warmup_s)


def benchmark_rust(script: Path, repeats: int):
    rustc_bin = shutil.which("rustc")
    if not script.exists():
        return {"runtime": "rust", "status": "unavailable", "reason": "Rust workload missing"}
    if not rustc_bin:
        return {"runtime": "rust", "status": "unavailable", "reason": "Rust toolchain not installed"}
    out_bin = ROOT / ".build-tmp" / "rust_matrix_bench"
    if os.name == "nt":
        out_bin = ROOT / ".build-tmp" / "rust_matrix_bench.exe"
    compile_cmd = [rustc_bin, "-O", "-C", "target-cpu=native", "-o", str(out_bin), str(script)]
    rust_compile_start = time.perf_counter()
    rustc = subprocess.run(compile_cmd, capture_output=True, text=True, env={**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"})
    if rustc.returncode != 0:
        return {"runtime": "rust", "status": "failed", "reason": rustc.stderr.strip() or rustc.stdout.strip()}
    compile_s = time.perf_counter() - rust_compile_start
    env = {**os.environ, "BENCH_LIMIT": "10000000", "BENCH_BIAS": "0"}
    samples, output, warmup_s = execute_with_warmup([str(out_bin)], str(ROOT), env, repeats)
    return summarize("rust", samples, output, compile_s=compile_s, warmup_s=warmup_s)


def main():
    parser = argparse.ArgumentParser(description="Run a reproducible fairness matrix for the subset compiler benchmark suite.")
    parser.add_argument("--repeats", type=int, default=5, help="Number of runs per runtime")
    parser.add_argument("--runtime", choices=["cpython", "pypy", "numba", "nuitka", "codon", "tarvos", "rust", "all"], default="all")
    parser.add_argument("--json-output", type=str, default=str(ROOT / "benchmarks" / "results" / "runtime_matrix.json"), help="JSON file to write")
    args = parser.parse_args()
    tarvos_binary = ROOT / "target" / ("release" if (ROOT / "target" / "release").exists() else "debug") / (
        "tarvos.exe" if os.name == "nt" else "tarvos"
    )
    toolchain = {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()
        if shutil.which("rustc")
        else "unavailable",
        "tarvos": subprocess.check_output([str(tarvos_binary), "--version"], text=True).strip()
        if tarvos_binary.exists()
        else "unavailable",
    }

    runtimes = ["cpython", "pypy", "numba", "nuitka", "codon", "tarvos", "rust"] if args.runtime == "all" else [args.runtime]
    rows = []
    for name in runtimes:
        if name == "cpython":
            rows.append({"workload": FAIR_PY.name, **benchmark_python(FAIR_PY, args.repeats)})
        elif name == "pypy":
            rows.append({"workload": SUPPORTED_PY.name, **benchmark_pypy(SUPPORTED_PY, args.repeats)})
        elif name == "numba":
            rows.append({"workload": NUMBA_WRAPPER.name, **benchmark_numba(NUMBA_WRAPPER, args.repeats)})
        elif name == "nuitka":
            rows.append({"workload": FAIR_PY.name, **benchmark_nuitka(FAIR_PY, args.repeats)})
        elif name == "codon":
            rows.append({"workload": SUPPORTED_PY.name, **benchmark_codon(SUPPORTED_PY, args.repeats)})
        elif name == "tarvos":
            rows.append({"workload": FAIR_PY.name, **benchmark_tarvos(FAIR_PY, args.repeats)})
        elif name == "rust":
            rows.append({"workload": FAIR_RS.name, **benchmark_rust(FAIR_RS, args.repeats)})

    path = Path(args.json_output)
    path.parent.mkdir(parents=True, exist_ok=True)
    available_outputs = {row["output"] for row in rows if row.get("output") is not None}
    output_comparison = {
        "status": "pass" if len(available_outputs) <= 1 else "fail",
        "outputs": sorted(available_outputs),
    }
    report = {
        "schema_version": 1,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": platform.platform(),
        "python": sys.version,
        "toolchain": toolchain,
        "workload": str(FAIR_PY.relative_to(ROOT)),
        "repeats": args.repeats,
        "warmup_runs": 1,
        "output_comparison": output_comparison,
        "rows": rows,
    }
    path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    for row in rows:
        if row.get("status") in {"unavailable", "failed"}:
            print(f"{row['runtime']}: {row.get('status')} - {row.get('reason', 'unknown reason')}")
        else:
            stats = row["execution_ms"]
            print(
                f"{row['runtime']}: median {stats['median']:.2f} ms "
                f"(min {stats['minimum']:.2f}, max {stats['maximum']:.2f}, "
                f"stdev {stats['standard_deviation']:.2f})"
            )

    print(f"Output comparison: {output_comparison['status']}")
    if output_comparison["status"] == "fail":
        raise SystemExit("Benchmark output parity failed")
    print(f"\nMatrix output written to: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
