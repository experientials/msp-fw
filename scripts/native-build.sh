#!/usr/bin/env bash
# Native, SEGREGATED diag build — no Docker/amd64 emulation. Invoked by `just diag build` (native
# path) and `just diag dev`.
#
# The pinned nightly is used ONLY through the dedicated RUSTUP_HOME/CARGO_HOME below; this never
# touches ~/.rustup and never becomes the system default rustc. Docker remains the canonical build
# (`just diag build`); this is the fast local alternative. See scripts/setup-native-macos.sh.
#
# Arg 1: cargo profile — "release" (default, LTO) or "fast" (no-LTO, quick iteration).
set -euo pipefail

profile="${1:-release}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
store="${MSP430_RUST_HOME:-$HOME/.local/share/msp430-rust}"
tc="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$repo_root/diag/rust-toolchain.toml")"

export RUSTUP_HOME="$store/rustup"
export CARGO_HOME="$store/cargo"
[ -d "$RUSTUP_HOME/toolchains" ] || { echo "native toolchain missing — run: bash scripts/setup-native-macos.sh" >&2; exit 1; }
command -v msp430-elf-gcc >/dev/null || { echo "msp430-elf-gcc missing — run: bash scripts/setup-native-macos.sh" >&2; exit 1; }

if [ "$profile" = "release" ]; then flag="--release"; else flag="--profile $profile"; fi
# shellcheck disable=SC2086
( cd "$repo_root/diag" && rustup run "$tc" cargo build $flag )
msp430-elf-size "$repo_root/diag/target/msp430-none-elf/$profile/diag"
