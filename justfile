# msp-fw command runner.
#
# Build recipes run inside the toolchain container. On your Mac they shell into
# Docker; in CI (already running inside the image) they build natively — the
# recipe detects /.dockerenv, so local and CI run the exact same code. No drift.
#
# Flash/probe run on the macOS HOST (USB) and need the one-time mspdebug setup
# from the msp430-macos-dev skill (x86_64 mspdebug + signed libmsp430.dylib).
#
#   just                       list recipes
#   just build                 build every example
#   just probe                 read the connected FR2476 id (no flash)
#   just check deps            verify the toolchain
#   just example build <name>  build one example (C make / Rust cargo, auto-detected)
#   just example run   <name>  build + flash one example

# Default to the locally-built image. Only the HOST uses this (CI builds inside the
# image, so it never pulls). Once toolchain-image.yml pushes to GHCR, you can set
# MSP430_IMAGE=ghcr.io/experientials/msp430-toolchain:latest to skip `just bootstrap`.
image    := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")
mspdebug := env_var_or_default("MSPDEBUG_BIN", "/usr/local/bin/mspdebug")
libdir   := env_var_or_default("MSP430_LIBDIR", "/usr/local/lib")

# subcommand groups:  just check deps  |  just example build <name>
mod check
mod example
mod pac
mod diag
mod usb

default:
    @just --list

# build the toolchain Docker image locally (run once)
bootstrap:
    docker build --platform linux/amd64 -t {{image}} docker/

# build every example (in the container on a host; natively in CI)
build:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f /.dockerenv ]; then
        for d in examples/*/; do just example build "$(basename "$d")"; done
    else
        docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work {{image}} just build
    fi

# read the connected FR2476 device id (no flash)
probe:
    DYLD_FALLBACK_LIBRARY_PATH={{libdir}} {{mspdebug}} tilib exit

# watch the UART console (eZ-FET backchannel, 9600 8N1). Ctrl-C to stop.
# No arg = auto-pick the highest-numbered usbmodem (the eZ-FET backchannel; the numbers
# change on every re-enumeration). If that one is silent, pass the other explicitly:
#   just monitor /dev/cu.usbmodem23601
# One-way read (firmware only prints) — plain `cat`, so no screen / detached port holders.
monitor port="":
    #!/usr/bin/env bash
    set -euo pipefail
    p="{{port}}"
    if [ -z "$p" ]; then
        p=$(ls -1 /dev/cu.usbmodem* 2>/dev/null | sort | tail -1)
        [ -n "$p" ] || { echo "no /dev/cu.usbmodem* found — run 'just usb status'" >&2; exit 1; }
    fi
    echo "monitoring $p @9600 — Ctrl-C to stop"
    stty -f "$p" 9600 cs8 -cstopb -parenb raw -echo
    exec cat "$p"

# remove all build artifacts
clean:
    rm -rf examples/*/build examples/*/target
