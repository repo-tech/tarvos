"""Acceptance test: compile a real multi-module Python project natively.

Builds a fixture project (entry, sibling module, package with `__init__` and a
submodule, pyproject.toml), compiles it with `tarvos package`, runs the emitted
native executable, and compares its stdout with CPython. Also checks that
`tarvos analyze` and `tarvos scan` accept the project.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE_SUFFIX = ".exe" if os.name == "nt" else ""
CLI = ROOT / "target" / "debug" / f"tarvos{EXE_SUFFIX}"

MAIN = """from package.math_utils import sum_all, average
from helpers import describe


def main():
    values = [3, 1, 4, 1, 5]
    total = sum_all(values)
    print(describe(total))
    print(average(values))


main()
"""

HELPERS = """def describe(value):
    return "total=" + str(value)
"""

INIT = """from .math_utils import sum_all
"""

MATH_UTILS = """def sum_all(values):
    total = 0
    for v in values:
        total = total + v
    return total


def average(values):
    return sum_all(values) / len(values)
"""

PYPROJECT = '[project]\nname = "sample-project"\nversion = "0.1.0"\n'


def build_fixture(base: pathlib.Path) -> pathlib.Path:
    project = base / "sample-project"
    (project / "package").mkdir(parents=True)
    (project / "main.py").write_text(MAIN, encoding="utf-8")
    (project / "helpers.py").write_text(HELPERS, encoding="utf-8")
    (project / "pyproject.toml").write_text(PYPROJECT, encoding="utf-8")
    (project / "package" / "__init__.py").write_text(INIT, encoding="utf-8")
    (project / "package" / "math_utils.py").write_text(MATH_UTILS, encoding="utf-8")
    return project


def main() -> int:
    if not CLI.exists():
        print(f"CLI not built: {CLI}", file=sys.stderr)
        return 2

    base = pathlib.Path(tempfile.mkdtemp(prefix="tarvos-project-"))
    checks: list[tuple[str, bool, str]] = []

    def check(name, ok, detail=""):
        checks.append((name, bool(ok), detail))
        print(f"[{'PASS' if ok else 'FAIL':4s}] {name}"
              + (f"\n        {detail}" if detail else ""), flush=True)

    try:
        project = build_fixture(base)
        entry = project / "main.py"

        # The reference behaviour is CPython's.
        reference = subprocess.run([sys.executable, str(entry)],
                                   capture_output=True, text=True, timeout=120)
        check("cpython reference runs", reference.returncode == 0,
              reference.stderr.strip()[:160])

        for command in ("analyze", "scan"):
            proc = subprocess.run([str(CLI), command, str(project)],
                                  capture_output=True, text=True, timeout=300)
            check(f"tarvos {command} <project>", proc.returncode == 0,
                  f"exit={proc.returncode} {proc.stderr.strip()[:120]}")

        out_dir = base / "sample-rust"
        proc = subprocess.run(
            [str(CLI), "package", str(project), "-o", str(out_dir),
             "--entry", "main.py"],
            capture_output=True, text=True, timeout=900)
        packaged = proc.returncode == 0
        check("tarvos package <project>", packaged,
              f"exit={proc.returncode} {proc.stderr.strip()[-200:]}")

        artifact = out_dir / "dist" / "sample_rust.exe"
        if os.name != "nt":
            artifact = out_dir / "dist" / "sample_rust"
        if packaged and artifact.exists():
            native = subprocess.run([str(artifact)], capture_output=True,
                                    text=True, timeout=120)
            check("native artifact runs", native.returncode == 0,
                  f"exit={native.returncode}")
            check("native stdout matches CPython",
                  native.stdout == reference.stdout,
                  f"CPython={reference.stdout!r}\n        native ={native.stdout!r}")
        elif packaged:
            check("native artifact exists", False, f"missing {artifact}")
    finally:
        shutil.rmtree(base, ignore_errors=True)

    failed = [c for c in checks if not c[1]]
    print("=" * 68)
    print(f"checks={len(checks)} pass={len(checks) - len(failed)} fail={len(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
