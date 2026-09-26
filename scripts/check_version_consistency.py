#!/usr/bin/env python3
"""Fail when a declared Tarvos version drifts from the canonical one.

The Cargo workspace version (`[workspace.package] version` in the root
`Cargo.toml`, inherited by every member crate) is the single source of truth.
Release metadata that cannot inherit it -- installer, launcher, README badge,
gateway banner, workflow defaults, packaging metadata -- is written down
separately, so this checker is what keeps the whole set in agreement.

Run as a release gate:

    python scripts/check_version_consistency.py

Exit code 0 means every declaration matches the canonical version.
Exit code 1 lists every mismatch, so one run shows the whole drift.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def normalize(version: str) -> str:
    """Collapse the representations of one version into a comparison key.

    Cargo writes `1.1.0-rc.3`, PEP 440 writes `1.1.0rc3`, shields.io escapes the
    pre-release dash as `1.1.0--rc.3`, and a git tag carries a `v` prefix. All
    four describe the same release, so the key drops the optional `v` and every
    `.`/`-` separator: `1.1.0-rc.3` and `1.1.0rc3` both become `110rc3`.
    """
    text = version.strip().strip('"').lower().lstrip("v")
    return text.replace("-", "").replace(".", "")


def read_text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def canonical_version() -> str:
    """Return the workspace version every other declaration must match."""
    text = read_text("Cargo.toml")
    workspace = re.search(r"(?s)\[workspace\.package\](.*?)(\n\[|\Z)", text)
    if workspace is None:
        sys.exit("Cargo.toml has no [workspace.package] section")
    version = re.search(r'(?m)^version\s*=\s*"([^"]+)"', workspace.group(1))
    if version is None:
        sys.exit("Cargo.toml [workspace.package] has no version")
    return version.group(1)


def member_crates_inherit() -> list[str]:
    """Return workspace members that still hard-code their own version."""
    manifests = sorted(ROOT.glob("crates/*/Cargo.toml")) + [
        ROOT / "compiler" / "Cargo.toml"
    ]
    offenders = []
    for manifest in manifests:
        text = manifest.read_text(encoding="utf-8")
        package = re.search(r"(?s)\[package\](.*?)(\n\[|\Z)", text)
        if package is None:
            continue
        body = package.group(1)
        relative = str(manifest.relative_to(ROOT)).replace("\\", "/")
        if re.search(r'(?m)^version\s*=\s*"', body):
            offenders.append(relative)
        elif "version.workspace = true" not in body:
            offenders.append(relative)
    return offenders


def extract(relative: str, pattern: str) -> str | None:
    """Return the first capture of `pattern` in a repository file."""
    match = re.search(pattern, read_text(relative), re.MULTILINE | re.DOTALL)
    return match.group(1) if match else None


def newest_tag() -> str | None:
    """Return the newest git tag, or None when git is unavailable."""
    try:
        output = subprocess.run(
            ["git", "tag", "--list", "--sort=-creatordate"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    tags = [line.strip() for line in output.splitlines() if line.strip()]
    return tags[0] if tags else None


def declarations() -> list[tuple[str, str, str | None]]:
    """Every release-metadata location that cannot inherit the Cargo version."""
    return [
        (
            "installer/Tarvos.iss",
            "installer version",
            extract("installer/Tarvos.iss", r'MyAppVersion\s+"([^"]+)"'),
        ),
        (
            "crates/tarvos-cli/src/main.rs",
            "clap version",
            extract(
                "crates/tarvos-cli/src/main.rs",
                r'#\[command\(version = "([^"]+)"\)\]',
            ),
        ),
        (
            "crates/tarvos-cli/src/main.rs",
            "version flag",
            extract("crates/tarvos-cli/src/main.rs", r'println!\("tarvos ([^"]+)"\)'),
        ),
        (
            "crates/tarvos-server/src/main.rs",
            "gateway banner",
            extract(
                "crates/tarvos-server/src/main.rs",
                r'"Tarvos v([^"]+) gateway listening',
            ),
        ),
        (
            "crates/tarvos-core/src/api/gateway.rs",
            "health payload",
            extract(
                "crates/tarvos-core/src/api/gateway.rs",
                r'"status":"operational","version":"([^"]+)"',
            ),
        ),
        (
            "crates/tarvos-core/src/api/gateway.rs",
            "cache namespace",
            extract(
                "crates/tarvos-core/src/api/gateway.rs",
                r'join\("cache"\)\.join\("v([^"]+)"\)',
            ),
        ),
        (
            "crates/tarvos-core/src/api/gateway.rs",
            "workspace namespace",
            extract(
                "crates/tarvos-core/src/api/gateway.rs",
                r'join\("workspaces"\)\.join\("v([^"]+)"\)',
            ),
        ),
        (
            "tarvos/launcher.py",
            "launcher default",
            extract(
                "tarvos/launcher.py",
                r'VERSION = os\.environ\.get\("TARVOS_VERSION", "([^"]+)"\)',
            ),
        ),
        (
            "install.ps1",
            "installer default",
            extract("install.ps1", r'\[string\]\$Version = "([^"]+)"'),
        ),
        (
            "render.yaml",
            "sandbox image tag",
            extract("render.yaml", r'value: "tarvos/sandbox:([^"]+)"'),
        ),
        (
            "pyproject.toml",
            "python packaging",
            extract("pyproject.toml", r'(?m)^version = "([^"]+)"'),
        ),
        (
            "README.md",
            "release badge",
            extract("README.md", r"badge/version-(\S+?)-blue"),
        ),
        (
            # A workflow file name is deliberately not a version site: the
            # release-validation workflow is version neutral, so the version
            # only has to be updated in one place, the Cargo workspace.
            ".github/workflows/release.yml",
            "dispatch default",
            extract(".github/workflows/release.yml", r'default: "v([^"]+)"'),
        ),
    ]


def required_tag_failures(tag: str, expected: str) -> list[str]:
    """Failures for `--require-tag`: the tag must exist and match the version."""
    failures: list[str] = []
    if normalize(tag) != expected:
        failures.append(f"release tag {tag!r} does not match canonical version")
    try:
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}"],
            cwd=ROOT,
            check=True,
            capture_output=True,
        )
    except (OSError, subprocess.CalledProcessError):
        failures.append(f"release tag {tag} does not exist in this repository")
    return failures


def main(argv: list[str]) -> int:
    version = canonical_version()
    expected = normalize(version)

    require_tag: str | None = None
    if "--require-tag" in argv:
        index = argv.index("--require-tag")
        if index + 1 >= len(argv):
            print("--require-tag needs a tag value", file=sys.stderr)
            return 2
        require_tag = argv[index + 1]

    failures: list[str] = []
    for path, label, found in declarations():
        if found is None:
            status = "MISSING"
        elif normalize(found) == expected:
            status = "ok"
        else:
            status = "DRIFT"
        if status != "ok":
            failures.append(f"{path} [{label}]: {found!r} != {version!r}")
        print(f"{status:8} {path:42} {label:20} {found}")

    for crate in member_crates_inherit():
        failures.append(f"{crate}: package version does not inherit from the workspace")
        print(f"{'DRIFT':8} {crate:42} member version       hard-coded")

    changelog = extract("CHANGELOG.md", r"(?m)^## \[([^\]]+)\]")
    if changelog and normalize(changelog) != expected:
        print(
            f"{'INFO':8} {'CHANGELOG.md':42} {'released section':20} {changelog}"
        )

    tag = newest_tag()
    if tag:
        tag_status = "ok" if normalize(tag) == expected else "INFO"
        print(f"{tag_status:8} {'git tag':42} {'newest tag':20} {tag}")

    if require_tag is not None:
        tag_failures = required_tag_failures(require_tag, expected)
        status = "ok" if not tag_failures else "DRIFT"
        print(f"{status:8} {'required release tag':42} {'--require-tag':20} {require_tag}")
        failures.extend(tag_failures)

    print(f"\ncanonical version: {version} (normalized: {expected})")
    if failures:
        print(f"\n{len(failures)} version drift failure(s):")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("all version declarations match the canonical version")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

