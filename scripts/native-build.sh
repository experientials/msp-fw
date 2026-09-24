#!/usr/bin/env bash
# Native, SEGREGATED diag build — no Docker/amd64 emulation. Invoked by `just diag build` (native
# path) and `just diag dev`.
#
# The pinned nightly is used ONLY through the dedicated RUSTUP_HOME/CARGO_HOME below; this never
# touches ~/.rustup and never becomes the system default rustc. Docker remains the canonical build
# (`just diag build`); this is the fast local alternative. See scripts/setup-native-macos.sh.
#
# Arg 1: cargo profile — "release" (default, LTO) or "fast" (no-LTO, quick iteration).
# Arg 2: crate dir — "diag" (default) or "prod". Builds <crate>/ with the pinned nightly.
# Arg 3: extra cargo flags (optional), e.g. "--no-default-features --features fr247x" for prod's
#        family axis. Word-split intentionally (multiple flags in one arg).
# The footprint gate is applied by the caller (`just <crate> build`), not here.
set -euo pipefail

profile="${1:-release}"
crate="${2:-diag}"
extra="${3:-}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
store="${MSP430_RUST_HOME:-$HOME/.local/share/msp430-rust}"
tc="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$repo_root/$crate/rust-toolchain.toml")"

export RUSTUP_HOME="$store/rustup"
export CARGO_HOME="$store/cargo"
[ -d "$RUSTUP_HOME/toolchains" ] || { echo "native toolchain missing — run: bash scripts/setup-native-macos.sh" >&2; exit 1; }
command -v msp430-elf-gcc >/dev/null || { echo "msp430-elf-gcc missing — run: bash scripts/setup-native-macos.sh" >&2; exit 1; }

if [ "$profile" = "release" ]; then flag="--release"; else flag="--profile $profile"; fi
# shellcheck disable=SC2086
( cd "$repo_root/$crate" && rustup run "$tc" cargo build $flag $extra )
