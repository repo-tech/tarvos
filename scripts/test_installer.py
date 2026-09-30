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

    # The managed install shells out to the upstream install.sh, which accepts
    # only the flags it documents in --help:
    #
    #   --prefix --components --without --bindir --libdir --datadir
    #   --mandir --docdir --disable-ldconfig --verbose --destdir --sysconfdir
    #   --uninstall --list-components --disable-verify
    #
    # Two of these are load-bearing:
    #
    #   --components / --without   a measured install laid down 993 MB of HTML
    #                               in share/doc, 55% of the toolchain, which
    #                               rustc and cargo never read.
    #   (and NOT --no-modify-path)  passing it aborted the install with
    #                               "Option '--no-modify-path' is not
    #                               recognized". PATH editing is rustup-init's
    #                               job, not this script's.
    KNOWN_INSTALL_SH_FLAGS = {
        "--uninstall",
        "--destdir",
        "--prefix",
        "--without",
        "--components",
        "--list-components",
        "--sysconfdir",
        "--bindir",
        "--libdir",
        "--datadir",
        "--mandir",
        "--docdir",
        "--disable-ldconfig",
        "--disable-verify",
        "--verbose",
    }
    toolchain_src = ROOT / "crates" / "tarvos-cli" / "src" / "toolchain.rs"
    if not toolchain_src.exists():
        failures.append("crates/tarvos-cli/src/toolchain.rs is missing")
    else:
        tc_text = toolchain_src.read_text(encoding="utf-8")
        # Only inspect the flags passed to the installer invocation.
        start = tc_text.find("let components = format!")
        end = tc_text.find(".status()", start)
        inv = tc_text[start:end] if start != -1 and end != -1 else ""
        if not inv:
            failures.append(
                "could not locate the install.sh invocation in toolchain.rs; "
                "the flag checks below would silently pass"
            )
        used = set(re.findall(r'\.arg\("(--[a-z0-9-]+)', inv))
        unknown = used - KNOWN_INSTALL_SH_FLAGS
        check(
            not unknown,
            f"toolchain.rs passes flags install.sh does not accept: "
            f"{sorted(unknown)}",
        )
        check(
            "rustc,cargo,rust-std-{triple}" in inv,
            "the component set must be exactly rustc, cargo and the host std",
        )
        check(
            "--without=rust-docs" in inv,
            "the documentation trees must be excluded by name; a full install "
            "left 993 MB of unread HTML in share/doc",
        )
        # Look for these only where a flag is genuinely passed, so a comment
        # explaining the mistake cannot trip the check.
        passed = set(re.findall(r'\.arg\("(--[a-z0-9-]+)', inv))
        check(
            "--no-modify-path" not in passed,
            "--no-modify-path does not exist in this install.sh and aborts the "
            "install; PATH editing belongs to rustup-init, not this script",
        )
        check(
            "--profile=minimal" not in passed,
            "--profile is not a documented flag for this install.sh either",
        )

    for failure in failures:
        print(f"[FAIL] {failure}")
    if failures:
        return 1
    print("installer and linux-verify checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
