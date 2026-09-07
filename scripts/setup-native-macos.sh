#!/usr/bin/env bash
# Set up a NATIVE macOS build of the msp-fw Rust firmware — no amd64 Docker emulation.
#
# Segregation is the whole point: the pinned Rust nightly lives in a DEDICATED toolchain home
# (default ~/.local/share/msp430-rust), NEVER in ~/.rustup and NEVER as the system-default rustc.
# Your stray Homebrew `rust 1.64` at /usr/local/bin is left completely untouched — this toolchain is
# only ever reached through `just diag build` (which exports the env below into its own subshell).
#
# The msp430-elf-gcc LINKER is a uniquely-named cross-tool (shadows no system compiler), so it's fine
# to install it via Homebrew natively. Flashing (mspdebug + libmsp430.dylib, x86_64/Rosetta) is a
# SEPARATE concern handled by the msp430-macos-dev skill's setup-macos.sh — not this script.
#
# Idempotent: safe to re-run. Reads the pinned channel from diag/rust-toolchain.toml (one source of
# truth, shared with the Docker image's RUST_TOOLCHAIN).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
store="${MSP430_RUST_HOME:-$HOME/.local/share/msp430-rust}"
toolchain_file="$repo_root/diag/rust-toolchain.toml"

# --- pinned channel: single source of truth (same file the Dockerfile pins to) --------------------
tc="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' "$toolchain_file")"
[ -n "$tc" ] || { echo "ERROR: could not read channel from $toolchain_file" >&2; exit 1; }
echo "== pinned Rust toolchain: $tc =="
echo "== segregated store:      $store =="

# --- preflight: rustup + brew must exist (we don't silently install system tools) -----------------
command -v rustup >/dev/null || {
  echo "ERROR: rustup not found. Install it WITHOUT a default toolchain so nothing global changes:" >&2
  echo "  brew install rustup                 # or: curl https://sh.rustup.rs | sh -s -- --default-toolchain none" >&2
  exit 1
}
command -v brew >/dev/null || { echo "ERROR: Homebrew not found (needed for msp430-elf-gcc)." >&2; exit 1; }

# --- 1) install the pinned nightly + rust-src INTO THE DEDICATED HOME ------------------------------
# Overriding RUSTUP_HOME/CARGO_HOME here means this writes only under $store — ~/.rustup is not
# touched, and we never run `rustup default`, so the system default rustc is unaffected.
export RUSTUP_HOME="$store/rustup"
export CARGO_HOME="$store/cargo"
mkdir -p "$RUSTUP_HOME" "$CARGO_HOME"
echo "== installing $tc (+ rust-src) into the dedicated home =="
rustup toolchain install "$tc" --profile minimal --component rust-src --no-self-update
rustup run "$tc" rustc --version

# --- 2) install the native msp430-elf-gcc linker (uniquely-named cross-tool) -----------------------
# The tgtakaoka tap formulae have two bugs on modern macOS (neither is a compiler/Xcode issue — no
# Xcode upgrade needed; see the skill's macos-setup.md):
#   BUG1: they build info docs needing `makeinfo` (absent on macOS) → binutils dies at bfd.info.
#         The docs are deleted anyway, so we patch `make` → `make MAKEINFO=true` to skip them.
#   BUG2: the binutils formula writes into /opt/homebrew/lib (outside its keg) → Homebrew's build
#         SANDBOX EPERMs → we install with HOMEBREW_NO_SANDBOX=1.
# The formula edits are local (a `brew update` reverts them) but the built keg persists.
if ! command -v msp430-elf-gcc >/dev/null 2>&1; then
  echo "== installing msp430-elf-gcc (tgtakaoka tap, ~20-40 min from source) =="
  brew tap tgtakaoka/msp430-elf
  brew trust tgtakaoka/msp430-elf 2>/dev/null || true
  tapdir="$(brew --repo tgtakaoka/msp430-elf)"
  for f in binutils-msp430-elf gcc-msp430-elf; do
    rb="$tapdir/$f.rb"
    if [ -f "$rb" ] && ! grep -q 'MAKEINFO=true' "$rb"; then   # idempotent
      sed -i '' -e 's/system "make", "install"/system "make", "install", "MAKEINFO=true"/' \
                -e 's/system "make"$/system "make", "MAKEINFO=true"/' "$rb"
    fi
  done
  HOMEBREW_NO_SANDBOX=1 brew install --build-from-source gcc-msp430-elf gdb-msp430-elf
fi
echo "linker: $(command -v msp430-elf-gcc) -> $(msp430-elf-gcc --version | head -1)"

# --- 3) smoke-build the simplest firmware natively, proving the whole path works -------------------
echo "== native smoke build: examples/hello-rust =="
( cd "$repo_root/examples/hello-rust" && rustup run "$tc" cargo build --release )

cat <<EOF

✅ Native toolchain ready — segregated in $store (NOT the system default).
   Build:  just diag build                   (native once set up; 'just diag dev' = fast build+flash)
   Check:  just diag doctor                  (verifies segregation held)
   Flash:  just diag flash                   (unchanged; msp430-macos-dev skill, Rosetta mspdebug)
   Docker build ('just diag build') is untouched and remains canonical for release/CI.
EOF
