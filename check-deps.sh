#!/bin/sh
# check-deps.sh — verify the MSP430 firmware toolchain is present.
#
# Portable POSIX sh: runs on macOS, Linux CI, and minimal Docker (BusyBox/dash).
# Fast by default (presence checks only) so it's cheap to run often.
#
#   ./check-deps.sh            build toolchain (compiler, binutils, make, support files)
#   ./check-deps.sh --flash    also check hardware-programming tools (mspdebug)
#   ./check-deps.sh --deep     authoritative check: actually compile a tiny ELF
#   ./check-deps.sh --quiet    print only problems (exit code still signals result)
#   ./check-deps.sh --help
#
# Exit 0 = all required deps satisfied, non-zero = something required is missing.
#
# Env overrides:
#   MSP430_SUPPORT / SUPPORT   path to the msp430-gcc support-files include dir
#                              (contains msp430fr2476.ld and the device headers)

set -u

FLASH=0
DEEP=0
QUIET=0
DEVICE=msp430fr2476
FAIL=0

for arg in "$@"; do
    case "$arg" in
        --flash) FLASH=1 ;;
        --deep)  DEEP=1 ;;
        --quiet) QUIET=1 ;;
        -h|--help)
            sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) printf 'unknown option: %s (try --help)\n' "$arg" >&2; exit 2 ;;
    esac
done

# --- output helpers (color only on a TTY, honoring NO_COLOR) ---
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_G=$(printf '\033[32m'); C_R=$(printf '\033[31m')
    C_Y=$(printf '\033[33m'); C_B=$(printf '\033[1m'); C_N=$(printf '\033[0m')
else
    C_G=; C_R=; C_Y=; C_B=; C_N=
fi
ok()   { [ "$QUIET" -eq 1 ] || printf '  %s[ ok ]%s %s\n' "$C_G" "$C_N" "$*"; }
info() { [ "$QUIET" -eq 1 ] || printf '  %s[info]%s %s\n' "$C_B" "$C_N" "$*"; }
warn() { printf '  %s[warn]%s %s\n' "$C_Y" "$C_N" "$*"; }
miss() { printf '  %s[MISS]%s %s\n' "$C_R" "$C_N" "$*"; FAIL=1; }
have() { command -v "$1" >/dev/null 2>&1; }

OS=$(uname -s 2>/dev/null || echo unknown)
ARCH=$(uname -m 2>/dev/null || echo unknown)
ctx="$OS/$ARCH"
[ -f /.dockerenv ] && ctx="$ctx (docker)"
[ -n "${CI:-}" ] && ctx="$ctx (ci)"
info "context: $ctx"

# --- command runner (required on every host) ---
if have just; then
    ok "just $(just --version 2>/dev/null | awk '{print $2}')"
else
    miss "just not found  (command runner; install: brew install just | cargo install just | https://just.systems)"
fi

# --- build capability: a native toolchain OR docker (we build in a container) ---
NATIVE=0
if have msp430-elf-gcc; then
    ok "msp430-elf-gcc $(msp430-elf-gcc -dumpversion 2>/dev/null) (native)"
    NATIVE=1
elif have docker; then
    ok "docker $(docker --version 2>/dev/null | awk '{print $3}' | tr -d ,) — firmware builds in a container; native msp430-elf-gcc not required on this host"
else
    miss "no build path: install docker (build in a container) OR msp430-elf-gcc (native / inside the image)"
fi

# --- build-env tools (only meaningful with a native toolchain: in-container or bare-metal) ---
if [ "$NATIVE" -eq 1 ]; then
    if have make; then ok "make"; else miss "make not found"; fi
    if have msp430-elf-size; then
        ok "msp430-elf-size (binutils)"
    else
        warn "msp430-elf-size not found — the Makefile size report will be skipped"
    fi

    SUPPORT_DIR=
    for d in "${MSP430_SUPPORT:-}" "${SUPPORT:-}" \
             /opt/homebrew/opt/headers-msp430-elf/include \
             /usr/local/opt/headers-msp430-elf/include \
             /opt/msp430-gcc-support-files/include \
             /usr/lib/msp430-elf/include \
             /usr/local/msp430-elf/include \
             /usr/msp430-elf/include; do
        [ -n "$d" ] || continue
        if [ -f "$d/$DEVICE.ld" ]; then SUPPORT_DIR="$d"; break; fi
    done
    if [ -n "$SUPPORT_DIR" ]; then
        ok "support files ($DEVICE.ld) at $SUPPORT_DIR"
    else
        warn "support files ($DEVICE.ld) not found — set MSP430_SUPPORT (confirm with --deep)"
    fi
else
    info "native build tools (make, binutils, support files) come from the build image — run --deep inside the container to verify them"
fi

# --- flash tools (only when programming real hardware) ---
if [ "$FLASH" -eq 1 ]; then
    if have mspdebug; then
        ok "mspdebug"
        if [ "$OS" = Darwin ] && [ "$ARCH" = arm64 ]; then
            # TI ships libmsp430.dylib as x86_64 only; flashing goes through the
            # skill's wrapper (x86_64 mspdebug under Rosetta + DYLD paths).
            if have file && file "$(command -v mspdebug)" 2>/dev/null | grep -q x86_64; then
                ok "mspdebug is x86_64 (loads libmsp430.dylib on Apple Silicon)"
            else
                warn "on Apple Silicon, flash via the msp430-macos-dev skill's mspdebug-macos.sh (needs x86_64 mspdebug + libmsp430.dylib)"
            fi
        fi
    else
        miss "mspdebug not found (required with --flash)"
    fi
fi

# --- deep check: prove the toolchain can actually produce an ELF ---
if [ "$DEEP" -eq 1 ] && [ "$NATIVE" -eq 0 ]; then
    warn "--deep skipped: no native msp430-elf-gcc here. Run it inside the build container, e.g. 'docker run --rm -v \"\$PWD\":/src -w /src <image> ./check-deps.sh --deep'"
fi
if [ "$DEEP" -eq 1 ] && [ "$NATIVE" -eq 1 ]; then
    tmp=$(mktemp -d 2>/dev/null || echo /tmp/mspdep.$$)
    mkdir -p "$tmp" 2>/dev/null
    trap 'rm -rf "$tmp"' EXIT INT TERM
    printf '#include <msp430.h>\nint main(void){WDTCTL=WDTPW|WDTHOLD;return 0;}\n' > "$tmp/t.c"
    inc=""; lib=""
    [ -n "$SUPPORT_DIR" ] && { inc="-I$SUPPORT_DIR"; lib="-L$SUPPORT_DIR"; }
    if msp430-elf-gcc -mmcu="$DEVICE" -Os $inc $lib -T "$DEVICE.ld" \
         "$tmp/t.c" -o "$tmp/t.elf" >"$tmp/err" 2>&1; then
        ok "trial compile for $DEVICE succeeded"
    else
        miss "trial compile for $DEVICE failed:"
        sed 's/^/        /' "$tmp/err" >&2
    fi
fi

# --- verdict ---
if [ "$FAIL" -eq 0 ]; then
    [ "$QUIET" -eq 1 ] || printf '%s✓ all required dependencies present%s\n' "$C_G" "$C_N"
    exit 0
else
    printf '%s✗ missing required dependencies%s\n' "$C_R" "$C_N" >&2
    exit 1
fi
