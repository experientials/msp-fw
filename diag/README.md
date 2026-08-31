# bob-929 diagnostic firmware (`diag/`)

A standalone **diagnostic ROM** for the MSP430FR2476, written in **Rust** on the vendored
[`msp430fr2476` PAC](../pac/msp430fr2476/) — flash it to confirm connected hardware is
present and healthy, read the verdict on the **LED matrix** + **backchannel UART**, then
flash the real firmware.

Register access is through the typed PAC (`p.e_usci_b0.ucb0ctlw0()...`) using proven bit
values from the FR2476 header — not raw pointers. This replaces the earlier C prototype.

## Current coverage

- **IS31FL3730 LED matrix** (QFLS2-EB, `0x61`) — presence, init, all-on + checker self-test,
  and it doubles as the pass/fail status display (check ✓ / cross ✗ glyph).
- Full **I²C bus scan** (`0x08`–`0x77`) each pass, so newly-wired devices show up before
  they have a dedicated test.
- **RCWL-0516 microwave radar** presence on **P2.4** (digital OUT) — sampled continuously by a
  cooperative task (fast while asserted, idling slower) and reported as a **motion window**
  (`motion/idle` + trigger count) each POST pass, so a trigger between passes isn't missed.

## Scheduling

diag runs on a tiny **cooperative scheduler** — the [`sched`](../crates/sched/) crate (PAC-agnostic, so
the same engine serves product firmware and examples on FR2433/FR2476). `main` builds a context +
a table of tasks and loops `sched::tick`; each task runs a short, non-blocking step and says when
it wants to run next. Today: `RadarTask` (~variable rate) samples P2.4; `PostTask` (~3 s) runs the
I²C POST and reports the radar window. The rule is **never block in a task** — the watchdog is the
backstop for a task that does. Time base is a polled TB0 millisecond clock ([src/clock.rs](src/clock.rs)),
no ISR.

## Hardware

Per [../../bob-929/docs/IS31FL3730_EB.md](../../bob-929/docs/IS31FL3730_EB.md): SDA→P1.2,
SCL→P1.3, shared GND, **JP1 open**, EB self-powered. SDB enable driven on P2.0
(see [../crates/bsp/connections.toml](../crates/bsp/connections.toml)). Backchannel UART on
P1.4/P1.5 → `/dev/cu.usbmodem*` at **9600 8N1**. RCWL-0516 radar OUT on **P2.4**
(VIN=5 V, GND shared, 3V3/CDS left open) — GPIO input + pulldown.

## Build & flash

Uses the Docker toolchain (build) + the `msp430-macos-dev` skill (flash) — see
[../TOOLCHAIN.md](../TOOLCHAIN.md).

```sh
# build in the container (same as CI; .cargo/config sets target + build-std)
docker run --rm --platform linux/amd64 -v "$PWD/..":/work -w /work/diag \
  msp430-c-rust:local cargo build --release

# flash the resulting ELF (host, via the skill's mspdebug wrapper)
DYLD_FALLBACK_LIBRARY_PATH=/usr/local/lib mspdebug tilib \
  "prog target/msp430-none-elf/release/diag" run exit

# watch output
just monitor            # or: screen /dev/cu.usbmodem* 9600
```

Expected per pass:

```
=== bob-929 diag POST ===
I2C scan: 0x61
  LED matrix (IS31FL3730 @0x61)  PASS
summary: 1/1 passed
  radar window (P2.4): idle, 0 trigger(s)
```

## Layout

| File | Role |
|---|---|
| `src/main.rs`  | entry, clock (1 MHz FLL), pin routing, scheduler setup + loop |
| `src/clock.rs` | polled TB0 millisecond time base (no ISR) |
| `src/tasks.rs` | the diag context (`Cx`) + cooperative tasks (`RadarTask`, `PostTask`) |
| `src/uart.rs`  | eUSCI_A0 backchannel, 9600 8N1 |
| `src/i2c.rs`   | eUSCI_B0 I²C master, polled |
| `src/is31.rs`  | IS31FL3730 driver |
| `src/rcwl.rs`  | RCWL-0516 radar OUT read (P2.4) |
| `src/diag.rs`  | bus scan, test list, POST runner, status glyphs |
| `../crates/sched/` | the shared cooperative scheduler crate (`Task`, `Slot`, `tick`) |

## Extending

Two extension points, for the two kinds of thing:

- **An I²C device check** (present/healthy at boot): add a `Dev` entry to the `DEVICES` registry
  in [src/diag.rs](src/diag.rs); put any register-level driver in its own `src/<chip>.rs`. This
  runs inside `PostTask`'s POST pass.
- **A time-based concern** (sampling at some rate, animation output, talking to another MCU): add a
  `sched::Task` in [src/tasks.rs](src/tasks.rs) and one `Slot::every(...)` line in `main`. Keep
  each `poll` non-blocking — split any "wait" into task state, never a spin.

When the feature-gated **board crate** lands, `diag` migrates to typed pins + the connection
registry instead of poking pins directly here.
