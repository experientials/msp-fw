#!/usr/bin/env bash
# Fetch a prebuilt msp430-elf-gcc (published by .github/workflows/toolchain-msp430-gcc.yml) for this
# host arch and extract it — so a Raspberry Pi / Linux node installs the toolchain in seconds instead
# of compiling it for ~30–60 min. See references/native-build-linux.md.
#
# Env: MSP430_GCC_REPO (owner/repo), MSP430_GCC_TAG (release tag), MSP430_GCC_DEST (extract parent).
set -euo pipefail

REPO="${MSP430_GCC_REPO:-experientials/msp-fw}"
TAG="${MSP430_GCC_TAG:-msp430-gcc-9.3.1}"
DEST="${MSP430_GCC_DEST:-/opt}"

case "$(uname -m)" in
  aarch64|arm64) arch=aarch64 ;;
  x86_64|amd64)  arch=x86_64 ;;
  *) echo "no prebuilt for arch $(uname -m) — build from source: scripts/build-msp430-gcc.sh" >&2; exit 1 ;;
esac

asset="msp430-elf-gcc-9.3.1-linux-${arch}.tar.xz"
url="https://github.com/$REPO/releases/download/$TAG/$asset"
SUDO=""; [ -w "$DEST" ] || SUDO="sudo"

echo "fetching $url"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
curl -fL "$url" -o "$tmp/$asset"
$SUDO tar -C "$DEST" -xf "$tmp/$asset"

echo "✅ installed at $DEST/msp430-gcc"
echo "   add to PATH:  export PATH=$DEST/msp430-gcc/bin:\$PATH"
"$DEST/msp430-gcc/bin/msp430-elf-gcc" --version | head -1 || true
