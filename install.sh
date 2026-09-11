#!/usr/bin/env bash
set -euo pipefail

echo "=== Tarvos Cross-Platform Installer ==="

# Check for Rust toolchain
if ! command -v cargo &> /dev/null; then
    echo "[1] Cargo not found. Installing Rust toolchain via rustup..."
    curl --proto '=https' --tl1v1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "[+] Building and installing tarvos CLI..."
cargo install --locked --path crates/tarvos-cli --force

echo ""
echo "[?] Tarvos installed successfully!"
echo "Run 'tarvos --help' to get started."
