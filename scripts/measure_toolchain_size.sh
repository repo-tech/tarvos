#!/usr/bin/env bash
# Measure the managed toolchain size on Linux.
#
# WHY THIS IS A SEPARATE SCRIPT
# The WSL instance on the development machine tears itself down during a
# four-minute cargo build, so the build plus install could not be driven from
# there end to end. Running it as one command on a machine that stays awake
# removes that flakiness from the measurement.
#
# What it does:
#   1. builds the Tarvos CLI for this host (the bootstrap step, the only step
#      that legitimately needs an already-present Rust)
#   2. installs the managed toolchain through Tarvos itself
#   3. reports the size, and whether the documentation trees are gone
#
# The documentation check is the point of the exercise. A full upstream
# install carries 993 MB of HTML in share/doc that rustc and cargo never read,
# which is 55% of the toolchain. If share/doc reappears, the --without flag
# has stopped working and the size will silently jump.
#
# Usage, from the repository root:
#   bash scripts/measure_toolchain_size.sh
#
# Exit code is 0 only when the toolchain installed, compiles a probe and stays
# under the size budget. Anything else is a failure worth reading.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

# Full upstream install measured 4005 MB. The managed install must stay far
# below that; 1200 MB leaves room for the component set to grow slightly
# without the budget failing on a routine toolchain update.
BUDGET_MB=1200

say() { printf '\n=== %s ===\n' "$1"; }
fail() { printf '[FAIL] %s\n' "$1"; exit 1; }

say "Host"
uname -srm
printf 'cargo: %s\n' "$(command -v cargo || echo '<not on PATH>')"

say "Build the Tarvos CLI (bootstrap; needs an existing Rust)"
if ! cargo build --release -p tarvos-cli; then
    fail "cargo build --release -p tarvos-cli"
fi
TARBOS="$ROOT/target/release/tarvos"
[ -x "$TARBOS" ] || fail "no tarvos binary at $TARBOS"
"$TARBOS" --version

say "Install the managed toolchain through Tarvos"
# Start clean so the number describes this run and not a leftover tree. Only
# the two Tarvos-owned paths are touched; nothing else under ~/.tarvos moves.
rm -rf "$HOME/.tarvos/toolchain" "$HOME/.tarvos/toolchain.staging"
if ! "$TARBOS" toolchain --install; then
    fail "tarvos toolchain --install"
fi

say "Size"
SIZE_MB=$(du -sm "$HOME/.tarvos/toolchain" | cut -f1)
printf 'toolchain: %s MB (budget %s MB)\n' "$SIZE_MB" "$BUDGET_MB"

say "Documentation trees"
# share/doc is the 993 MB that motivated the --without flag. It must be gone.
if [ -d "$HOME/.tarvos/toolchain/share/doc" ]; then
    DOC_MB=$(du -sm "$HOME/.tarvos/toolchain/share/doc" | cut -f1)
    printf '[FAIL] share/doc still present at %s MB; the --without flag is not working\n' "$DOC_MB"
    exit 1
fi
printf '[OK] share/doc absent\n'
printf 'largest subdirectories:\n'
du -sm "$HOME/.tarvos/toolchain"/* 2>/dev/null | sort -rn | head -5

say "Verification"
if ! "$TARBOS" toolchain --verify; then
    fail "tarvos toolchain --verify"
fi

say "Build a Python program with no rustc on PATH"
# The point of the managed toolchain: a user with no Rust installed still gets
# a native binary. Hide the bootstrap Rust for the whole build.
mkdir -p .build-tmp/sizecheck
printf 'values = [3, 1, 2]\nvalues.append(4)\nprint(sorted(values))\n' \
    > .build-tmp/sizecheck/sizecheck.py
if env PATH="/usr/bin:/bin" "$TARBOS" build .build-tmp/sizecheck/sizecheck.py \
        -o .build-tmp/sizecheck/sizecheck; then
    printf '[OK] native build with rustc absent from PATH\n'
else
    fail "native build with rustc absent from PATH"
fi
GOT=$(./.build-tmp/sizecheck/sizecheck)
WANT='[1, 2, 3, 4]'
if [ "$GOT" != "$WANT" ]; then
    fail "native output was '$GOT', expected '$WANT'"
fi
printf '[OK] native output matches CPython: %s\n' "$GOT"

if [ "$SIZE_MB" -gt "$BUDGET_MB" ]; then
    fail "toolchain is $SIZE_MB MB, over the $BUDGET_MB MB budget"
fi

printf '\nAll size checks passed: %s MB (was 4005 MB before the component pin)\n' "$SIZE_MB"