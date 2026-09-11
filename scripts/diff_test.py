#!/usr/bin/env python3
"""Tarvos differential test runner.

Runs CPython and Tarvos-generated Rust against the project workload set and
compares the final stdout.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "benchmarks" / "workloads" / "expected_outputs.json"
DEFAULT_REPORT = ROOT / "benchmarks" / "results" / "results.json"
BUILD_TMP_ROOT = ROOT / ".build-tmp"

PASS = "[PASS]"
FAIL = "[FAIL]"
SKIP = "[SKIP]"


def find_python() -> str:
    for cmd in ("python3", "python", "py"):
        try:
            r = subprocess.run([cmd, "--version"], capture_output=True, timeout=5)
            if r.returncode == 0:
                return cmd
        except FileNotFoundError:
            continue
    sys.exit("ERROR: Could not find a Python interpreter on PATH")


def load_expected_outputs(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        if isinstance(payload, dict):
            return {str(k): str(v) for k, v in payload.items()}
    except json.JSONDecodeError:
        pass
    return {}


def normalise(output: str) -> str:
    if output is None:
        return ""
    return "\n".join(line.rstrip() for line in output.strip().splitlines())


def resolve_binary(path_arg: str | None) -> Path:
    if path_arg:
        return Path(path_arg).expanduser().resolve()

    candidates = [
        ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / "tarvos.exe",
        ROOT / "target" / "x86_64-pc-windows-gnu" / "debug" / "tarvos.exe",
        ROOT / "target" / "release" / "tarvos.exe",
        ROOT / "target" / "release" / "tarvos",
        ROOT / "target" / "debug" / "tarvos.exe",
        ROOT / "target" / "debug" / "tarvos",
        Path("target") / "release" / "tarvos.exe",
        Path("target") / "release" / "tarvos",
        Path("target") / "debug" / "tarvos.exe",
        Path("target") / "debug" / "tarvos",
    ]
    existing = [c for c in candidates if c.exists()]
    if not existing:
        raise FileNotFoundError(
            "Could not find tarvos binary. Run: cargo build --bin tarvos"
        )
    existing.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return existing[0]


def run_python(python: str, script: Path) -> tuple[int, str, str]:
    r = subprocess.run([python, str(script)], capture_output=True, text=True, timeout=30)
    return r.returncode, r.stdout, r.stderr


def run_tarvos(binary: Path, script: Path, tmpdir: str) -> tuple[int, str, str]:
    rs_path = Path(tmpdir) / "epy_out.rs"
    exe_path = Path(tmpdir) / "epy_out.exe" if sys.platform == "win32" else Path(tmpdir) / "epy_out"

    r = subprocess.run(
        [str(binary), "compile", str(script), "--output", str(rs_path)],
        capture_output=True,
        text=True,
        timeout=30,
        cwd=str(ROOT),
    )
    if r.returncode != 0:
        return r.returncode, "", r.stderr + r.stdout

    rustc_args = ["rustc"]
    if sys.platform == "win32":
        rustc_args.append("+stable-x86_64-pc-windows-gnu")
    rustc_args.extend([str(rs_path), "-o", str(exe_path)])

    r2 = subprocess.run(rustc_args, capture_output=True, text=True, timeout=60)
    if r2.returncode != 0:
        return r2.returncode, "", f"rustc error:\n{r2.stderr}"

    r3 = subprocess.run([str(exe_path)], capture_output=True, text=True, timeout=30)
    return r3.returncode, r3.stdout, r3.stderr


def discover_targets(directory: Path, include_all: bool, known_files: set[str]) -> list[Path]:
    if include_all:
        return sorted(directory.glob("*.py"))
    return [directory / name for name in sorted(known_files) if (directory / name).exists()]


def main():
    parser = argparse.ArgumentParser(description="Tarvos differential test runner")
    parser.add_argument("--workloads-dir", default=str(DEFAULT_MANIFEST.parent), help="Directory containing workload .py files")
    parser.add_argument("--binary", default=None, help="Path to tarvos binary")
    parser.add_argument("--all", action="store_true", help="Test every .py file in the workload directory")
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST), help="Path to JSON expected-output manifest")
    parser.add_argument("--json-output", default=str(DEFAULT_REPORT), help="Write summary JSON to this path")
    args = parser.parse_args()

    workload_dir = Path(args.workloads_dir).expanduser().resolve()
    if not workload_dir.exists():
        sys.exit(f"ERROR: workload directory not found: {workload_dir}")

    manifest_path = Path(args.manifest).expanduser().resolve()
    expected_outputs = load_expected_outputs(manifest_path)
    known_files = set(expected_outputs.keys()) or {p.name for p in workload_dir.glob("*.py")}

    try:
        binary = resolve_binary(args.binary)
    except FileNotFoundError as exc:
        sys.exit(f"ERROR: {exc}")

    python = find_python()
    targets = discover_targets(workload_dir, args.all, known_files)
    if not targets:
        sys.exit(f"No Python workload files found in {workload_dir}")

    print(f"\nTarvos Differential Test Runner")
    print(f"{'=' * 60}")
    print(f"  Python:        {python}")
    print(f"  Binary:        {binary}")
    print(f"  Workloads:     {workload_dir}")
    print(f"  Manifest:      {manifest_path}")
    print(f"  Test count:    {len(targets)}")
    print(f"{'=' * 60}\n")

    results = []
    BUILD_TMP_ROOT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tarvos_diff_", dir=str(BUILD_TMP_ROOT)) as tmpdir:
        for script in targets:
            name = script.name
            expected = expected_outputs.get(name, "")
            t0 = time.monotonic()
            source = script.read_text(encoding="utf-8")
            if any(line.lstrip().startswith(("import ", "from ")) for line in source.splitlines()):
                print(f"  {SKIP}  {name}  (uses imports outside the Tarvos native subset)")
                results.append({"name": name, "status": "skip", "reason": "imports are outside the supported subset"})
                continue

            py_rc, py_out, py_err = run_python(python, script)
            if py_rc != 0:
                print(f"  {SKIP}  {name}  (Python failed: {py_err.strip()[:80]})")
                results.append({"name": name, "status": "skip", "reason": py_err.strip()})
                continue

            try:
                ep_rc, ep_out, ep_err = run_tarvos(binary, script, tmpdir)
            except subprocess.TimeoutExpired:
                print(f"  {FAIL}  {name}  TIMEOUT")
                results.append({"name": name, "status": "fail", "reason": "timeout"})
                continue
            except Exception as exc:
                print(f"  {FAIL}  {name}  exception: {exc}")
                results.append({"name": name, "status": "fail", "reason": str(exc)})
                continue

            elapsed = time.monotonic() - t0
            if ep_rc != 0:
                print(f"  {FAIL}  {name}  (tarvos/rustc error)")
                print(f"         {ep_err.strip()[:200]}")
                results.append({"name": name, "status": "fail", "reason": ep_err.strip()})
                continue

            py_norm = normalise(py_out)
            ep_norm = normalise(ep_out)
            if expected:
                expected_norm = normalise(expected)
            else:
                expected_norm = py_norm

            if ep_norm == expected_norm:
                print(f"  {PASS}  {name}  ({elapsed:.2f}s)")
                results.append({
                    "name": name,
                    "status": "pass",
                    "python_stdout": py_norm,
                    "tarvos_stdout": ep_norm,
                    "expected_stdout": expected_norm,
                })
            else:
                print(f"  {FAIL}  {name}  (output mismatch)")
                print(f"         Python:     {repr(py_norm[:120])}")
                print(f"         Expected:   {repr(expected_norm[:120])}")
                print(f"         Tarvos: {repr(ep_norm[:120])}")
                results.append({
                    "name": name,
                    "status": "fail",
                    "reason": "output mismatch",
                    "python_stdout": py_norm,
                    "tarvos_stdout": ep_norm,
                    "expected_stdout": expected_norm,
                })

    passed = sum(1 for r in results if r.get("status") == "pass")
    failed = sum(1 for r in results if r.get("status") == "fail")
    skipped = sum(1 for r in results if r.get("status") == "skip")

    summary = {
        "project": "Tarvos",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "workload_directory": str(workload_dir),
        "total": len(results),
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "results": results,
    }

    report_path = Path(args.json_output).expanduser().resolve()
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    print(f"\n{'=' * 60}")
    print(f"Results: {passed} passed, {failed} failed, {skipped} skipped")
    print(f"Report:  {report_path}")
    print(f"{'=' * 60}\n")

    if failed > 0:
        print("FAILED TESTS:")
        for item in results:
            if item.get("status") == "fail":
                print(f"  - {item['name']}")
        sys.exit(1)

    print("All workload tests passed [OK]")
    sys.exit(0)


if __name__ == "__main__":
    main()
