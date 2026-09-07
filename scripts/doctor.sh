#!/usr/bin/env bash
# Prove the native toolchain is correctly SEGREGATED: the pinned nightly is reachable for builds,
# and it is NOT the system default rustc. Invoked by `just diag doctor`. Non-zero exit on problems.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
store="${MSP430_RUST_HOME:-$HOME/.local/share/msp430-rust}"
tc="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$repo_root/diag/rust-toolchain.toml")"
fail=0

echo "pinned channel:   $tc"
echo "segregated store: $store"

# 1) System default rustc must NOT be our nightly — that's the segregation guarantee.
sys_rustc="$(command -v rustc || true)"
sys_ver="$([ -n "$sys_rustc" ] && "$sys_rustc" --version 2>/dev/null || echo '<none>')"
echo "system rustc:     ${sys_rustc:-<none>}  ($sys_ver)"
if printf '%s' "$sys_ver" | grep -q "$tc"; then
  echo "  ✗ system default rustc IS the pinned nightly — segregation LEAKED (did something run 'rustup default $tc'?)"
  fail=1
else
  echo "  ✓ system default rustc is not the msp-fw nightly"
fi
case "$sys_rustc" in
  */Cellar/rust/*|/usr/local/bin/rustc)
    echo "  ℹ note: that's the old Homebrew 'rust' — harmless (native build never uses it); 'brew uninstall rust' if unused elsewhere" ;;
esac

# 2) The pinned nightly + rust-src must exist in the DEDICATED store (not ~/.rustup).
if [ -d "$store/rustup/toolchains" ] && \
   RUSTUP_HOME="$store/rustup" CARGO_HOME="$store/cargo" rustup run "$tc" rustc --version >/dev/null 2>&1; then
  echo "  ✓ pinned nightly in dedicated store: $(RUSTUP_HOME="$store/rustup" CARGO_HOME="$store/cargo" rustup run "$tc" rustc --version)"
else
  echo "  ✗ pinned nightly NOT in dedicated store — run: bash scripts/setup-native-macos.sh"
  fail=1
fi

# 3) The linker.
if command -v msp430-elf-gcc >/dev/null; then
  echo "  ✓ linker: $(command -v msp430-elf-gcc)  ($(msp430-elf-gcc --version | head -1))"
else
  echo "  ✗ msp430-elf-gcc not found — run: bash scripts/setup-native-macos.sh"
  fail=1
fi

if [ "$fail" = 0 ]; then echo "doctor: OK — segregation held"; else echo "doctor: PROBLEMS (see ✗ above)"; exit 1; fi
