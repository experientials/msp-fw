# Build, flash & monitor the diag firmware on the FR2476 LaunchPad (macOS)

The repo drives everything through `just`. Builds run **inside a Docker container** (so they
match CI), while flashing and serial run **natively on your Mac** (Docker on macOS has no USB
passthrough). The whole diag flow is in `diag.just` and the top-level `justfile`.

## TL;DR (once setup is done)

```bash
cd /Volumes/Projects/Talki/msp-fw
just diag run     # = build (in container) + flash (on host)
just monitor      # watch the POST output over the backchannel UART, Ctrl-C to stop
```

`just diag run` is just `build` then `flash` (see `diag.just:35`). You can also run the two
steps separately: `just diag build` then `just diag flash`.

## One-time setup (do these first if you haven't)

1. **Build the toolchain Docker image** (needed for `build`):
   ```bash
   just bootstrap        # docker build --platform linux/amd64 -t msp430-c-rust:local docker/
   ```
   This is the default `MSP430_IMAGE`. On Apple Silicon it runs under Rosetta/QEMU — fine for
   building, just not native speed. (Docker Desktop must be running.)

2. **Set up mspdebug for flashing** via the `msp430-macos-dev` skill (needed for `flash`,
   `probe`, and USB recovery). The skill lives at:
   `/Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/SKILL.md`
   ```bash
   bash /Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/scripts/setup-macos.sh
   ```
   This installs an **x86_64 mspdebug** (run under Rosetta) plus TI's signed, de-quarantined
   `libmsp430.dylib` at `/usr/local/lib`. That's why the just recipes prefix flash commands with
   `DYLD_FALLBACK_LIBRARY_PATH=/usr/local/lib`. The x86_64 build + code-signing + DYLD path are
   three separate Apple-Silicon gotchas the skill resolves — don't hand-roll it.

3. **Verify the toolchain** (optional sanity check):
   ```bash
   just check deps            # verify build toolchain present
   just check deps -- --flash # also verify the flashing side
   ```

## Full step-by-step

```bash
cd /Volumes/Projects/Talki/msp-fw

# 0. Plug in the LP-MSP430FR2476 LaunchPad (onboard eZ-FET). Confirm the Mac sees it:
just usb status            # should say "TI eZ-FET PRESENT" and show a /dev/cu.usbmodem*
just probe                 # reads the FR2476 device id without flashing (good smoke test)
# First ever connect may ask to update the eZ-FET firmware — allowed via the skill's
# scripts/mspdebug-macos.sh --allow-fw-update tilib exit  (updates the debugger, not the target)

# 1. Build the diag ELF (runs cargo build --release in the container)
just diag build
#   -> diag/target/msp430-none-elf/release/diag

# 2. Flash it to the board
just diag flash            # programs then runs the new firmware (mspdebug tilib "prog … " exit)

#   (steps 1+2 in one go: just diag run)

# 3. Watch the serial output — backchannel UART on P1.4/P1.5, 9600 8N1
just monitor               # auto-picks the highest /dev/cu.usbmodem*; Ctrl-C to stop
#   If that port is silent, pass the other one explicitly:
#   just monitor /dev/cu.usbmodem23601
```

### Expected serial output (per POST pass)

```
=== bob-929 diag POST ===
I2C scan: 0x61
  LED matrix (IS31FL3730 @0x61)  PASS
summary: 1/1 passed
```

The diag ROM also shows a pass/fail glyph (check ✓ / cross ✗) on the IS31FL3730 LED matrix, so
you get a verdict even without the serial console.

## If the board misbehaves

- `just usb status` — is the eZ-FET on the bus? which `/dev` node? any stale mspdebug holder?
- `just usb recover` — kills stale FET/port holders and waits for re-enumeration; prints
  physical steps (unplug OLED/shorts, power-cycle the hub ~20 s, prefer a powered hub + data
  cable) if the board has dropped off the bus. macOS can't re-enumerate a physically-absent
  device from software.
- `just usb wait` — block until `/dev/cu.usbmodem*` reappears.
- No `/dev/cu.usbmodem*` isn't needed for flashing — only for the backchannel-UART output.

## Key references in the repo

- `diag.just` — `build` / `flash` / `run` recipes for diag
- `justfile` — `bootstrap`, `probe`, `monitor`, `check`, module wiring
- `usb.just` — `usb status` / `recover` / `wait`
- `diag/README.md` — what diag tests, wiring (SDA→P1.2, SCL→P1.3, UART P1.4/P1.5 @9600 8N1)
- `TOOLCHAIN.md` — container-build-vs-host-flash split, image details
- `msp430-macos-dev` skill (path above) — the mspdebug/Apple-Silicon flashing setup

## Overrides (env vars), if your paths differ

`MSP430_IMAGE` (default `msp430-c-rust:local`), `MSPDEBUG_BIN` (default `/usr/local/bin/mspdebug`),
`MSP430_LIBDIR` (default `/usr/local/lib`).
