"""Asset resolution, the one decision a user cannot check by reading the file.

`_asset()` picks a release attachment from the platform. Getting it wrong is
silent and fatal in the worst way: `pip install tarvos` succeeds everywhere, and
the CLI fails only when it is first run, on the machine that needed it. So the
mapping is pinned here rather than left to a release page to reveal.

`_download_binary()` verifies a SHA-256 against the `.sha256` fetched from the
same release. A mismatch must abort and leave nothing behind, because a launcher
that runs an unverified binary is worse than one that refuses to run.
"""

from __future__ import annotations

import hashlib
import json

import tarvos.launcher as launcher


def _asset(system: str, machine: str) -> str:
    original_system = launcher.platform.system
    original_machine = launcher.platform.machine
    launcher.platform.system = lambda: system
    launcher.platform.machine = lambda: machine
    try:
        return launcher._asset()
    finally:
        launcher.platform.system = original_system
        launcher.platform.machine = original_machine


def test_windows_x86_64_resolves_to_the_windows_asset():
    assert _asset("Windows", "AMD64") == "tarvos.exe"
# 

def test_linux_x86_64_resolves_without_an_exe_suffix():
    # A `.exe` name is a PE file that Linux cannot execute, so this is an
    # architecture requirement rather than a naming preference.
    assert _asset("Linux", "x86_64") == "tarvos"


def test_macos_is_refused_and_says_why():
    # macOS is not built for this release. Mapping it to an asset that does not
    # exist would install cleanly and then fail on the download, so the launcher
    # refuses at the point where the platform is still known and names it.
    try:
        _asset("Darwin", "x86_64")
    except RuntimeError as error:
        message = str(error)
        assert "darwin/x86_64" in message
        # The user has to be told which platforms do work, or the refusal is
        # just a dead end.
        assert "Windows and Linux" in message
    else:
        raise AssertionError(
            "no macOS binary is published, so macOS must not resolve to an asset"
        )


def test_macos_arm64_is_refused_rather_than_given_an_intel_binary():
    # Apple silicon runs x86_64 under emulation, but that is not something a
    # launcher may promise on the user's behalf: it costs memory they may not
    # have, and it fails outright on a machine without Rosetta. Refusing is the
    # honest answer, and now it is also the only answer.
    try:
        _asset("Darwin", "arm64")
    except RuntimeError as error:
        assert "darwin/arm64" in str(error)
    else:
        raise AssertionError(
            "macOS arm64 has no published binary and must not be served an x86_64 one"
        )

def test_an_unsupported_machine_fails_with_the_platform_in_the_message():
    # The message is all a user sees when this is wrong, so it has to name the
    # platform that was rejected rather than just saying "unsupported".
    try:
        _asset("Windows", "riscv64")
    except RuntimeError as error:
        assert "windows/riscv64" in str(error)
    else:
        raise AssertionError("an unsupported architecture must be refused")


def test_a_32_bit_machine_is_not_silently_given_a_64_bit_binary():
    for system in ("Windows", "Linux", "Darwin"):
        try:
            _asset(system, "x86")
        except RuntimeError:
            continue
        raise AssertionError(f"{system}/x86 must not resolve to a 64-bit asset")


class _FakeResponse:
    """Minimal stand-in for the context manager `urlopen` returns."""

    def __init__(self, payload: bytes) -> None:
        self._payload = payload

    def __enter__(self) -> "_FakeResponse":
        return self

    def __exit__(self, *_exc: object) -> bool:
        return False

    def read(self) -> bytes:
        return self._payload


def _manifest_bytes(asset: str) -> bytes:
    return json.dumps(
        {
            "tag_name": "v1.0.0",
            "assets": [
                {"name": asset, "url": f"https://uploads.example/{asset}"},
                {
                    "name": f"{asset}.sha256",
                    "url": f"https://uploads.example/{asset}.sha256",
                },
            ],
        }
    ).encode("utf-8")


def _serve(manifest: bytes, checksum: bytes, payload: bytes):
    """A `urlopen` that answers the three requests the launcher makes."""

    def fake_urlopen(request, timeout=0):
        url = getattr(request, "full_url", "")
        if "api.github.com" in url:
            return _FakeResponse(manifest)
        if url.endswith(".sha256"):
            return _FakeResponse(checksum)
        return _FakeResponse(payload)

    return fake_urlopen


def test_a_matching_checksum_installs_the_binary(monkeypatch, tmp_path):
    payload = b"fake tarvos binary"
    digest = hashlib.sha256(payload).hexdigest()
    asset = "tarvos-linux-x86_64"

    monkeypatch.setattr(launcher, "_bin_dir", lambda: tmp_path / "bin")
    monkeypatch.setattr(launcher.os, "name", "posix")
    monkeypatch.setattr(launcher, "_asset", lambda: asset)
    monkeypatch.setattr(
        launcher.urllib.request,
        "urlopen",
        _serve(_manifest_bytes(asset), digest.encode("ascii"), payload),
    )

    installed = launcher._download_binary()

    assert installed.read_bytes() == payload
    # No temporary file may survive: a half-written binary that a later run
    # mistakes for a complete one is worse than no file at all.
    assert not installed.with_suffix(installed.suffix + ".tmp").exists()


def test_a_wrong_checksum_installs_nothing(monkeypatch, tmp_path):
    payload = b"tampered tarvos binary"
    expected = hashlib.sha256(b"the real one").hexdigest()
    asset = "tarvos-linux-x86_64"

    monkeypatch.setattr(launcher, "_bin_dir", lambda: tmp_path / "bin")
    monkeypatch.setattr(launcher.os, "name", "posix")
    monkeypatch.setattr(launcher, "_asset", lambda: asset)
    monkeypatch.setattr(
        launcher.urllib.request,
        "urlopen",
        _serve(_manifest_bytes(asset), expected.encode("ascii"), payload),
    )

    try:
        launcher._download_binary()
    except RuntimeError as error:
        assert "checksum" in str(error)
    else:
        raise AssertionError("a checksum mismatch must not be installed")

    installed = tmp_path / "bin" / asset
    assert not installed.exists(), "an unverified binary must not be left on disk"
    assert not installed.with_suffix(installed.suffix + ".tmp").exists()


def test_a_release_missing_its_binary_is_refused(monkeypatch, tmp_path):
    # A release published without the asset this platform needs is a publishing
    # mistake, and the message has to say so rather than reporting a checksum
    # problem the user cannot act on.
    asset = "tarvos-macos-x86_64"
    manifest = json.dumps({"tag_name": "v1.0.0", "assets": []}).encode("utf-8")

    monkeypatch.setattr(launcher, "_bin_dir", lambda: tmp_path / "bin")
    monkeypatch.setattr(launcher, "_asset", lambda: asset)
    monkeypatch.setattr(launcher.urllib.request, "urlopen", lambda *a, **k: _FakeResponse(manifest))

    try:
        launcher._download_binary()
    except RuntimeError as error:
        assert asset in str(error)
    else:
        raise AssertionError("a release without the required asset must be refused")


def test_main_reports_failure_instead_of_raising(monkeypatch, capsys):
    # `main` is the console-script entry point. An uncaught exception there
    # produces a traceback that says nothing about what the user should do.
    def boom():
        raise RuntimeError("Tarvos has no published binary for darwin/arm64")

    monkeypatch.setattr(launcher, "_download_binary", boom)

    code = launcher.main()

    captured = capsys.readouterr()
    assert code == 1
    assert "darwin/arm64" in captured.err
    assert "TARVOS_VERSION" in captured.err


def test_main_forwards_the_arguments_and_returns_the_exit_code(monkeypatch):
    recorded = {}

    class Completed:
        returncode = 7

    def fake_run(command, check=False):
        recorded["command"] = command
        return Completed()

    monkeypatch.setattr(launcher, "_download_binary", lambda: "/opt/tarvos")
    monkeypatch.setattr(launcher.subprocess, "run", fake_run)
    monkeypatch.setattr(launcher.sys, "argv", ["tarvos", "build", "app.py"])

    assert launcher.main() == 7, "the CLI's exit status must reach the shell"
    assert recorded["command"] == ["/opt/tarvos", "build", "app.py"]
