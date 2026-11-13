#!/usr/bin/env python3
"""Install and verify the available competitive runtimes for benchmarking.

This script intentionally keeps the installation local to the repository so the
benchmark environment is reproducible without installing system-wide packages.

Supported flows:
- PyPy: download a portable Windows ZIP when available.
- Numba: pip install into a local virtual environment.
- Nuitka: pip install into a local virtual environment.
- Codon: use the official Exaloop installer only on Linux/macOS.

On Windows, Codon is known to be unsupported by the official installer and this
script reports that explicitly instead of pretending it was installed.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VENV_DIR = ROOT / ".bench-venv"
TOOLS_DIR = ROOT / "tools"
PYTHON_EXE = sys.executable


def safe_tool_roots():
    roots = [ROOT.resolve(), Path(sys.executable).resolve().parent]
    home = Path.home()
    for candidate in [
        home / ".cargo" / "bin",
        home / ".local" / "bin",
        VENV_DIR / ("Scripts" if os.name == "nt" else "bin"),
        TOOLS_DIR,
    ]:
        roots.append(candidate)
    return list(dict.fromkeys(roots))


def safe_env(base_env=None):
    env = (base_env or os.environ).copy()
    safe_roots = [str(root) for root in safe_tool_roots() if root.exists()]
    current = env.get("PATH", "").split(os.pathsep)
    merged = []
    for entry in safe_roots + current:
        if entry and entry not in merged:
            merged.append(entry)
    env["PATH"] = os.pathsep.join(merged)
    return env


def run(cmd, *, cwd=None, env=None):
    completed = subprocess.run(cmd, cwd=cwd, env=safe_env(env), text=True, capture_output=True)
    if completed.stdout:
        print(completed.stdout.rstrip())
    if completed.stderr:
        print(completed.stderr.rstrip(), file=sys.stderr)
    return completed.returncode


def ensure_venv():
    if not VENV_DIR.exists():
        print(f"[install] creating venv at {VENV_DIR}")
        rc = run([sys.executable, "-m", "venv", str(VENV_DIR)])
        if rc != 0:
            raise RuntimeError("Failed to create the benchmark virtual environment")
    py = VENV_DIR / ("Scripts" / "python.exe" if os.name == "nt" else Path("bin") / "python")
    if not py.exists():
        raise RuntimeError(f"Benchmark venv is missing Python: {py}")
    return py


def install_python_packages(py_exe: Path):
    packages = ["numba", "nuitka"]
    print("[install] installing Python packages:", ", ".join(packages))
    rc = run([str(py_exe), "-m", "pip", "install", *packages])
    if rc != 0:
        raise RuntimeError("Failed to install Numba/Nuitka into the benchmark venv")


def install_pypy():
    target = TOOLS_DIR / "pypy3.11-v7.3.23-win64"
    if target.exists():
        print(f"[install] PyPy already present at {target}")
        return target
    TOOLS_DIR.mkdir(parents=True, exist_ok=True)
    zip_path = TOOLS_DIR / "pypy3.11-v7.3.23-win64.zip"
    url = "https://downloads.python.org/pypy/pypy3.11-v7.3.23-win64.zip"
    print(f"[install] downloading PyPy from {url}")
    rc = run([sys.executable, "-c", f"import urllib.request; urllib.request.urlretrieve({url!r}, {str(zip_path)!r})"])
    if rc != 0 or not zip_path.exists():
        raise RuntimeError(f"Failed to download PyPy from {url}")
    rc = run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", f"Expand-Archive -Path '{zip_path}' -DestinationPath '{TOOLS_DIR}' -Force"])
    if rc != 0 or not target.exists():
        raise RuntimeError(f"Failed to extract PyPy to {target}")
    return target


def install_codon_linux():
    if os.name == "nt":
        print("[install] Codon is not supported on Windows; use WSL/Ubuntu and the official installer instead.")
        return False
    script = shutil.which("bash")
    if not script:
        print("[install] bash is required to install Codon from the official Exaloop script.")
        return False
    print("[install] running official Codon installer via bash")
    rc = run([script, "-lc", "curl -fsSL https://exaloop.io/install.sh | bash"])
    if rc != 0:
        print("[install] Codon installation via the official installer failed.")
        return False
    codon = shutil.which("codon")
    if codon:
        print(f"[install] Codon installed at {codon}")
        return True
    print("[install] Codon CLI was not found on PATH after installation.")
    return False


def verify_runtime_status():
    bin_dir = VENV_DIR / ("Scripts" if os.name == "nt" else "bin")
    env_path = os.environ.get("PATH", "")
    venv_path = str(bin_dir)
    if venv_path not in env_path.split(os.pathsep):
        os.environ["PATH"] = venv_path + os.pathsep + env_path
    checks = {
        "pypy": shutil.which("pypy3") is not None or shutil.which("pypy") is not None,
        "numba": subprocess.run([sys.executable, "-c", "import importlib; importlib.import_module('numba')"], capture_output=True, text=True).returncode == 0,
        "nuitka": shutil.which("nuitka") is not None or subprocess.run([sys.executable, "-c", "import importlib; importlib.import_module('nuitka')"], capture_output=True, text=True).returncode == 0,
        "codon": shutil.which("codon") is not None,
    }
    print("[verify] runtime availability:")
    for key, value in checks.items():
        print(f"  - {key}: {'yes' if value else 'no'}")
    return checks


def main():
    print(f"[install] repository root: {ROOT}")
    py_exe = ensure_venv()
    install_python_packages(py_exe)
    try:
        install_pypy()
    except RuntimeError as exc:
        print(f"[warn] {exc}")
    install_codon_linux()
    verify_runtime_status()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
