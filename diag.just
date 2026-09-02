# `just diag <verb>` — build/flash the diagnostic ROM (top-level diag/, Rust on the FR2476 PAC).
#
#   just diag build     build the ELF (in the container on a host; native in CI)
#   just diag flash     program the connected board (host; needs msp430-macos-dev setup)
#   just diag run       build + flash + VERIFY the running firmware reports the stamp we flashed
#
# `run` closes the "did my flash actually take?" gap: it stamps the build (DIAG_BUILD), flashes,
# then reads the backchannel UART and refuses to succeed until it sees that exact stamp come back.
# For a plain look at the POST, use the top-level `just monitor`.

image    := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")
mspdebug := env_var_or_default("MSPDEBUG_BIN", "/usr/local/bin/mspdebug")
libdir   := env_var_or_default("MSP430_LIBDIR", "/usr/local/lib")

# build diag (Rust). Native inside the image; Docker-wrapped on a host.
# `-e DIAG_BUILD` forwards the caller's stamp INTO the container so the firmware carries the
# exact value `run` will verify. Unset -> build.rs derives one from git + time (see build.rs).
build:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f /.dockerenv ]; then
        ( cd diag && cargo build --release && msp430-elf-size target/msp430-none-elf/release/diag )
    else
        docker run --rm --platform linux/amd64 -e DIAG_BUILD -v "$PWD":/work -w /work {{image}} just diag build
    fi

# Note: stress is no longer a separate build — it's a runtime task selected from the OLED menu
# (S1 opens the menu, S2 cycles to "I2C MARGIN"/"I2C SOAK", S1 selects). One unified `just diag run`.

# flash diag's built ELF to the connected board (host; needs msp430-macos-dev setup)
flash:
    #!/usr/bin/env bash
    set -euo pipefail
    elf="diag/target/msp430-none-elf/release/diag"
    [ -f "$elf" ] || { echo "no built ELF — run 'just diag build' first" >&2; exit 1; }
    echo "flashing $elf"
    # Clear a stale FET holder first: a lingering mspdebug (or a `just diag run`/`just monitor`
    # that didn't release) makes tilib fail with "No unused FET found". Cheap self-heal.
    if pgrep -x mspdebug >/dev/null 2>&1; then echo "  freeing stale mspdebug"; pkill -x mspdebug || true; sleep 1; fi
    # program then detach: on release tilib runs the target (MSP430_Run), so the board
    # executes the new firmware and this returns immediately.
    DYLD_FALLBACK_LIBRARY_PATH={{libdir}} {{mspdebug}} tilib "prog $elf" exit

# build + flash, then VERIFY the running firmware reports the exact stamp we just flashed.
# The verification is the point: a successful `prog` does NOT prove the board rebooted into the
# new image, so we read the UART and wait to SEE the stamp. No stamp in the window => treat it as
# stale/failed and exit non-zero, rather than reporting a false success.
run:
    #!/usr/bin/env bash
    set -euo pipefail
    hash="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)"
    if [ -n "$(git status --porcelain 2>/dev/null)" ]; then dirty="-dirty"; else dirty=""; fi
    stamp="$(date -u +%Y%m%d-%H%M%S)-${hash}${dirty}"
    echo "== build stamp: $stamp =="
    export DIAG_BUILD="$stamp"
    just diag build
    just diag flash
    # Verify over the backchannel UART. macOS has no `timeout`, so read into a temp file in the
    # background and poll it; the stamp rides every POST banner (~3 s), so a short window suffices.
    port="$(ls -1 /dev/cu.usbmodem* 2>/dev/null | sort | tail -1)"
    [ -n "$port" ] || { echo "no /dev/cu.usbmodem* — can't verify; run 'just usb status'" >&2; exit 1; }
    stty -f "$port" 9600 cs8 -cstopb -parenb raw -echo
    tmp="$(mktemp)"; cat "$port" > "$tmp" & catpid=$!
    echo "verifying $port reports build $stamp (up to 15 s)..."
    found=""
    for _ in $(seq 1 15); do
        if grep -q "$stamp" "$tmp" 2>/dev/null; then found=1; break; fi
        sleep 1
    done
    kill "$catpid" 2>/dev/null || true; rm -f "$tmp"
    if [ -n "$found" ]; then
        echo "✓ CONFIRMED running: build $stamp"
    else
        echo "✗ did NOT see build $stamp within 15 s — board is likely running STALE firmware." >&2
        echo "  Check the flash output above for erase/write/verify lines (not just 'run'); if the" >&2
        echo "  chip id was 'unknown' and no write happened, the old FRAM image is still live." >&2
        exit 1
    fi

# HIL: flash an ALREADY-BUILT ELF and assert the POST over UART. Used by the self-hosted CI
# runner (the ELF arrives as the cloud build artifact — this recipe never builds). Asserts the
# firmware reports EXPECT_STAMP, the expected devices are present, and nothing is FAULTY.
# Locally: `EXPECT_STAMP=$(git rev-parse HEAD) just diag hil` after a `just diag build`.
hil:
    #!/usr/bin/env bash
    set -euo pipefail
    elf="diag/target/msp430-none-elf/release/diag"
    [ -f "$elf" ] || { echo "no ELF at $elf — build or download the artifact first" >&2; exit 1; }
    echo "flashing $elf"
    DYLD_FALLBACK_LIBRARY_PATH={{libdir}} {{mspdebug}} tilib "prog $elf" exit
    port="$(ls -1 /dev/cu.usbmodem* 2>/dev/null | sort | tail -1)"
    [ -n "$port" ] || { echo "no /dev/cu.usbmodem* — run 'just usb status'" >&2; exit 1; }
    stty -f "$port" 9600 cs8 -cstopb -parenb raw -echo
    python3 diag/tests/hil_assert.py --port "$port" --stamp "${EXPECT_STAMP:-}"
