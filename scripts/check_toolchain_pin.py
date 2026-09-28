"""Verify the pinned Rust toolchain agrees across every place it is declared.

The toolchain is named in three places that can silently disagree:

    rust-toolchain.toml            the channel Cargo and rustup actually use
    .github/workflows/ci.yml       the channel the setup action installs
    .github/workflows/release.yml  the channel the release build installs

When they disagree the failure is confusing rather than obvious: Cargo honours
`rust-toolchain.toml` and quietly re-downloads a different compiler than the
step above installed, so a clippy result from one run gets attributed to another.
This makes the disagreement a named, immediate error instead.

Exit code is 0 only when every declaration matches.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

TOOLCHAIN_FILE = ROOT / "rust-toolchain.toml"
WORKFLOWS = [
    ROOT / ".github" / "workflows" / "ci.yml",
    ROOT / ".github" / "workflows" / "release.yml",
]

CHANNEL = re.compile(r'^\s*channel\s*=\s*"([^"]+)"', re.MULTILINE)
TOOLCHAIN_ACTION = re.compile(r"dtolnay/rust-toolchain@([^\s]+)")


def main() -> int:
    if not TOOLCHAIN_FILE.is_file():
        print(f"fail  missing {TOOLCHAIN_FILE.relative_to(ROOT)}")
        return 1
    match = CHANNEL.search(TOOLCHAIN_FILE.read_text(encoding="utf-8"))
    if not match:
        print("fail  rust-toolchain.toml has no channel")
        return 1
    pinned = match.group(1)
    if pinned in {"stable", "nightly", "beta"}:
        print(
            f"fail  rust-toolchain.toml pins a floating channel ({pinned!r}). "
            "A moving channel makes CI non-reproducible; pin an exact version."
        )
        return 1

    failures = 0
    for workflow in WORKFLOWS:
        if not workflow.is_file():
            continue
        found = set(TOOLCHAIN_ACTION.findall(workflow.read_text(encoding="utf-8")))
        if not found:
            print(f"ok    {workflow.relative_to(ROOT)} (no toolchain step)")
            continue
        for used in sorted(found):
            if used == pinned:
                print(f"ok    {workflow.relative_to(ROOT)} uses {pinned}")
            else:
                failures += 1
                print(
                    f"fail  {workflow.relative_to(ROOT)} installs {used!r} but "
                    f"rust-toolchain.toml pins {pinned!r}"
                )

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
