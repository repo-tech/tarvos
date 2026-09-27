"""Differential test harness: CPython vs Tarvos-native.

For each case in a corpus:
  1. run CPython
  2. transpile + compile with Tarvos
  3. run the native executable
  4. compare stdout, stderr, and exit code

A case is only a PASS when the native artifact actually runs and matches.
Compiler rejections are recorded as SKIP with the reason, never as passes.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "corpus"
WORK = ROOT / ".build-tmp" / "difftest"
CLI = ROOT / "target" / "debug" / "tarvos.exe"
EXE = ".exe" if os.name == "nt" else ""


def run(cmd, cwd=None, timeout=180):
    return subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, errors="replace"
    )


def normalize(text: str) -> str:
    """Normalize only what is genuinely nondeterministic.

    Object addresses and temp paths differ between CPython and a native
    binary and carry no semantic meaning. Nothing else is rewritten: stdout is
    compared exactly, because a difference in output IS a semantic difference.
    """
    out = []
    for line in text.splitlines():
        if line.startswith("0x") and " at " in line:
            line = "<address>"
        out.append(line)
    return "\n".join(out)


def cpython(case: pathlib.Path) -> tuple[int, str, str]:
    proc = run([sys.executable, str(case)])
    return proc.returncode, normalize(proc.stdout), normalize(proc.stderr)


def native(case: pathlib.Path, tag: str) -> tuple[str, str]:
    """Transpile and build, returning (status, detail)."""
    build_dir = WORK / tag
    if build_dir.exists():
        shutil.rmtree(build_dir, ignore_errors=True)
    build_dir.mkdir(parents=True, exist_ok=True)

    rust = build_dir / "main.rs"
    proc = run([str(CLI), "compile", str(case), "-o", str(rust)])
    if proc.returncode != 0:
        return "REJECTED", (proc.stdout + proc.stderr).strip()

    manifest = build_dir / "Cargo.toml"
    manifest.write_text(
        # An empty [workspace] stops Cargo from walking up and attaching the
        # generated crate to the real Tarvos workspace.
        "[workspace]\n\n"
        "[package]\n"
        'name = "case"\n'
        'version = "0.0.0"\n'
        'edition = "2021"\n\n'
        "[[bin]]\n"
        'name = "case"\n'
        'path = "main.rs"\n\n'
        "[profile.release]\n"
        'panic = "abort"\n'
        "opt-level = 1\n",
        encoding="utf-8",
    )
    proc = run(
        ["cargo", "build", "--release", "--offline"],
        cwd=build_dir,
        timeout=600,
    )
    if proc.returncode != 0:
        return "RUSTC_FAILED", (proc.stdout + proc.stderr).strip()[-2000:]
    return "BUILT", ""


def run_native(tag: str) -> tuple[int, str, str]:
    exe = WORK / tag / "target" / "release" / f"case{EXE}"
    if not exe.exists():
        raise FileNotFoundError(exe)
    proc = run([str(exe)], cwd=WORK / tag, timeout=60)
    return proc.returncode, normalize(proc.stdout), normalize(proc.stderr)


def compare(name, cpy, nat) -> tuple[str, str]:
    ccode, cout, cerr = cpy
    ncode, nout, nerr = nat
    if ccode != ncode:
        return "FAIL", f"exit code: CPython={ccode} native={ncode}"
    if cout != nout:
        return "FAIL", f"stdout differs\n  CPython: {cout!r}\n  native : {nout!r}"
    if cerr != nerr:
        return "FAIL", f"stderr differs\n  CPython: {cerr!r}\n  native : {nerr!r}"
    return "PASS", ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--filter", default="", help="substring filter on case name")
    ap.add_argument("--keep", action="store_true", help="keep build dirs")
    ap.add_argument("--json", default="", help="write a JSON report here")
    args = ap.parse_args()

    if not CLI.exists():
        print(f"CLI not built: {CLI}", file=sys.stderr)
        return 2

    WORK.mkdir(parents=True, exist_ok=True)
    cases = sorted(CORPUS.glob("*.py"))
    if args.filter:
        cases = [c for c in cases if args.filter in c.stem]

    results = []
    counts = {"PASS": 0, "FAIL": 0, "REJECTED": 0, "RUSTC_FAILED": 0}
    start = time.time()

    for case in cases:
        name = case.stem
        cpy = cpython(case)
        status, detail = native(case, name)
        if status == "BUILT":
            try:
                nat = run_native(name)
                verdict, reason = compare(name, cpy, nat)
            except FileNotFoundError as exc:
                verdict, reason = "FAIL", f"artifact missing: {exc}"
        else:
            verdict, reason = status, detail

        counts[verdict] = counts.get(verdict, 0) + 1
        results.append(
            {"name": name, "verdict": verdict, "reason": reason, "cpython": cpy[0]}
        )
        mark = {"PASS": "PASS", "FAIL": "FAIL"}.get(verdict, "SKIP")
        line = f"[{mark:4s}] {name}"
        if verdict != "PASS" and reason:
            first = reason.splitlines()[0]
            line += f"  -- {first}"
        print(line, flush=True)

        if not args.keep and verdict != "FAIL":
            shutil.rmtree(WORK / name, ignore_errors=True)

    elapsed = time.time() - start
    print("=" * 68)
    print(
        f"cases={len(cases)}  pass={counts['PASS']}  fail={counts['FAIL']}  "
        f"rejected={counts['REJECTED']}  rustc_failed={counts['RUSTC_FAILED']}  "
        f"{elapsed:.1f}s"
    )

    if args.json:
        pathlib.Path(args.json).write_text(
            json.dumps({"counts": counts, "results": results}, indent=2),
            encoding="utf-8",
        )
    return 1 if counts["FAIL"] else 0


if __name__ == "__main__":
    sys.exit(main())
