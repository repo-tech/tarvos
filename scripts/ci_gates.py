"""Run the exact gates that CI runs, in the same order.

This exists so the local engineering gates and the CI gates cannot drift apart.
A gate duplicated inside a workflow YAML and a developer's shell inevitably
disagrees eventually; when both call this script, "passes locally" means the
same thing as "passes in CI".

Usage:

    python scripts/ci_gates.py            # every gate
    python scripts/ci_gates.py --quick    # skip the slowest gate (cargo test)

Exit code is 0 only when every gate passed. A failing gate stops the run and
names itself, because a CI log that says "3 of 4 gates passed" is much harder to
act on than one that says which gate failed and why.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# (name, argv, is_slow). Order matters: the cheapest structural checks run
# first, so an obviously broken tree fails in seconds rather than after a full
# workspace build.
GATES: list[tuple[str, list[str], bool]] = [
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"], False),
    ("check", ["cargo", "check", "--workspace", "--all-targets"], False),
    ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], False),
    ("test", ["cargo", "test", "--workspace", "--no-fail-fast"], True),
]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--quick",
        action="store_true",
        help="skip the slow cargo test gate",
    )
    args = parser.parse_args()

    failures: list[str] = []
    for name, command, slow in GATES:
        if slow and args.quick:
            print(f"skip  {name} (--quick)")
            continue
        print(f"\n==== gate: {name} ====\n$ {' '.join(command)}", flush=True)
        started = time.monotonic()
        result = subprocess.run(command, cwd=ROOT)
        elapsed = time.monotonic() - started
        if result.returncode != 0:
            failures.append(name)
            print(f"FAIL  {name} ({elapsed:.1f}s, exit {result.returncode})", flush=True)
            # Emit a workflow annotation as well as log text. A log is only
            # readable with write access to the repository, but an annotation is
            # visible on the check run to anyone who can read the repo. That
            # difference is the whole point: a failing gate stays diagnosable
            # when the log cannot be fetched.
            print(
                f"::error title=Gate failed: {name}::"
                f"`{' '.join(command)}` exited {result.returncode} after {elapsed:.1f}s.",
                flush=True,
            )
            # Stop at the first failing gate. Continuing would bury the real
            # failure under the noise of every gate that depends on it.
            break
        print(f"ok    {name} ({elapsed:.1f}s)", flush=True)

    if failures:
        print(f"\nGATE FAILED: {', '.join(failures)}")
        return 1
    print("\nAll gates passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
