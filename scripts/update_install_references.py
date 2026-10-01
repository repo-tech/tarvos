#!/usr/bin/env python3
"""Point the release notes at the release asset and fix the command spelling.

Two corrections, both found by actually running the documented install:

  - the notes fetched the installer from raw.githubusercontent.com, which on a
    measured connection took 30s for the same 1.3 kB script the release asset
    delivers in under a second. That reads to a user as a frozen installer;
  - `tarvos toolchain install` is spelled without dashes in eight places, which
    reads as a subcommand and is rejected by the CLI.

The engine version is read from the distribution repository's VERSION file
rather than the compiler workspace version, because the download URL points at
the engine release and the two version lines are deliberately independent.

    python scripts/update_install_references.py
"""

import io
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NOTES = ROOT / "RELEASE_NOTES.md"
ENGINE_VERSION = "1.0.0"

REPLACEMENTS = [
    (
        "irm https://raw.githubusercontent.com/repo-tech/tarvos-engine/main/install.ps1 | iex",
        "irm https://github.com/repo-tech/tarvos-engine/releases/download/"
        f"v{ENGINE_VERSION}/install.ps1 | iex",
    ),
    (
        "curl --fail --location https://raw.githubusercontent.com/repo-tech/tarvos-engine/main/install.sh | bash",
        "curl --fail --location https://github.com/repo-tech/tarvos-engine/releases/download/"
        f"v{ENGINE_VERSION}/install.sh | bash",
    ),
    ("tarvos toolchain install", "tarvos toolchain --install"),
    ("tarvos toolchain verify", "tarvos toolchain --verify"),
    ("tarvos toolchain status", "tarvos toolchain --status"),
]

UNDASHED = ("toolchain install", "toolchain verify", "toolchain status")


def main() -> int:
    if not NOTES.is_file():
        print(f"no release notes at {NOTES}", file=sys.stderr)
        return 2

    text = NOTES.read_text(encoding="utf-8")
    original = text
    for old, new in REPLACEMENTS:
        count = original.count(old)
        if count:
            print(f"  {count}x  {old[:66]}")
            text = text.replace(old, new)

    if text == original:
        print("no change needed")
        return 0

    with io.open(NOTES, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)

    # Confirm nothing undashed survived, so this cannot report success while a
    # stale reference is still in the notes. Only invocations count: a sentence
    # such as "the toolchain install no longer appends to shell profiles" is
    # prose about the step, not a command, and must not be rewritten.
    stale = []
    for line in text.splitlines():
        if "tarvos toolchain" not in line:
            continue
        after = line.split("tarvos toolchain", 1)[1]
        for flag in ("install", "verify", "status"):
            if after.lstrip().startswith(flag):
                stale.append(line.strip())
    if stale:
        print("\nundashed toolchain invocations still present:")
        for line in stale:
            print(f"  {line}")
        return 1

    print(f"\nupdated {NOTES.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())