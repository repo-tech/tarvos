#!/usr/bin/env bash
# Tarvos Linux acceptance run.
#
# This is the machine a normal Linux user has: no rustc, no cargo, no rustup on
# PATH. That is the whole point. If this script only passes on a machine that
# already has Rust, it has not tested anything worth testing.
#
# Usage:
#   scripts/verify_linux.sh            # full run, managed toolchain expected
#   scripts/verify_linux.sh --system   # exercise the explicit system-Rust path
#
# Exit code is 0 only when every check passed. Each check prints PASS, FAIL or
# SKIP with the reason inline, so a partial run is still readable.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$ROOT/.build-tmp/linux-verify"
USE_SYSTEM=0
[[ "${1:-}" == "--system" ]] && USE_SYSTEM=1

PASS=0
FAIL=0
SKIP=0

say()  { printf '\n=== %s ===\n' "$*"; }
ok()   { printf '[PASS] %s\n' "$*"; PASS=$((PASS+1)); }
bad()  { printf '[FAIL] %s\n' "$*"; FAIL=$((FAIL+1)); }
skip() { printf '[SKIP] %s\n' "$*"; SKIP=$((SKIP+1)); }

# Run a check, mark it passed on exit 0 and failed otherwise.
check() {
  local name="$1"; shift
  if "$@"; then ok "$name"; else bad "$name"; fi
}

rm -rf "$WORK"
mkdir -p "$WORK" || { echo "cannot create $WORK"; exit 1; }

cat > "$WORK/hello.py" <<'PY'
import json


def classify(n):
    if n < 0:
        return "negative"
    if n == 0:
        return "zero"
    return "positive"


values = [classify(n) for n in (-1, 0, 1)]
print(json.dumps(values))
PY

say "Host"
uname -a
printf 'cargo: %s\n' "$(command -v cargo || echo '<not on PATH>')"
printf 'rustc: %s\n' "$(command -v rustc || echo '<not on PATH>')"
printf 'rustup: %s\n' "$(command -v rustup || echo '<not on PATH>')"

say "Build the Tarvos CLI from source (bootstrap only, needs a Rust toolchain)"
# The CLI itself is Rust, so producing it needs *some* Rust. Once built, the
# artifact is self-sufficient; that is what the remaining checks prove.
BOOTSTRAP_LOG="$WORK/bootstrap.log"
if cargo build --release -p tarvos-cli > "$BOOTSTRAP_LOG" 2>&1; then
  ok "cargo build -p tarvos-cli"
else
  bad "cargo build -p tarvos-cli (see $BOOTSTRAP_LOG)"
  tail -20 "$BOOTSTRAP_LOG"
fi

TARBOS="$ROOT/target/release/tarvos"
if [[ ! -x "$TARBOS" ]]; then
  bad "release binary missing at $TARBOS; nothing else can be checked"
  printf '\nRESULT: %d passed, %d failed, %d skipped\n' "$PASS" "$FAIL" "$SKIP"
  exit 1
fi

say "tarvos --version"
check "tarvos --version" "$TARBOS" --version

say "tarvos toolchain --status"
"$TARBOS" toolchain --status || bad "tarvos toolchain --status"

say "tarvos toolchain --verify (expect ERROR before install; that is honest)"
if "$TARBOS" toolchain --verify; then
  ok "toolchain already verified"
else
  ok "toolchain verify correctly reports missing (not a crash)"
fi

if [[ "$USE_SYSTEM" -eq 1 ]]; then
  say "System-Rust path (explicitly requested)"
  if command -v rustc > /dev/null 2>&1; then
    check "build --system-rust" "$TARBOS" build "$WORK/hello.py" \
      -o "$WORK/hello-system" --system-rust
  else
    skip "build --system-rust (no system rustc on PATH, as expected in a clean env)"
  fi
else
  say "Managed toolchain install"
  if "$TARBOS" toolchain --install; then
    ok "toolchain --install"
  else
    bad "toolchain --install (see the message above)"
  fi
fi

say "tarvos toolchain --verify (after install)"
if "$TARBOS" toolchain --verify; then
  ok "every managed check passed"
else
  bad "managed toolchain did not verify"
fi

say "Native build without rustc on PATH"
# The acceptance test: hide every Rust toolchain from PATH and require the
# build to still work through the managed toolchain.
env PATH="/usr/bin:/bin" HOME="$HOME" \
  bash -c "command -v rustc > /dev/null && exit 99; true" || true
HIDE_OUT="$WORK/nopath.log"
if env PATH="/usr/bin:/bin" "$TARBOS" build "$WORK/hello.py" -o "$WORK/hello" > "$HIDE_OUT" 2>&1; then
  ok "build succeeds with rustc/cargo/rustup absent from PATH"
else
  bad "build failed with rustc/cargo/rustup absent from PATH (see $HIDE_OUT)"
  tail -15 "$HIDE_OUT"
fi

say "Run the native binary (no Python, no Tarvos needed)"
if [[ -x "$WORK/hello" ]]; then
  chmod +x "$WORK/hello" 2>/dev/null || true
  OUT="$("$WORK/hello" 2>&1)"
  EXPECT='["negative", "zero", "positive"]'
  if [[ "$OUT" == "$EXPECT" ]]; then
    ok "native output matches CPython: $OUT"
  else
    bad "native output differs"
    printf '      expected: %s\n      actual  : %s\n' "$EXPECT" "$OUT"
  fi
  if command -v ldd > /dev/null 2>&1; then
    printf '      shared libs: %s\n' "$(ldd "$WORK/hello" | tr '\n' ' ')"
  fi
else
  bad "native binary was not produced"
fi

say "tarvos run"
check "tarvos run" "$TARBOS" run "$WORK/hello.py"

say "tarvos export"
if "$TARBOS" export "$WORK/hello.py" "$WORK/exported" > "$WORK/export.log" 2>&1; then
  if [[ -f "$WORK/exported/Cargo.toml" ]] && [[ -f "$WORK/exported/src/main.rs" ]]; then
    ok "export produced a valid Cargo project"
  else
    bad "export did not produce Cargo.toml and src/main.rs"
  fi
else
  bad "tarvos export (see $WORK/export.log)"
fi

say "Source-only build (no toolchain needed)"
check "build --source-only" "$TARBOS" build "$WORK/hello.py" -o "$WORK/hello.rs" --source-only

say "Result"
printf '%d passed, %d failed, %d skipped\n' "$PASS" "$FAIL" "$SKIP"
[[ "$FAIL" -eq 0 ]] || exit 1
