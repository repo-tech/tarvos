"""Reproducible build-time and binary-size measurement for Tarvos.

Measures, separately and without combining unrelated quantities:

  A. transpile time    Python -> Rust source
  B. Cargo build time  generated Rust -> native executable
  C. total build time  A + B
  D. native runtime    the compiled program's own execution
  E. binary size       raw bytes, plus a compressed figure

Each timing is repeated so a median and spread can be reported instead of one
noisy sample. Every result records the machine, tool versions, and the exact
workload, because a size or speed number without those is not reproducible.

    python benchmarks/size_benchmark.py
    python benchmarks/size_benchmark.py --json benchmarks/size_report.json
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE_SUFFIX = ".exe" if os.name == "nt" else ""
CLI = ROOT / "target" / "release" / f"tarvos{EXE_SUFFIX}"
REPEATS = 3

WORKLOADS = {
    "hello": 'print("hello from tarvos")\n',
    "arithmetic": """
total = 0
for i in range(100000):
    total = total + i * 2
print(total)
""",
    "collections": """
xs = []
for i in range(1000):
    xs.append(i * i)
total = 0
for v in xs:
    total = total + v
print(total, len(xs))
""",
    "strings": """
parts = []
for i in range(200):
    parts.append(str(i))
joined = "-".join(parts)
print(len(joined), joined[0], joined[-1])
""",
}


def tool_version(command: list[str]) -> str:
    try:
        proc = subprocess.run(command, capture_output=True, text=True, timeout=60)
        return (proc.stdout or proc.stderr).strip().splitlines()[0]
    except Exception as error:  # noqa: BLE001 - reporting only
        return f"unavailable ({error})"


def environment() -> dict:
    return {
        "os": platform.system(),
        "os_release": platform.release(),
        "machine": platform.machine(),
        "python": sys.version.split()[0],
        "cargo": tool_version(["cargo", "--version"]),
        "rustc": tool_version(["rustc", "--version"]),
        "profile": "release (opt-level=z, lto, codegen-units=1, strip)",
    }


def transpile(source: pathlib.Path, out: pathlib.Path) -> float:
    start = time.perf_counter()
    proc = subprocess.run(
        [str(CLI), "compile", str(source), "-o", str(out)],
        capture_output=True, text=True, timeout=300,
    )
    elapsed = time.perf_counter() - start
    if proc.returncode != 0:
        raise RuntimeError(f"compile failed: {proc.stderr.strip()[:200]}")
    return elapsed


def cargo_build(work: pathlib.Path) -> tuple[float, pathlib.Path]:
    """Build the generated Rust and return (seconds, binary path)."""
    binary = work / "target" / "release" / f"bench{EXE_SUFFIX}"
    start = time.perf_counter()
    proc = subprocess.run(
        ["cargo", "build", "--release", "--offline"],
        cwd=work, capture_output=True, text=True, timeout=900,
    )
    elapsed = time.perf_counter() - start
    if proc.returncode != 0:
        raise RuntimeError(f"cargo build failed: {proc.stderr.strip()[-300:]}")
    return elapsed, binary


def run_native(binary: pathlib.Path, cwd: pathlib.Path) -> float:
    start = time.perf_counter()
    proc = subprocess.run([str(binary)], cwd=cwd, capture_output=True,
                          text=True, timeout=300)
    elapsed = time.perf_counter() - start
    if proc.returncode != 0:
        raise RuntimeError(f"native run failed: {proc.stderr.strip()[:200]}")
    return elapsed


def stats(samples: list[float]) -> dict:
    return {
        "median_ms": round(statistics.median(samples) * 1000, 2),
        "min_ms": round(min(samples) * 1000, 2),
        "max_ms": round(max(samples) * 1000, 2),
        "runs": len(samples),
    }


def measure(name: str, source_text: str) -> dict:
    work = pathlib.Path(tempfile.mkdtemp(prefix=f"tarvos-size-{name}-"))
    try:
        source = work / "program.py"
        source.write_text(source_text, encoding="utf-8")
        rust = work / "main.rs"

        transpile_times = [transpile(source, rust) for _ in range(REPEATS)]
        # A manifest is needed so the generated crate is standalone. The empty
        # [workspace] stops Cargo from attaching it to the Tarvos workspace.
        (work / "Cargo.toml").write_text(
            "[workspace]\n\n"
            "[package]\n"
            'name = "bench"\n'
            'version = "0.0.0"\n'
            'edition = "2021"\n\n'
            "[[bin]]\n"
            'name = "bench"\n'
            'path = "main.rs"\n',
            encoding="utf-8",
        )
        # The Cargo build is measured once and reused for size and runtime: a
        # repeat would mostly measure the incremental cache, not the build.
        build_time, binary = cargo_build(work)
        raw_bytes = binary.stat().st_size
        run_times = [run_native(binary, work) for _ in range(REPEATS)]

        try:
            import zlib
            compressed = len(zlib.compress(binary.read_bytes(), 9))
        except Exception:  # noqa: BLE001 - reporting only
            compressed = -1

        return {
            "workload": name,
            "transpile": stats(transpile_times),
            "cargo_build_ms": round(build_time * 1000, 2),
            "total_build_ms": round(
                (statistics.median(transpile_times) + build_time) * 1000, 2
            ),
            "native_runtime": stats(run_times),
            "binary_bytes": raw_bytes,
            "binary_kib": round(raw_bytes / 1024, 1),
            "compressed_bytes": compressed,
        }
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", default="", help="write a JSON report here")
    args = parser.parse_args()

    if not CLI.exists():
        print(f"Release CLI not built: {CLI}", file=sys.stderr)
        print("Run: cargo build --release -p tarvos-cli", file=sys.stderr)
        return 2

    report = {
        "tarvos_version": tool_version([str(CLI), "--version"]),
        "environment": environment(),
        "methodology": (
            "Transpile and native runtime are repeated and reported as "
            "median/min/max. The Cargo build is measured once because a repeat "
            "mostly measures the incremental cache. Binary size is the on-disk "
            "artifact; the compressed figure is zlib level 9 and is NOT a tarball "
            "or installer size. No competitor is measured here: comparing against "
            "a bundler that embeds a Python runtime is not an equivalent artifact."
        ),
        "results": [],
    }

    env = report["environment"]
    print(f"Tarvos: {report['tarvos_version']}")
    print(f"Host  : {env['os']} {env['os_release']} / {env['machine']}")
    print(f"Tool  : {env['cargo']} | {env['rustc']} | Python {env['python']}")
    print("-" * 92)
    print(f"{'workload':<14}{'transpile':>11}{'cargo':>10}{'total':>10}"
          f"{'runtime':>11}{'binary':>12}{'zlib9':>11}")

    for name, source in WORKLOADS.items():
        result = measure(name, source)
        report["results"].append(result)
        print(f"{name:<14}"
              f"{result['transpile']['median_ms']:>10.1f}m"
              f"{result['cargo_build_ms']:>9.0f}m"
              f"{result['total_build_ms']:>9.0f}m"
              f"{result['native_runtime']['median_ms']:>10.2f}m"
              f"{result['binary_kib']:>10.1f}K"
              f"{result['compressed_bytes'] / 1024:>9.1f}K")

    print("-" * 92)
    print("m = milliseconds. binary = raw artifact on disk.")
    print("zlib9 is a compression figure, not a distributable package size.")
    print("No competitor comparison: see the methodology note in the JSON report.")

    if args.json:
        pathlib.Path(args.json).write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8"
        )
        print(f"wrote {args.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
