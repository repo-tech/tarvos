from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
import sys
import urllib.request
from urllib.error import HTTPError, URLError
from pathlib import Path


VERSION = os.environ.get("TARVOS_VERSION", "v1.5.0")
REPOSITORY = os.environ.get("TARVOS_REPOSITORY", "repo-tech/tarvos")
TOKEN = os.environ.get("TARVOS_GITHUB_TOKEN")


def _asset() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    if system == "windows" and machine in {"amd64", "x86_64", "x64"}:
        return "tarvos-windows-x86_64.exe"
    if system == "linux" and machine in {"amd64", "x86_64", "x64"}:
        return "tarvos-linux-x86_64"
    if system == "darwin" and machine in {"amd64", "x86_64", "x64"}:
        return "tarvos-macos-x86_64"
    raise RuntimeError(f"Tarvos has no published binary for {system}/{machine}")


def _bin_dir() -> Path:
    home = Path.home()
    return home / ".tarvos" / "bin"


def _download_binary() -> Path:
    asset = _asset()
    directory = _bin_dir()
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / asset
    if os.name == "nt":
        binary = binary.with_suffix(".exe")
    if binary.exists():
        return binary

    release_path = "latest" if VERSION == "latest" else f"tags/{VERSION}"
    api_base = f"https://api.github.com/repos/{REPOSITORY}/releases"
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "tarvos-python-launcher",
    }
    if TOKEN:
        headers["Authorization"] = f"Bearer {TOKEN}"
    request = urllib.request.Request(f"{api_base}/{release_path}", headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        release = json.loads(response.read().decode("utf-8"))
    assets = {entry["name"]: entry for entry in release.get("assets", [])}
    if asset not in assets or f"{asset}.sha256" not in assets:
        raise RuntimeError(
            f"release {release.get('tag_name', VERSION)} is missing {asset} or {asset}.sha256"
        )

    def read_asset(name: str) -> bytes:
        request = urllib.request.Request(
            assets[name]["url"],
            headers={
                **headers,
                "Accept": "application/octet-stream",
            },
        )
        with urllib.request.urlopen(request, timeout=60) as response:
            return response.read()

    temporary = binary.with_suffix(binary.suffix + ".tmp")
    temporary.write_bytes(read_asset(asset))
    expected = read_asset(f"{asset}.sha256").decode("ascii").split()[0].lower()
    actual = hashlib.sha256(temporary.read_bytes()).hexdigest()
    if actual != expected:
        temporary.unlink(missing_ok=True)
        raise RuntimeError("Tarvos binary checksum verification failed")
    temporary.replace(binary)
    if os.name != "nt":
        binary.chmod(0o755)
    return binary


def main() -> int:
    try:
        binary = _download_binary()
    except (HTTPError, URLError, OSError, RuntimeError, KeyError, ValueError) as error:
        print(f"Tarvos installation failed: {error}", file=sys.stderr)
        print("Set TARVOS_VERSION to a published release tag and retry.", file=sys.stderr)
        return 1
    completed = subprocess.run([str(binary), *sys.argv[1:]], check=False)
    return completed.returncode
