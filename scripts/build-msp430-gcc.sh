#!/usr/bin/env bash
# Build msp430-elf-gcc from source into a relocatable prefix — Linux AND macOS.
#
# Recipe = the tgtakaoka Homebrew formula's sources, verbatim: GNU **binutils 2.34 + gcc 9.3.0 +
# newlib 2.4.0**, all patched with **TI's MSP430-GCC 9.3.1 source patches**, plus TI's device support
# files. We build directly (not via `brew install`) because the tap formula is incompatible with
# modern macOS Homebrew (it writes outside its keg → the mandatory install sandbox EPERMs, and that
# sandbox can't be disabled on macOS). Building ourselves sidesteps Homebrew entirely.
#
# `MAKEINFO=true` skips the info-doc build (needs `makeinfo`/texinfo, absent on macOS) — the docs
# are unused. So this needs NO texinfo and NO newer Xcode; the compiler was never the problem.
#
# Used by CI (.github/workflows/toolchain-msp430-gcc.yml) for Linux tarballs, on a Pi, and on macOS.
# Env: PREFIX (install dir), JOBS, WORK (build dir). On macOS, gmp/mpfr/libmpc/isl come from brew.
set -euo pipefail

OS="$(uname -s)"
PREFIX="${PREFIX:-/opt/msp430-gcc}"
JOBS="${JOBS:-$( (command -v nproc >/dev/null && nproc) || sysctl -n hw.ncpu 2>/dev/null || echo 4 )}"
WORK="${WORK:-$PWD/msp430-gcc-build}"
TARGET=msp430-elf

BINUTILS=binutils-2.34
GCC=gcc-9.3.0
NEWLIB=newlib-2.4.0
TI_BASE="https://software-dl.ti.com/msp430/msp430_public_sw/mcu/msp430/MSPGCC/9_3_1_2/export"
PATCHES_TB="msp430-gcc-9.3.1.11-source-patches.tar.bz2"
SUPPORT_ZIP="msp430-gcc-support-files-1.212.zip"

mkdir -p "$(dirname "$PREFIX")" 2>/dev/null || true
# apt needs root; `make install` needs root only if the prefix's parent isn't ours.
apt_sudo=""; [ "$(id -u)" = 0 ] || apt_sudo="sudo"
inst_sudo=""; { [ -w "$(dirname "$PREFIX")" ] || [ "$(id -u)" = 0 ]; } || inst_sudo="sudo"

# macOS has TWO Homebrews (arm64 /opt/homebrew, Intel /usr/local). A NATIVE build needs the libs
# from the matching-arch one, and plain `brew` on PATH is often the Intel one — which would make
# gcc's --with-gmp point where no arm64 libs exist. Pin the native-arch brew explicitly.
BREW=""
if [ "$OS" = Darwin ]; then
  if [ "$(uname -m)" = arm64 ] && [ -x /opt/homebrew/bin/brew ]; then BREW=/opt/homebrew/bin/brew
  elif [ -x /usr/local/bin/brew ]; then BREW=/usr/local/bin/brew
  else BREW="$(command -v brew || true)"; fi
  [ -n "$BREW" ] || { echo "Homebrew required on macOS (brew.sh)" >&2; exit 1; }
fi

install_deps() {
  case "$OS" in
    Darwin)
      "$BREW" install gmp mpfr libmpc isl >/dev/null 2>&1 || "$BREW" install gmp mpfr libmpc isl
      ;;
    *)
      command -v apt-get >/dev/null || return 0
      $apt_sudo apt-get update
      $apt_sudo apt-get install -y --no-install-recommends \
        build-essential texinfo bison flex libgmp-dev libmpfr-dev libmpc-dev libisl-dev \
        zlib1g-dev wget unzip bzip2 xz-utils ca-certificates patch
      ;;
  esac
}

fetch() { local f; f="$(basename "$1")"; [ -f "$f" ] || curl -fL "$1" -o "$f"; }

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
  cp -a "$NEWLIB/newlib" "$NEWLIB/libgloss" "$GCC/"   # staged into the gcc tree, as the formula does

  echo "== binutils =="
  mkdir build-binutils && ( cd build-binutils &&
    "../$BINUTILS/configure" --target=$TARGET --program-prefix=$TARGET- --prefix="$PREFIX" \
      --enable-languages=c,c++ --disable-nls --enable-inifini-array --disable-sim --disable-gdb \
      --disable-werror --with-system-zlib &&
    make -j"$JOBS" MAKEINFO=true && $inst_sudo make install MAKEINFO=true )
  export PATH="$PREFIX/bin:$PATH"

  # On macOS the GMP/MPFR/MPC/ISL that gcc needs live under Homebrew, not the default search path.
  # Plain (unquoted) string, not an array — avoids macOS bash 3.2's empty-array-under-`set -u` error.
  # Homebrew paths have no spaces, so word-splitting is safe here.
  local gmp_flags=""
  if [ "$OS" = Darwin ]; then
    local bp; bp="$("$BREW" --prefix)"   # the NATIVE-arch Homebrew (see BREW resolution above)
    gmp_flags="--with-gmp=$bp/opt/gmp --with-mpfr=$bp/opt/mpfr --with-mpc=$bp/opt/libmpc --with-isl=$bp/opt/isl"
  fi

  echo "== gcc + newlib =="
  # shellcheck disable=SC2086
  mkdir build-gcc && ( cd build-gcc &&
    "../$GCC/configure" --target=$TARGET --program-prefix=$TARGET- --prefix="$PREFIX" \
      --enable-languages=c,c++ --disable-nls --enable-inifini-array --enable-target-optspace \
      --enable-newlib-nano-formatted-io --with-system-zlib $gmp_flags \
      --with-as="$PREFIX/bin/$TARGET-as" --with-ld="$PREFIX/bin/$TARGET-ld" &&
    make -j"$JOBS" MAKEINFO=true && $inst_sudo make install MAKEINFO=true )

  echo "== device support files (headers + linker scripts) =="
  mkdir support && ( cd support && unzip -q "../$SUPPORT_ZIP" )
  local sfinc; sfinc="$(dirname "$(find support -name 'msp430fr2476.ld' | head -1)")"
  [ -n "$sfinc" ] && { $inst_sudo mkdir -p "$PREFIX/include"; $inst_sudo cp -a "$sfinc/." "$PREFIX/include/"; }

  echo "✅ built msp430-elf-gcc at $PREFIX"
  "$PREFIX/bin/$TARGET-gcc" --version | head -1
}

main "$@"
