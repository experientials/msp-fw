# `just diag <verb>` — build/flash the diagnostic ROM (top-level diag/, Rust on the FR2476 PAC).
#
#   just diag build     build the ELF (in the container on a host; native in CI)
#   just diag flash     program the connected board (host; needs msp430-macos-dev setup)
#   just diag run       build + flash
#
# Then watch the POST over the backchannel UART with the top-level `just monitor`.

image    := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")
mspdebug := env_var_or_default("MSPDEBUG_BIN", "/usr/local/bin/mspdebug")
libdir   := env_var_or_default("MSP430_LIBDIR", "/usr/local/lib")

# build diag (Rust). Native inside the image; Docker-wrapped on a host.
build:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f /.dockerenv ]; then
        ( cd diag && cargo build --release && msp430-elf-size target/msp430-none-elf/release/diag )
    else
        docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work {{image}} just diag build
    fi

# flash diag's built ELF to the connected board (host; needs msp430-macos-dev setup)
flash:
    #!/usr/bin/env bash
    set -euo pipefail
    elf="diag/target/msp430-none-elf/release/diag"
    [ -f "$elf" ] || { echo "no built ELF — run 'just diag build' first" >&2; exit 1; }
    echo "flashing $elf"
    # program then detach: on release tilib runs the target (MSP430_Run), so the board
    # executes the new firmware and this returns immediately.
    DYLD_FALLBACK_LIBRARY_PATH={{libdir}} {{mspdebug}} tilib "prog $elf" exit

# build then flash
run: build flash
