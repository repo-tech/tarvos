#!/usr/bin/env bash
set -euo pipefail

echo "=== Tarvos Cross-Platform Installer ==="

# Check for Rust toolchain
if ! command -v cargo &> /dev/null; then
    echo "[1] Cargo not found. Installing Rust toolchain via rustup..."
    # `--tl1v1.2` was a typo for `--tlsv1.2` (an `s` written as `1`). curl
    # rejected the whole command with "option '--ttl1.2' is unknown", so the
    # toolchain step never ran and the install died with no useful output.
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "[+] Building and installing tarvos CLI..."
# This runs from the user's current directory, which is almost never a Tarvos
# checkout. Before this, `cargo install --path crates/tarvos-cli` failed with a
# "path not found" error that told the user nothing about what was wrong.
if [ ! -d "crates/tarvos-cli" ]; then
    echo "[!] No Tarvos source tree found in $(pwd)." >&2
    echo "    Run this script from a Tarvos checkout, or install a release" >&2
    echo "    build from https://github.com/repo-tech/Tarvos/releases" >&2
    exit 1
fi
cargo install --locked --path crates/tarvos-cli --force

echo ""
echo "[?] Tarvos installed successfully!"
echo "Run 'tarvos --help' to get started."
