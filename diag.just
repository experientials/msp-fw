# `just diag <verb>` — build/flash the diagnostic ROM (top-level diag/, Rust on the FR2476 PAC).
#
#   just diag build     build the ELF — native if the toolchain is set up, else Docker (see `build`)
#   just diag dev       fast native build + flash — the sub-minute inner loop
#   just diag flash     program the connected board (host; needs msp430-macos-dev setup)
#   just diag run       build + flash + VERIFY the running firmware reports the stamp we flashed
#
# `run` closes the "did my flash actually take?" gap: it stamps the build (DIAG_BUILD), flashes,
# then reads the backchannel UART and refuses to succeed until it sees that exact stamp come back.
# For a plain look at the POST, use the top-level `just monitor`.

image    := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")
mspdebug := env_var_or_default("MSPDEBUG_BIN", "/usr/local/bin/mspdebug")
libdir   := env_var_or_default("MSP430_LIBDIR", "/usr/local/lib")

# Persistent cargo caches (named docker volumes) so the registry index + downloaded crates survive
# `--rm` container runs — otherwise every build re-downloads crates.io (image CARGO_HOME=/opt/cargo).
# Mounts only the data subdirs, so the image's cargo/rust binaries in /opt/cargo aren't shadowed.
cargo_cache := "-v msp430-cargo-registry:/opt/cargo/registry -v msp430-cargo-git:/opt/cargo/git"

# Build diag (Rust). `just diag build` picks the fastest available path:
#   • host WITH the native toolchain (scripts/setup-native-macos.sh) → native, no emulation
#   • host WITHOUT it                                                → Docker image (amd64)
#   • inside the toolchain container (CI)                            → builds natively in-container
# `mode` overrides the choice:
#   auto (default) | native (LTO) | fast (native, no-LTO) | docker (LTO) | docker-fast (no-LTO)
# NOTE: this is only the LOCAL default for convenience — shipped artifacts are always built on CI
# (see .github/workflows/hil.yml). `-e DIAG_BUILD` forwards the caller's stamp into the container;
# on the native path build.rs picks it up from the inherited env (unset -> derives from git+time).
build mode="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    # Inside the toolchain container (CI, or our own docker path): build here; profile from mode.
    if [ -f /.dockerenv ]; then
        case "{{mode}}" in *fast) prof=fast; flag="--profile fast";; *) prof=release; flag="--release";; esac
        ( cd diag && cargo build $flag && msp430-elf-size "target/msp430-none-elf/$prof/diag" )
        exit 0
    fi
    run_docker() { docker run --rm --platform linux/amd64 -e DIAG_BUILD {{cargo_cache}} -v "$PWD":/work -w /work {{image}} just diag build "$1"; }
    mode="{{mode}}"
    if [ "$mode" = auto ]; then
        if scripts/native-available.sh; then mode=native
        else mode=docker; echo "· native toolchain not found → Docker (set it up: scripts/setup-native-macos.sh)"; fi
    fi
    case "$mode" in
        native)      scripts/native-build.sh release ;;
        fast)        scripts/native-build.sh fast ;;
        docker)      run_docker docker ;;
        docker-fast) run_docker docker-fast ;;
        *) echo "unknown mode '$mode' — use auto|native|fast|docker|docker-fast" >&2; exit 1 ;;
    esac

# Inner dev loop: fast native build (no-LTO) + flash. Sub-minute edit→running once native is set up.
dev:
    just diag build fast
    just diag flash fast

# Verify the native toolchain is correctly segregated (pinned nightly reachable, NOT system default).
doctor:
    scripts/doctor.sh

# Note: stress is no longer a separate build — it's a runtime task selected from the OLED menu
# (S1 opens the menu, S2 cycles to "I2C MARGIN"/"I2C SOAK", S1 selects). One unified `just diag run`.

# flash diag's built ELF to the connected board (host; needs msp430-macos-dev setup).
# `profile` selects which build to flash: "release" (default) or "fast" (`just diag build fast`).
flash profile="release":
    #!/usr/bin/env bash
    set -euo pipefail
    elf="diag/target/msp430-none-elf/{{profile}}/diag"
    [ -f "$elf" ] || { echo "no built ELF at $elf — run 'just diag build' (or 'just diag build fast') first" >&2; exit 1; }
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
