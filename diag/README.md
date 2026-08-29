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

## Hardware

Per [../../bob-929/docs/IS31FL3730_EB.md](../../bob-929/docs/IS31FL3730_EB.md): SDA→P1.2,
SCL→P1.3, shared GND, **JP1 open**, EB self-powered. SDB enable driven on P2.0
(see [../board/connections.toml](../board/connections.toml)). Backchannel UART on
P1.4/P1.5 → `/dev/cu.usbmodem*` at **9600 8N1**.

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
```

## Layout

| File | Role |
|---|---|
| `src/main.rs`  | entry, clock (1 MHz FLL), pin routing, POST loop |
| `src/uart.rs`  | eUSCI_A0 backchannel, 9600 8N1 |
| `src/i2c.rs`   | eUSCI_B0 I²C master, polled |
| `src/is31.rs`  | IS31FL3730 driver |
| `src/diag.rs`  | bus scan, test list, POST runner, status glyphs |
| `src/util.rs`  | crude `delay_ms` |

## Extending

Add a device test in [src/diag.rs](src/diag.rs) and a line in `run` (mirrors the C version's
test registry). Put any register-level chip driver in its own `src/<chip>.rs`. When the
feature-gated **board crate** lands, `diag` migrates to typed pins + the connection registry
instead of poking pins directly here.
