# Examples

Minimal firmware that proves the toolchain end to end. Both blink **LED1 (P1.0)** on the
LP-MSP430FR2476.

| Example | Language / build | What it does |
|---|---|---|
| `hello-c` | C, `make` | blink LED1 (driverlib-style) |
| `hello-rust` | Rust, `cargo` (`msp430-none-elf`) | blink LED1 (raw `volatile` SFRs, no PAC) |
| `i2c-is31` | Rust, `cargo` | drive the IS31FL3730 8×8 LED matrix over I²C (eUSCI_B0, P1.2/P1.3, @0x61) — walks a lit row |

## I²C comms status (2026-08-30)

**Not yet confirmed working on hardware.** State of the IS31FL3730-over-I²C bring-up:

- ✅ **Firmware runs & UART console works.** After adding a proper DCO+FLL 1 MHz clock init
  (SCG0-off → set CS → wait-for-lock), the 9600-baud console is clean. `[scan]` prints found
  addresses, then `[is31] init` / `[row NN]` with `ACK` / `NACK` / `TIMEOUT` (i2c-is31 now has
  timeout-guarded polls so a stuck bus can't freeze it).
- ✅ **Pin config is correct.** P1.2→`UCB0SDA`, P1.3→`UCB0SCL` via `P1SEL0` (`P1SELx=01`) —
  confirmed against datasheet **Table 9-23**. UCB1 (the *other* I²C) is P3.2/P3.6 or P4.3/P4.4.
- ✅ **IS31 board is healthy.** Powered (5 V rail), pull-ups hold TP3 SDA/SCL at ~3.0 V when
  disconnected. Its own 4.7 kΩ pull-ups; do **not** add external ones.
- ❌ **Bus doesn't communicate yet.** Symptom seen connected: every address "ACKs" and the
  lines sit at 0 V while running = **SDA held low** → prime suspect is **wiring** (IS31 SDA/SCL
  landing on the wrong header pins, or not on P1.2/P1.3), since the MSP side has no internal
  pull-up and the firmware/pins check out. Verify SDA→**P1.2**, SCL→**P1.3**, GND shared.

**Debug tooling:** `just monitor` (UART console, Ctrl-C to stop), the in-firmware `[scan]`, and
SBW breakpoints via the skill's `mspdebug-macos.sh`.

**Direction:** the raw-register `i2c-is31` was a bring-up vehicle; the **PAC-based `diag/`**
(typed `msp430fr2476` registers) is the canonical firmware — migrate/retire `i2c-is31` once the
bus is proven. See [../diag/](../diag/) and [../NOTES.md](../NOTES.md).

## Commands

Per-example operations use the `example` subcommand group — `just example <verb> <name>`:

```bash
just example list             # list examples
just example build hello-c    # build (auto-detects C make vs Rust cargo)
just example flash hello-rust # flash the built ELF to the connected board
just example run   hello-c    # build + flash (the everyday loop)
```

- **build** runs in the toolchain container (Docker on your Mac, native in CI).
- **flash**/**run** flash natively over USB via `mspdebug` — Docker has no USB passthrough — and
  need the one-time [msp430-macos-dev](../../bob-929/.claude/skills/msp430-macos-dev/SKILL.md)
  setup (x86_64 mspdebug + signed `libmsp430.dylib`).
- Flashing is fire-and-forget: it programs, the MCU runs the new firmware, and your prompt returns.

Build everything at once with top-level `just build`. See [../TOOLCHAIN.md](../TOOLCHAIN.md) for the
image/CI details.

## Adding an example

Create `examples/<name>/` with either a `Makefile` (C) or a `Cargo.toml` (Rust) — the `example`
recipes auto-detect the build system, so `just example build <name>` just works, no justfile edits.
For C, emit the ELF under `build/`; for Rust, name the crate after the directory.
