#!/usr/bin/env python3
"""Print the distribution repository's release body for one engine version.

The compiler publishes many release candidates; the public distribution is
versioned independently and cut when there is something worth publishing. The
two release bodies are therefore different documents, and the public one comes
from the distribution repository's own RELEASE_NOTES.md.

    python scripts/extract_engine_release_notes.py \
      --notes distribution/RELEASE_NOTES.md --version v1.0.0 > engine-body.md

Exits non-zero when the version is malformed, when the notes have no section for
it, when that section is too short to describe a release, or when it carries
more than one comparison link. Publishing an empty or duplicated body is worse
than failing the release job.
"""

from __future__ import annotations

import argparse
import re
import sys

HEADER = "# Tarvos Engine "
VERSION_PATTERN = re.compile(r"\d+\.\d+\.\d+(?:[-.][0-9A-Za-z.-]+)?")
COMPARE_LINK = re.compile(r"(?m)^https://github\.com/\S+/compare/\S+")


def extract(notes: str, version: str) -> str:
    """Return the release-notes section for one engine version."""
    header = f"{HEADER}{version}"
    start = notes.find(header)
    if start == -1:
        raise LookupError(f"no section for {header!r}")

    rest = notes[start + len(header) :]
    following = re.search(r"(?m)^# Tarvos Engine ", rest)
    section = (header + (rest[: following.start()] if following else rest)).strip() + "\n"

    if len(section.splitlines()) < 3:
        raise LookupError(f"section for {header!r} is empty")

    # A duplicated changelog block is the exact defect seen on a previously
    # deleted draft release, so it fails rather than publishing.
    links = COMPARE_LINK.findall(section)
    if len(links) > 1:
        raise ValueError(f"section for {header!r} has {len(links)} comparison links: {links}")

    return section


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--notes", required=True, help="path to the distribution RELEASE_NOTES.md")
    parser.add_argument("--version", required=True, help="engine version, with or without a v prefix")
    args = parser.parse_args(argv)

    version = args.version.strip().lstrip("v")
    if not VERSION_PATTERN.fullmatch(version):
        print(f"engine version {args.version!r} is not a version", file=sys.stderr)
        return 2

    try:
        with open(args.notes, encoding="utf-8") as handle:
            notes = handle.read()
    except OSError as error:
        print(f"cannot read {args.notes}: {error}", file=sys.stderr)
        return 1

    try:
        section = extract(notes, version)
    except (LookupError, ValueError) as error:
        print(f"{args.notes}: {error}", file=sys.stderr)
        return 1

    # Emit UTF-8 bytes. The default stdout encoding on the Windows console is
    # the code page, which silently mangles the em dashes in these notes.
    sys.stdout.buffer.write(section.encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))