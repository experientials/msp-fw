# Adding a VL53L0X diagnostic test to the `diag` firmware

Guidance only — where the code goes and the conventions to follow. All paths are in
`/Volumes/Projects/Talki/msp-fw/`.

## The big picture

`diag/` is the standalone diagnostic ROM (Rust, `no_std`, MSP430FR2476, vendored
`msp430fr2476` PAC). Every reset does `init once → loop { POST cycle; delay 3 s }`. A POST
cycle is: banner → bus health → I²C scan (`0x08..0x77`) → **per-device tests** → summary →
visual verdict on the LED matrix. Your VL53L0X test is a new per-device test.

The authoritative design/extension contract is `diag/DESIGN.md` ("Adding a test"). Read it —
but note it is partly aspirational (see the gap below).

## Where the code goes

1. **New driver file: `diag/src/vl53l0x.rs`** — the register-level driver, one file per chip,
   mirroring `diag/src/is31.rs`. This holds the address const, register consts, and a
   `pub fn test(p: &Peripherals) -> bool` (plus any presence/init helpers).

2. **Declare the module in `diag/src/main.rs`** — add `mod vl53l0x;` alongside the existing
   `mod i2c; mod is31; ...` block (lines 18-23).

3. **Register + invoke the test in `diag/src/diag.rs`** — this is the POST runner. Add a call
   in `run()` and bump the `total` test count (see "the diag.rs gap" below for the exact shape
   the current code uses).

4. **Record the pins/address in `board/connections.toml`** — the single source of truth for
   wiring. The VL53L0X shares the existing sensor bus (`i2c_sda` P1.2 / `i2c_scl` P1.3, already
   `active`). Add a `[[connection]]` entry for the sensor (and its XSHUT pin if wired) with
   `status = "planned"` until verified on the bench. Do not scatter pin/address magic numbers
   in the source — they live here.

## The `diag.rs` gap (important — DESIGN.md vs. reality)

`DESIGN.md` shows a clean `static TESTS: &[(&str, fn(&Peripherals) -> bool)]` registry and an
`i2c::read_reg(...)` helper. **Neither exists in the code yet.** What's actually there:

- `diag/src/diag.rs` hand-writes each test inline in `run()`. The LED test is:
  ```rust
  let total = 1u16;              // <-- bump to 2
  let mut passed = 0u16;
  uart::puts(p, "  LED matrix (IS31FL3730 @0x60-63)");
  if test_led(p) { uart::puts(p, "  PASS\n"); passed += 1; }
  else           { uart::puts(p, "  FAIL\n"); }
  ```
  Follow this exact pattern for the VL53L0X (add a `test_vl53l0x(p)` helper or call
  `vl53l0x::test(p)`), and increment `total` to `2`. Match the `"  <name>"` + `PASS/FAIL`
  formatting so the UART log stays consistent.

- `diag/src/i2c.rs` exposes only `probe(addr) -> bool`, `write(addr, &[u8]) -> bool`,
  and `recover()`. **There is no register-read function.** DESIGN.md's rule "prove a read, not
  just an ACK" requires one. So a *proper* VL53L0X test needs you to **add a read helper to
  `i2c.rs`** — e.g. `pub fn read_reg(p, addr, reg, buf: &mut [u8]) -> bool` doing
  START+addr(W)+reg, repeated-START+addr(R), read N bytes, STOP — using the same bounded-`SPIN`
  polling style as `write`/`probe` so it can never hang. This is the one non-trivial piece of
  plumbing; without it the test can only ACK-probe (weaker, but a valid minimal first cut).

## VL53L0X specifics (7-bit addr `0x29`)

- **Identify register:** `MODEL_ID` at reg `0xC0` reads `0xEE` (single-byte register index).
  `0xC1` = revision `0xAA`, `0xC2` = module ID. Use `0xC0 == 0xEE` as the identify/verify step.
- **XSHUT:** the sensor only responds when its XSHUT pin is high (default `0x29`). If XSHUT is
  wired to a GPIO, drive it high before probing (mirror how `main.rs` drives IS31 SDB high on
  P2.5, lines 80-83). If it's strapped high on the board, nothing extra needed.
- No datasheet for the VL53L0X is in `datasheets/` — the register facts above are the ones you
  need for a diagnostic (model-ID check). A full ranging init is explicitly *not* required for
  diag.

## Conventions to follow (non-negotiable)

- **Test shape: probe → identify → exercise.** For diag, `identify` (read `0xC0` == `0xEE`) is
  enough; you don't need real ranging.
- **Never hang.** Only use the bounded I²C helpers; a dead/absent sensor must return `false`
  (FAIL), never wedge the POST loop. Any new read helper must be `SPIN`-bounded like the
  existing ones.
- **Typed PAC access**, not raw pointers — go through `p.e_usci_b0....` etc. (i2c.rs already
  does; you inherit this by using its helpers).
- **Report clear PASS/FAIL over UART** using the existing `uart::puts/putc/hex8/dec` helpers.
- **Keep the driver in its own `src/vl53l0x.rs`;** `diag.rs` only orchestrates.
- Chip is `no_std` / tiny-budget — keep it minimal.

## Skeleton (illustrative, not a full impl)

```rust
// diag/src/vl53l0x.rs
use crate::{i2c};              // add uart if you log inside the driver
use msp430fr2476::Peripherals;

const ADDR: u8 = 0x29;
const REG_MODEL_ID: u8 = 0xC0; // reads 0xEE
const MODEL_ID: u8 = 0xEE;

pub fn present(p: &Peripherals) -> bool { i2c::probe(p, ADDR) }

pub fn test(p: &Peripherals) -> bool {
    if !present(p) { return false; }                     // probe
    let mut id = [0u8; 1];
    if !i2c::read_reg(p, ADDR, REG_MODEL_ID, &mut id) {  // identify — NEEDS the new i2c helper
        return false;
    }
    id[0] == MODEL_ID                                    // verify
}
```

## Build / verify

- Build in the Docker toolchain (same as CI): `just diag build` (or the `docker run ...` in
  `diag/README.md`). Flash with `just diag flash` / `just diag run`.
- Watch the POST over the backchannel UART: `just monitor` (9600 8N1). You should see the new
  `VL53L0X @0x29  PASS/FAIL` line and the `summary: N/2 passed`, plus `0x29` appearing in the
  I²C scan line once wired.

## Reference files

- `diag/DESIGN.md` — extension contract, POST model, invariants (VL53L0X is on its roadmap).
- `diag/src/is31.rs` — the driver pattern to mirror.
- `diag/src/diag.rs` — POST runner; where you wire the test into `run()`.
- `diag/src/i2c.rs` — bounded I²C helpers; where a `read_reg` needs to be added.
- `diag/src/main.rs` — module declarations + init (clock, pin mux, device-rail enable).
- `board/connections.toml` — pin/address registry (add the sensor here).
- `I2C-API.md`, `FIRMWARE-API.md`, `HARDWARE.md` (repo root) — bus/product context.
