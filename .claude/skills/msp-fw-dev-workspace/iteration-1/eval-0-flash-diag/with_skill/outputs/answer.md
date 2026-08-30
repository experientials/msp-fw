# Build, flash & monitor the `diag` firmware on the FR2476 LaunchPad (macOS)

The model: **builds run in the Docker toolchain container**, **flashing runs natively on the
host** over USB (Docker on macOS has no USB passthrough). The `just diag` recipes do both for you.
Run everything from the repo root: `/Volumes/Projects/Talki/msp-fw`.

## TL;DR

```sh
cd /Volumes/Projects/Talki/msp-fw
just diag run     # builds diag/ in the container, then flashes the connected board
just monitor      # watch the 9600 8N1 POST output; Ctrl-C to stop
```

`just diag run` = `just diag build` (container) + `just diag flash` (host). If it works, you're done —
the rest below is one-time setup and troubleshooting.

## One-time setup (do these once, in order)

1. **Toolchain image** (for building). Build it once:
   ```sh
   just bootstrap          # docker build -t msp430-c-rust:local docker/
   ```
   Requires Docker running. The image is amd64, so on Apple Silicon it builds under Rosetta/QEMU —
   correct, just not fast.

2. **Host flash tools** (for flashing). Flashing needs an **x86_64 `mspdebug` + a signed, de-quarantined
   `libmsp430.dylib`**. This is set up by the **`msp430-macos-dev` skill** in the sibling repo:
   `/Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/`
   ```sh
   bash /Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/scripts/setup-macos.sh
   ```
   Why it's fiddly on Apple Silicon (three stacked gotchas): TI ships `libmsp430.dylib` x86_64-only,
   macOS quarantines/unsigns it, and dyld needs `DYLD_FALLBACK_LIBRARY_PATH` pointed at it (and you must
   NOT wrap the command in `/usr/bin/arch`, which strips `DYLD_*`). The `just diag flash` recipe already
   sets `DYLD_FALLBACK_LIBRARY_PATH=/usr/local/lib` and calls `/usr/local/bin/mspdebug tilib ...` for you.

3. **Verify** before flashing anything:
   ```sh
   just check deps --flash   # checks host flash tooling too
   just probe                # reads the FR2476 device id over the FET (no flash) — confirms the link
   ```

4. **First connect only:** if `just probe` says *"FET firmware update is required"*, update the
   onboard eZ-FET debugger once (this flashes the debugger, NOT your target — don't unplug mid-update):
   ```sh
   /Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/scripts/mspdebug-macos.sh --allow-fw-update tilib exit
   ```

## Watching the serial output

`diag` prints its POST over the eZ-FET **backchannel UART at 9600 8N1** (firmware UART on P1.4/P1.5).

```sh
just monitor                          # auto-picks the highest-numbered /dev/cu.usbmodem*
just monitor /dev/cu.usbmodem23601    # ...or name the port if the auto-pick is the silent one
```

`just monitor` is a one-way `cat` (no `screen`/`picocom` holding the port), so it won't fight the
flasher for the device. Expected per pass:

```
=== bob-929 diag POST ===
I2C scan: 0x61
  LED matrix (IS31FL3730 @0x61)  PASS
summary: 1/1 passed
```

Note: on some macOS setups the CDC backchannel interface doesn't enumerate — that only affects serial
printf, not flashing. If no `/dev/cu.usbmodem*` shows up, run `just usb status` and check the board's
UART jumpers / FET firmware.

## If the USB link drops ("No unused FET found", device disappears)

```sh
just usb status     # diagnose the eZ-FET USB (hub/short/latch)
just usb recover
```

Deeper flash/mspdebug/USB troubleshooting lives in the **msp430-macos-dev** skill
(`references/flashing.md`, and the "Known gotchas" table in its `SKILL.md`).

## Key references
- `/Volumes/Projects/Talki/msp-fw/TOOLCHAIN.md` — build/flash/CI model
- `/Volumes/Projects/Talki/msp-fw/diag/README.md` and `diag/DESIGN.md` — what `diag` does, expected output
- `/Volumes/Projects/Talki/msp-fw/diag.just` and `/Volumes/Projects/Talki/msp-fw/justfile` — the exact recipes
- `/Volumes/Projects/Talki/bob-929/.claude/skills/msp430-macos-dev/` — host flashing/toolchain setup + gotchas
