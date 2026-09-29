"""Regression tests for the POSIX installer.

The installer shipped a `--tl1v1.2` typo where curl wanted `--tlsv1.2`. curl
rejects the entire invocation with "option '--ttl1.2' is unknown", so the
rustup step never ran and the install failed with no actionable message. These
tests pin the exact flags the installer may pass, so a typo cannot come back
unnoticed.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
INSTALLER = ROOT / "install.sh"
LINUX_VERIFY = ROOT / "scripts" / "verify_linux.sh"

# Flags curl accepts. Anything starting with `--` and not listed here is
# rejected by curl at runtime, which is exactly how the --tl1v1 typo shipped.
KNOWN_CURL_LONG_FLAGS = {
    "--proto",
    "--tlsv1.2",
    "--tls-max",
    "--insecure",
    "--location",
    "--fail",
    "--silent",
    "--show-error",
    "--output",
    "--retry",
    "--connect-timeout",
    "--max-time",
    "--cacert",
    "--capath",
}

failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


def main() -> int:
    check(INSTALLER.is_file(), "install.sh is missing")
    if failures:
        for failure in failures:
            print(f"[FAIL] {failure}")
        return 1

    text = INSTALLER.read_text(encoding="utf-8")
    # Only executable lines count. Comments describe the bug on purpose, so
    # scanning them would make this test fail against its own documentation.
    code = [
        line
        for line in text.splitlines()
        if not line.strip().startswith("#")
    ]
    code_text = "\n".join(code)

    # 1. No misspellings of the TLS flag.
    check("--tl1v1.2" not in code_text, "install.sh still contains the --tl1v1.2 typo")
    check("--ttl1.2" not in code_text, "install.sh contains the --ttl1.2 typo")

    # 2. Every long flag passed to curl is one curl actually accepts.
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped.startswith("curl "):
            continue
        for flag in re.findall(r"(?<![\w-])(--[A-Za-z0-9][\w.-]*)", stripped):
            check(
                flag in KNOWN_CURL_LONG_FLAGS,
                f"install.sh passes unknown curl flag {flag}: {stripped}",
            )

    # 3. Every curl invocation is piped into something, so a download failure
    #    cannot leave a half-installed state silently.
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("curl ") and "|" not in stripped and "-o " not in stripped:
            check(False, f"curl output is not consumed: {stripped}")

    # 4. The installer must fail loudly rather than continuing after a bad
    #    download.
    check("set -euo pipefail" in text, "installer does not use strict mode")

    # 5. It must not silently assume a source tree exists in the caller's cwd.
    check(
        "crates/tarvos-cli" in text and "if [ ! -d" in text,
        "installer does not check for a Tarvos source tree before building",
    )

    # The Linux acceptance script is shell too, and a CRLF shebang makes bash
    # refuse the file with an error that looks like a missing interpreter.
    check(
        LINUX_VERIFY.is_file(),
        "scripts/verify_linux.sh is missing",
    )
    if LINUX_VERIFY.is_file():
        raw = LINUX_VERIFY.read_bytes()
        check(
            b"\r\n" not in raw,
            "scripts/verify_linux.sh has CRLF line endings; bash needs LF for the shebang",
        )
        check(
            not raw.startswith(b"\xef\xbb\xbf"),
            "scripts/verify_linux.sh starts with a UTF-8 BOM; bash cannot match the shebang",
        )
        verify_text = raw.decode("utf-8")
        # The shebang is compared against the raw first line: stripping
        # comments would strip the shebang too and make this check vacuous.
        first_line = verify_text.splitlines()[0] if verify_text.splitlines() else ""
        check(
            first_line == "#!/usr/bin/env bash",
            f"scripts/verify_linux.sh needs a bash shebang, found {first_line!r}",
        )
        # The acceptance run has to hide Rust from PATH, or it proves nothing.
        check(
            'PATH="/usr/bin:/bin"' in verify_text,
            "scripts/verify_linux.sh must build with rustc absent from PATH",
        )
        check(
            "toolchain --install" in verify_text and "toolchain --verify" in verify_text,
            "scripts/verify_linux.sh must exercise the managed toolchain",
        )

    for failure in failures:
        print(f"[FAIL] {failure}")
    if failures:
        return 1
    print("installer and linux-verify checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
