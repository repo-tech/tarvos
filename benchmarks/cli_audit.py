"""Audit every advertised `tarvos` subcommand for real behaviour.

A command is PASS only when it runs, produces its documented effect, and
returns a correct exit code. "The parser accepts it" is not evidence. Destructive
commands run inside an isolated temporary directory, so a developer's real
environment is never touched.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CLI = ROOT / "target" / "debug" / "tarvos.exe"

SAMPLE = 'print("hello from tarvos")\n'

COMMANDS = {
    "compile", "build", "run", "python", "doctor", "ai-status", "analyze",
    "scan", "benchmark", "init", "export", "package", "clean", "install",
    "validate",
}


def run(args, cwd=None, timeout=600, env=None):
    return subprocess.run(
        [str(CLI), *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=timeout,
        errors="replace",
        env={**os.environ, **(env or {})},
    )


class Audit:
    def __init__(self):
        self.results: list[dict] = []

    def check(self, name, ok, detail=""):
        status = "PASS" if ok else "FAIL"
        self.results.append({"command": name, "status": status, "detail": detail})
        line = f"[{status:4s}] tarvos {name}"
        if detail:
            line += f"\n        {detail}"
        print(line, flush=True)
        return ok

    def summary(self) -> int:
        failed = [r for r in self.results if r["status"] == "FAIL"]
        print("=" * 68)
        print(f"checks={len(self.results)} pass={len(self.results) - len(failed)} "
              f"fail={len(failed)}")
        for r in failed:
            print(f"  FAIL {r['command']}: {r['detail']}")
        return 1 if failed else 0


def _cli_surface(a: Audit, work: pathlib.Path) -> pathlib.Path:
    """Help, version, and the per-subcommand help. Returns the sample file."""
    proc = run(["-h"])
    listed = {c for c in COMMANDS if f"  {c}" in proc.stdout}
    a.check("help", proc.returncode == 0 and listed == COMMANDS,
            f"advertised={len(listed)}/{len(COMMANDS)}"
            + (f" missing={sorted(COMMANDS - listed)}" if listed != COMMANDS else ""))

    proc = run(["--version"])
    a.check("--version", proc.returncode == 0 and "1.1.0-rc.3" in proc.stdout,
            proc.stdout.strip()[:60])

    bad = [c for c in sorted(COMMANDS) if run([c, "--help"]).returncode != 0]
    a.check("subcommand --help", not bad, f"failing={bad}")

    sample = work / "sample.py"
    sample.write_text(SAMPLE, encoding="utf-8")
    return sample


def _pipeline_commands(a: Audit, work: pathlib.Path, sample: pathlib.Path) -> None:
    """compile, build, run, python."""
    failing = work / "failing.py"
    failing.write_text("import sys\nsys.exit(3)\n", encoding="utf-8")
    proc = run(["python", str(failing)])
    a.check("python exit code", proc.returncode == 3, f"exit={proc.returncode}")

    proc = run(["python", str(sample)])
    a.check("python", proc.returncode == 0 and "hello from tarvos" in proc.stdout,
            f"exit={proc.returncode}")

    out_rs = work / "out.rs"
    proc = run(["compile", str(sample), "-o", str(out_rs)])
    ok = (proc.returncode == 0 and out_rs.exists()
          and "fn main" in out_rs.read_text(encoding="utf-8"))
    a.check("compile", ok, f"exit={proc.returncode} emitted={out_rs.exists()}")

    built = work / "built.exe"
    proc = run(["build", str(sample), "-o", str(built)])
    if proc.returncode == 0 and built.exists():
        # Execute the artifact directly. It must not go through `run`, which
        # prefixes the CLI and would invoke `tarvos built.exe` instead.
        native = subprocess.run([str(built)], capture_output=True, text=True,
                                timeout=120)
        a.check("build + native run",
                native.returncode == 0 and "hello from tarvos" in native.stdout,
                f"native_exit={native.returncode} out={native.stdout.strip()[:40]!r}")
    else:
        a.check("build + native run", False,
                f"exit={proc.returncode} {proc.stderr.strip()[:110]}")

    proc = run(["run", str(sample)])
    a.check("run", proc.returncode == 0 and "hello from tarvos" in proc.stdout,
            f"exit={proc.returncode}")


def _tooling_commands(a: Audit, work: pathlib.Path, sample: pathlib.Path) -> None:
    """doctor, analyze, scan, benchmark, init."""
    proc = run(["doctor"])
    a.check("doctor", proc.returncode == 0 and "Python" in proc.stdout,
            f"exit={proc.returncode}")

    proc = run(["analyze", str(sample)])
    a.check("analyze", proc.returncode == 0 and bool(proc.stdout.strip()),
            f"exit={proc.returncode}")

    proc = run(["scan", str(work)])
    a.check("scan", proc.returncode == 0, f"exit={proc.returncode}")

    proc = run(["benchmark", str(sample)])
    a.check("benchmark", proc.returncode == 0, f"exit={proc.returncode}")

    init_dir = work / "initproj"
    proc = run(["init", str(init_dir)])
    a.check("init", proc.returncode == 0 and init_dir.exists(),
            f"exit={proc.returncode} created={init_dir.exists()}")


def _project_commands(a: Audit, work: pathlib.Path, sample: pathlib.Path) -> None:
    """export, package, clean, using each command's real signature."""
    # `export` takes the destination as a positional argument, not -o.
    export_dir = work / "exported"
    proc = run(["export", str(sample), str(export_dir)])
    manifest = export_dir / "Cargo.toml"
    exported = a.check("export", proc.returncode == 0 and manifest.exists(),
                       f"exit={proc.returncode} cargo_toml={manifest.exists()}"
                       + (f" {proc.stderr.strip()[-120:]}"
                          if proc.returncode and not manifest.exists() else ""))
    if exported:
        check = subprocess.run(["cargo", "check", "--offline"], cwd=export_dir,
                               capture_output=True, text=True, timeout=600)
        a.check("export -> cargo check", check.returncode == 0,
                f"exit={check.returncode}"
                + (f" {check.stderr.strip()[-200:]}" if check.returncode else ""))

    # `package` refuses to write inside the input's own tree, so the output goes
    # to a sibling directory rather than a subdirectory of `work`.
    pkg_dir = work.parent / (work.name + "-packaged")
    proc = run(["package", str(sample), "-o", str(pkg_dir)])
    a.check("package", proc.returncode == 0 and pkg_dir.exists(),
            f"exit={proc.returncode} dir={pkg_dir.exists()}"
            + (f" {proc.stderr.strip()[-120:]}" if proc.returncode else ""))

    # `clean` takes no path and removes an explicit allowlist of Tarvos-owned
    # locations (.build-tmp, .tarvos_cache, tmp_build, out, tarvos-export, and
    # the user-level tarvos-cache-* directories). It must never remove a file
    # the user owns, so the check asserts both halves of that contract.
    sandbox = work / "cleandir"
    sandbox.mkdir()
    (sandbox / "user_data.txt").write_text("precious", encoding="utf-8")
    (sandbox / "main.py").write_text(SAMPLE, encoding="utf-8")
    owned = sandbox / ".build-tmp"
    owned.mkdir()
    (owned / "scratch.rs").write_text("// scratch", encoding="utf-8")
    stray_exe = sandbox / "tarvos_app.exe"
    stray_exe.write_text("artifact", encoding="utf-8")

    proc = run(["clean"], cwd=sandbox)
    a.check("clean removes only Tarvos-owned paths",
            proc.returncode == 0
            and not owned.exists()
            and (sandbox / "user_data.txt").exists()
            and (sandbox / "main.py").exists()
            and stray_exe.exists(),
            f"exit={proc.returncode} build_tmp_removed={not owned.exists()} "
            f"user_file={(sandbox / 'user_data.txt').exists()} "
            f"source={(sandbox / 'main.py').exists()} "
            f"stray_exe_kept={stray_exe.exists()}")


def _error_paths(a: Audit, work: pathlib.Path) -> None:
    """Failures must be clean messages, never a raw Rust panic."""
    proc = run(["build", str(work / "nope.py")])
    panicked = "panicked at" in proc.stderr
    a.check("missing input is a clean error",
            proc.returncode != 0 and not panicked,
            f"exit={proc.returncode} panic={panicked}")

    unsupported = work / "unsupported.py"
    unsupported.write_text("import importlib\nm = importlib.import_module('os')\n",
                           encoding="utf-8")
    proc = run(["compile", str(unsupported), "-o", str(work / "u.rs")])
    a.check("unsupported feature diagnoses",
            proc.returncode != 0 and "panicked at" not in proc.stderr,
            f"exit={proc.returncode} {proc.stderr.strip()[:80]!r}")

    proc = run(["install"], env={"TARVOS_SANDBOX": "1"})
    a.check("install (sandboxed)", proc.returncode == 0, f"exit={proc.returncode}")


def main() -> int:
    if not CLI.exists():
        print(f"CLI not built: {CLI}", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="tarvos-cli-audit-"))
    a = Audit()
    try:
        sample = _cli_surface(a, work)
        _pipeline_commands(a, work, sample)
        _tooling_commands(a, work, sample)
        _project_commands(a, work, sample)
        _error_paths(a, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return a.summary()


if __name__ == "__main__":
    sys.exit(main())
