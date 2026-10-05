"""Regression tests for the benchmark harness's PATH sanitization.

The Linux CLI audit failed with

    [FAIL] tarvos benchmark
        exit=1 stderr='Benchmark failed: error: linker `cc` not found'

while the same audit passed on Windows. The cause was `sanitize_environment` in
`benchmarks/run_benchmarks.py`, which keeps a PATH entry only when it sits inside
one of `safe_tool_roots()`. Those roots covered the repository, the interpreter,
the cargo homes and `tools/`, but not the system binary directories. rustc does
not link by itself: it shells out to `cc`, which lives in /usr/bin or /bin, so
stripping those directories removed the linker and every native build in the
harness failed.

The asymmetry is the part worth pinning. On Windows the linking tool is link.exe
from the MSVC Build Tools, which rustc finds through the registry rather than
through PATH, so removing System32 from PATH does not break linking there. The
same filter therefore has to keep the Unix system directories *and* only those,
and a test that only ever runs on one platform cannot catch the difference.

These tests exercise `safe_tool_roots` and `sanitize_environment` directly rather
than running a full benchmark, because a benchmark that fails to link cannot
report why: the assertion has to be about the PATH the harness builds, not about
whatever the linker happens to do.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
HARNESS = ROOT / "benchmarks" / "run_benchmarks.py"


def load_harness():
    """Import the harness without executing its CLI entry point.

    `run_benchmarks.py` is a script that runs a benchmark on import, so it is
    compiled and executed into a private namespace instead of imported normally.
    `__name__` is not `__main__`, which is what keeps the entry point from
    firing.
    """
    source = HARNESS.read_text(encoding="utf-8")
    namespace: dict = {
        "__file__": str(HARNESS),
        "__name__": "run_benchmarks_under_test",
    }
    exec(compile(source, str(HARNESS), "exec"), namespace)  # noqa: S102
    return namespace


failures: list[str] = []


def check(condition: bool, message: str) -> None:
    if not condition:
        failures.append(message)
        print(f"[FAIL] {message}")
    else:
        print(f"[PASS] {message}")


def main() -> int:
    harness = load_harness()
    safe_tool_roots = harness["safe_tool_roots"]
    sanitize_environment = harness["sanitize_environment"]

    if os.name != "nt":
        # The linker is the reason this list exists. On a Unix host rustc invokes
        # `cc`, so a root list without /usr/bin or /bin guarantees a build
        # failure no matter how the rest of the harness is configured.
        roots = {str(root) for root in safe_tool_roots()}
        for directory in ("/usr/bin", "/bin"):
            check(
                directory in roots,
                f"{directory} is a safe root so the C linker stays reachable",
            )

        # The filter has to accept those directories as PATH entries themselves,
        # not merely as parents of one. `root in dir_path.parents` is strict
        # about equality, so a root that is also a PATH entry needs a separate
        # equality branch or the filter drops it anyway.
        env = sanitize_environment({"PATH": "/usr/bin:/bin"})
        entries = env["PATH"].split(os.pathsep)
        for directory in ("/usr/bin", "/bin"):
            check(
                directory in entries,
                f"{directory} survives PATH sanitization: {entries}",
            )

        # A root that exists has to be present in the resulting PATH, because
        # that is what makes the linker reachable rather than merely permitted.
        entries = sanitize_environment({"PATH": ""})["PATH"].split(os.pathsep)
        check(
            "/usr/bin" in entries,
            f"the system directory is added even when PATH is empty: {entries}",
        )

    # The sanitization exists to keep the harness from running an arbitrary
    # executable, so it must keep doing that. Adding the system directories fixes
    # the linker; it is not a licence to accept every PATH entry.
    #
    # The decoy lives outside the repository on purpose. Anything under ROOT is
    # trusted by design, so a directory inside it would be accepted and the check
    # would pass for the wrong reason.
    decoy = pathlib.Path(tempfile.mkdtemp(prefix="tarvos-path-decoy-"))
    try:
        entries = sanitize_environment({"PATH": str(decoy)})["PATH"].split(os.pathsep)
        check(
            str(decoy) not in entries,
            f"an unrelated directory is still filtered out: {entries}",
        )
    finally:
        shutil.rmtree(decoy, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} check(s) failed")
        return 1
    print("\nbenchmark PATH checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())