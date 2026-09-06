#!/usr/bin/env bash
# Build msp430-elf-gcc from source into a relocatable prefix.
#
# Recipe = the tgtakaoka Homebrew formula, verbatim: GNU **binutils 2.34 + gcc 9.3.0 + newlib 2.4.0**,
# all patched with **TI's MSP430-GCC 9.3.1 source patches**, plus TI's device support files. This is
# the toolchain for Linux hosts where TI ships no prebuilt — notably **arm64 / Raspberry Pi**.
#
# Used by CI (.github/workflows/toolchain-msp430-gcc.yml) to publish per-arch tarballs, and runnable
# locally on a Pi. Reproducible: upstream versions + the TI patch set are pinned below.
#
# Env: PREFIX (install dir, default /opt/msp430-gcc), JOBS (default nproc), WORK (build dir).
set -euo pipefail

PREFIX="${PREFIX:-/opt/msp430-gcc}"
JOBS="${JOBS:-$(nproc)}"
WORK="${WORK:-$PWD/msp430-gcc-build}"
TARGET=msp430-elf

BINUTILS=binutils-2.34
GCC=gcc-9.3.0
NEWLIB=newlib-2.4.0
TI_BASE="https://software-dl.ti.com/msp430/msp430_public_sw/mcu/msp430/MSPGCC/9_3_1_2/export"
PATCHES_TB="msp430-gcc-9.3.1.11-source-patches.tar.bz2"
SUPPORT_ZIP="msp430-gcc-support-files-1.212.zip"

# sudo only if we can't write PREFIX's parent ourselves (CI runs as a user with passwordless sudo;
# a rootless container may already own /opt).
SUDO=""; [ -w "$(dirname "$PREFIX")" ] || SUDO="sudo"

install_deps() {
  command -v apt-get >/dev/null || return 0
  $SUDO apt-get update
  $SUDO apt-get install -y --no-install-recommends \
    build-essential texinfo bison flex libgmp-dev libmpfr-dev libmpc-dev libisl-dev \
    zlib1g-dev wget unzip bzip2 xz-utils ca-certificates patch
}

fetch() { local f; f="$(basename "$1")"; [ -f "$f" ] || wget -q "$1" -O "$f"; }

main() {
  install_deps
  mkdir -p "$WORK"; cd "$WORK"

  echo "== fetch sources =="
  fetch "https://ftp.gnu.org/gnu/binutils/$BINUTILS.tar.bz2"
  fetch "https://ftp.gnu.org/gnu/gcc/$GCC/$GCC.tar.xz"
  fetch "https://sourceware.org/pub/newlib/$NEWLIB.tar.gz"
  fetch "$TI_BASE/$PATCHES_TB"
  fetch "$TI_BASE/$SUPPORT_ZIP"

  rm -rf "$BINUTILS" "$GCC" "$NEWLIB" patches build-binutils build-gcc support
  tar xf "$BINUTILS.tar.bz2"
  tar xf "$GCC.tar.xz"
  tar xf "$NEWLIB.tar.gz"
  mkdir -p patches && tar xf "$PATCHES_TB" -C patches

  # Locate the three TI patches regardless of the archive's internal directory name.
  local bpatch gpatch npatch
  bpatch="$(find "$PWD/patches" -name 'binutils-2_34.patch' | head -1)"
  gpatch="$(find "$PWD/patches" -name 'gcc-9.3.0.patch'     | head -1)"
  npatch="$(find "$PWD/patches" -name 'newlib-2_4_0.patch'  | head -1)"
  [ -n "$bpatch" ] && [ -n "$gpatch" ] && [ -n "$npatch" ] \
    || { echo "TI patches not found inside $PATCHES_TB" >&2; exit 1; }

  echo "== patch (TI 9.3.1 over upstream) =="
  ( cd "$BINUTILS" && patch -p0 < "$bpatch" )
  ( cd "$GCC"      && patch -p0 < "$gpatch" )
  ( cd "$NEWLIB"   && patch -p0 < "$npatch" )
  # newlib + libgloss staged into the gcc tree, exactly as the formula does.
  cp -a "$NEWLIB/newlib" "$NEWLIB/libgloss" "$GCC/"

  echo "== binutils =="
  mkdir build-binutils && ( cd build-binutils &&
    "../$BINUTILS/configure" --target=$TARGET --program-prefix=$TARGET- --prefix="$PREFIX" \
      --enable-languages=c,c++ --disable-nls --enable-inifini-array --disable-sim --disable-gdb \
      --disable-werror --with-system-zlib &&
    make -j"$JOBS" && $SUDO make install )
  export PATH="$PREFIX/bin:$PATH"

  echo "== gcc + newlib =="
  mkdir build-gcc && ( cd build-gcc &&
    "../$GCC/configure" --target=$TARGET --program-prefix=$TARGET- --prefix="$PREFIX" \
      --enable-languages=c,c++ --disable-nls --enable-inifini-array --enable-target-optspace \
      --enable-newlib-nano-formatted-io --with-system-zlib \
      --with-as="$PREFIX/bin/$TARGET-as" --with-ld="$PREFIX/bin/$TARGET-ld" &&
    make -j"$JOBS" && $SUDO make install )

  echo "== device support files (headers + linker scripts) =="
  mkdir support && ( cd support && unzip -q "../$SUPPORT_ZIP" )
  local sfinc; sfinc="$(dirname "$(find support -name 'msp430fr2476.ld' | head -1)")"
  [ -n "$sfinc" ] && { $SUDO mkdir -p "$PREFIX/include"; $SUDO cp -a "$sfinc/." "$PREFIX/include/"; }

  echo "✅ built msp430-elf-gcc at $PREFIX"
  "$PREFIX/bin/$TARGET-gcc" --version | head -1
}

main "$@"
