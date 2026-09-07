#!/usr/bin/env bash
# Exit 0 if the segregated native toolchain is ready (pinned nightly in the dedicated store +
# msp430-elf-gcc on PATH), else 1. Quiet — used by `just diag build` to pick native vs Docker.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
store="${MSP430_RUST_HOME:-$HOME/.local/share/msp430-rust}"
tc="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$repo_root/diag/rust-toolchain.toml")"

command -v msp430-elf-gcc >/dev/null 2>&1 || exit 1
command -v rustup >/dev/null 2>&1 || exit 1
[ -d "$store/rustup/toolchains" ] || exit 1
RUSTUP_HOME="$store/rustup" CARGO_HOME="$store/cargo" rustup run "$tc" rustc --version >/dev/null 2>&1 || exit 1
