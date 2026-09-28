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
# cargo prints the assertion message under a per-test stdout header. Without
# this the annotation can name the failing test but not say what it printed,
# which on a platform-specific failure is the only thing that matters.
STDOUT_SECTION = re.compile(r"^---- (.+?) stdout ----\n(.*?)(?=^\s*$)", re.MULTILINE | re.DOTALL)


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

    body = "".join(captured)
    failed = FAILED.findall(body)
    if failed:
        # The assertion text is what distinguishes a platform bug from a real
        # regression, so it travels with the test name.
        detail = {name: text.strip() for name, text in STDOUT_SECTION.findall(body)}
        print(f"\n{len(failed)} failing test(s):", flush=True)
        for name in failed:
            extra = detail.get(name, "")
            if len(extra) > 900:
                extra = extra[:900] + " ...(truncated)"
            message = name if not extra else f"{name}\\n{extra}"
            # A real newline inside an annotation truncates it, so it is escaped.
            sys.stdout.write(
                "\n::error title=Test failed::" + message.replace("\n", "%0A") + "\n"
            )
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
