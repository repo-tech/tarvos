#!/usr/bin/env python3
"""Print the RELEASE_NOTES.md section for one release tag.

Release bodies are generated from this file instead of GitHub's auto-generated
notes: auto-generation appended a second "Full Changelog" block to an existing
one in the past (the stale v1.2.0 draft), so the published body must come from
exactly one source.

    python scripts/extract_release_notes.py v1.1.0-rc.3 > release-body.md

Exits non-zero when the tag has no section, because publishing an empty or
generic body is worse than failing the release job.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2

    tag = argv[1].strip()
    version = tag.lstrip("v")
    notes = (ROOT / "RELEASE_NOTES.md").read_text(encoding="utf-8")

    header = f"# Tarvos {version}"
    start = notes.find(header)
    if start == -1:
        print(f"RELEASE_NOTES.md has no section for {header!r}", file=sys.stderr)
        return 1

    rest = notes[start + len(header) :]
    next_section = re.search(r"(?m)^# Tarvos ", rest)
    section = header + (rest[: next_section.start()] if next_section else rest)
    section = section.strip() + "\n"

    if len(section.splitlines()) < 3:
        print(f"RELEASE_NOTES.md section for {tag} is empty", file=sys.stderr)
        return 1

    sys.stdout.write(section)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
