#!/usr/bin/env python3
"""Detect the availability of common Python acceleration runtimes and compilers.

This script intentionally reports availability without trying to install missing
packages. That keeps the project honest in constrained environments while still
providing a reproducible verification flow for PyPy, Numba, Nuitka, Codon, and
Rust toolchains.
"""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT_PATH = ROOT / "benchmarks" / "results" / "competitor_environment.json"


def run_cmd(command):
    try:
        proc = subprocess.run(command, capture_output=True, text=True, timeout=20)
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except FileNotFoundError:
        return 127, "", ""
    except subprocess.TimeoutExpired:
        return 124, "", "timed out"


def detect_python_module(module_name: str):
    code = (
        "import importlib; "
        f"module = importlib.import_module({module_name!r}); "
        "print(getattr(module, '__version__', 'unknown'))"
    )
    cmd = [sys.executable, "-c", code]
    rc, out, err = run_cmd(cmd)
    if rc == 0:
        return {"available": True, "version": out or "unknown", "source": "python-module"}
    return {"available": False, "version": None, "source": "python-module", "error": err or "module not importable"}


def detect_binary(name: str, version_args):
    candidates = []
    for candidate in [name, f"{name}.exe", f"{name}.cmd", f"{name}.bat"]:
        resolved = shutil.which(candidate)
        if resolved:
            candidates.append(resolved)
    venv_bin = Path(sys.executable).resolve().parent
    if venv_bin.exists():
        for suffix in ["", ".exe", ".cmd", ".bat"]:
            candidate = venv_bin / f"{name}{suffix}"
            if candidate.exists():
                candidates.append(str(candidate))
    candidates = list(dict.fromkeys(candidates))
    path = candidates[0] if candidates else None
    if not path:
        return {"available": False, "path": None, "version": None, "reason": "not on PATH"}
    rc, out, err = run_cmd([path, *version_args])
    if rc == 0:
        version = (out or err).splitlines()[0] if (out or err) else "unknown"
        return {"available": True, "path": path, "version": version, "reason": "ok"}
    return {"available": False, "path": path, "version": None, "reason": err or out or "version check failed"}


def detect_rust():
    info = detect_binary("rustc", ["--version"])
    if info["available"]:
        cargo = detect_binary("cargo", ["--version"])
        info["cargo"] = cargo
    else:
        info["cargo"] = {"available": False, "path": None, "version": None, "reason": "rustc missing"}
    return info


def main():
    parser = argparse.ArgumentParser(description="Check the environment for Python acceleration tools")
    parser.add_argument("--json-output", default=str(OUTPUT_PATH), help="Write the detection report to this JSON file")
    args = parser.parse_args()

    report = {
        "python": {"executable": sys.executable, "version": sys.version.split()[0]},
        "pypy": detect_binary("pypy3", ["--version"]),
        "numba": detect_python_module("numba"),
        "nuitka": detect_binary("nuitka", ["--version"]),
        "codon": detect_binary("codon", ["--version"]),
        "rust": detect_rust(),
    }

    report_path = Path(args.json_output).expanduser().resolve()
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
