"""Run the workspace tests and surface each failure as a workflow annotation.

`cargo test` writes failures to stdout. That output only appears in the job log,
which requires write access to fetch, so a macOS-only failure is invisible to
anyone who can merely read the repository. This wrapper re-runs nothing: it
streams the same test run, echoes it, and then emits one `::error::` annotation
per failed test, so the failing test name is visible on the check run itself.

The annotations are additive. The gate still fails on the real exit code, so
nothing here can turn a failure into a pass.
"""

from __future__ import annotations

import re
import subprocess
import sys

# `test <name> ... FAILED` and the `---- <name> stdout ----` headers both name the
# failing test, but the first form is the one cargo prints for every failure.
FAILED = re.compile(r"^test (.+?) \.\.\. FAILED\s*$", re.MULTILINE)


def main() -> int:
    process = subprocess.Popen(
        ["cargo", "test", "--workspace", "--no-fail-fast"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
    )
    captured: list[str] = []
    assert process.stdout is not None
    for line in process.stdout:
        captured.append(line)
        sys.stdout.write(line)
        sys.stdout.flush()
    code = process.wait()

    failed = FAILED.findall("".join(captured))
    if failed:
        print(f"\n{len(failed)} failing test(s):", flush=True)
        for name in failed:
            sys.stdout.write(f"\n::error title=Test failed::{name}\n")
    else:
        # A non-zero exit with no parsed name means the harness itself failed
        # (build error, missing binary) rather than a test asserting false.
        if code != 0:
            sys.stdout.write(
                "\n::error title=Test command failed::"
                "`cargo test --workspace --no-fail-fast` exited "
                f"{code} without reporting a named failing test.\n"
            )
    return code


if __name__ == "__main__":
    sys.exit(main())
