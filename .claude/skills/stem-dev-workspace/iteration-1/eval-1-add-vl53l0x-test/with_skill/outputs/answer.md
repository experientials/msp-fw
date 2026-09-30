# Adding a VL53L0X ToF diagnostic test to `diag`

Scope: where the code goes and the conventions to follow — not a full driver. The `diag`
firmware is the I²C **master** bring-up tool; each device test is `probe → identify → exercise`,
reports a clear PASS/FAIL, and must **never hang**.

## Files to touch (4)

| File | Change |
|---|---|
| `diag/src/vl53l0x.rs` | **New** register-level driver, mirroring `diag/src/is31.rs`. |
| `diag/src/main.rs` | Add `mod vl53l0x;` alongside the other `mod` lines (currently lines 18–23). |
| `diag/src/diag.rs` | Add one test block in `run()` and bump the `total` counter. |
| `board/connections.toml` | Add a `planned` connection entry for the sensor on the shared I²C bus. |

## ⚠️ Gap to resolve first: there is no `i2c::read_reg`

`diag/DESIGN.md` shows an example test calling `i2c::read_reg(...)`, but **that helper does not
exist yet**. `diag/src/i2c.rs` currently exports only `probe`, `write`, `recover`, `init` — there
is no I²C **read** path (no RX-mode / repeated-START code). This matters because a proper VL53L0X
identify step reads a register (model ID `0xC0` → `0xEE`), which a write-only API can't do.

Two options — pick one and say which:

- **Minimal (probe-only)** — presence via `i2c::probe(p, 0x29)` plus a write-poke that ACKs. No
  read; you can't verify the model ID. Fine for "is it on the bus", weaker than the design's
  "prove a read, not just an ACK" rule.
- **Proper (recommended)** — add a bounded `i2c::read_reg(p, addr, reg, &mut buf) -> bool` to
  `i2c.rs` (write the register pointer, repeated START, RX mode `UCTR=0`, clock `buf.len()` bytes
  with NACK+STOP on the last), reusing the existing `SPIN` bound so it can't wedge the loop. Then
  the VL53L0X identify becomes a real read of `0xC0`. This helper is also needed by every future
  WHO_AM_I sensor on the roadmap (APDS-9960, ICM-42605), so it's the right place to invest.

## `diag/src/vl53l0x.rs` — skeleton (proper version)

```rust
//! STMicro VL53L0X time-of-flight ranging sensor (I²C).
//! 7-bit address 0x29 (fixed at power-on; re-addressable via I2C_SLAVE_DEVICE_ADDRESS 0x8A).
//! Identify via the model/revision ID block:
//!   REG_IDENTIFICATION_MODEL_ID    0xC0 -> 0xEE
//!   REG_IDENTIFICATION_REVISION_ID 0xC2 -> silicon revision (e.g. 0x10)
//! Datasheet/register names per ST UM2039 / the api "vl53l0x_device.h".

use crate::i2c;
use msp430fr2476::Peripherals;

const ADDR: u8 = 0x29;
const REG_MODEL_ID: u8 = 0xC0;
const MODEL_ID: u8 = 0xEE;

pub fn present(p: &Peripherals) -> bool {
    i2c::probe(p, ADDR)
}

/// probe -> identify (read model ID) -> verify. Returns PASS/FAIL for the registry.
pub fn test(p: &Peripherals) -> bool {
    if !present(p) {
        return false;                       // presence
    }
    let mut id = [0u8; 1];
    if !i2c::read_reg(p, ADDR, REG_MODEL_ID, &mut id) {
        return false;                       // identify (needs the new read helper)
    }
    id[0] == MODEL_ID                       // verify
}
```

Keep addresses/registers as `const`s next to the driver (as `is31.rs` does) — don't scatter
magic numbers. Bus init/mux and the SDA/SCL pins are already owned by `main.rs` + `i2c.rs`; the
driver only speaks through the `i2c` helpers, never the PAC directly.

## `diag/src/diag.rs` — register it

`diag.rs` does **not** use the `static TESTS` array shown in DESIGN.md; the live code hand-writes
each test as an inline block in `run()` with `total` / `passed` counters (see the LED block at
lines 73–92). Follow the existing pattern:

```rust
let total = 2u16;                 // was 1 — bump for the new test
// ...existing LED block...

uart::puts(p, "  VL53L0X ToF (@0x29)");
if vl53l0x::test(p) {
    uart::puts(p, "  PASS\n");
    passed += 1;
} else {
    uart::puts(p, "  FAIL\n");
}
```

Add `use crate::... vl53l0x` to the `use` line at the top of `diag.rs`. UART log helpers available:
`puts`, `putc`, `hex8`, `hex16`, `dec` (from `uart.rs`).

## `board/connections.toml` — the pin registry

The sensor rides the already-`active` shared bus (`i2c_sda` P1.2 / `i2c_scl` P1.3), so no new mux
pins are strictly required. Still add a connection entry so the device address is documented in the
single source of truth. Use `status = "planned"` until verified on the bench (only promote to
`active` after checking against the datasheet). If you wire the XSHUT (shutdown) or GPIO1
(interrupt) lines to spare GPIO, add those as separate `planned` entries too. Follow the existing
block shape (id / signal / pin / function / module / sel / dir / status / net / notes).

## Conventions checklist (from the skill + DESIGN.md)

- **Never hang** — go only through the bounded (`SPIN`-limited) `i2c` helpers; a dead sensor must
  FAIL, not wedge the POST loop. If you add `read_reg`, keep every wait `SPIN`-bounded.
- **Prove a read, not just an ACK** — verify model ID `0xC0 == 0xEE`, don't stop at `probe`.
- **Typed PAC access only** — already encapsulated inside `i2c.rs`; the driver shouldn't touch
  registers or raw pointers.
- **One driver per chip** — `src/vl53l0x.rs`; `diag.rs` only orchestrates.
- **Pins/addresses in one place** — device address as a `const` in the driver, wiring in
  `connections.toml`.
- **Build/flash**: `just diag build` (in-container), `just diag run` (flash on host),
  `just monitor` to watch the 9600 8N1 backchannel for the PASS/FAIL line.

## Docs to read

- `diag/DESIGN.md` — boot→cycle model + the "Adding a test" extension contract (note its
  `read_reg`/`static TESTS` examples are aspirational vs. the current code).
- `diag/src/is31.rs` — the reference driver to mirror.
- `diag/src/i2c.rs` — the helper surface you build on (and where `read_reg` would go).
- `board/connections.toml` — pin/address registry conventions.
- VL53L0X register names: ST UM2039 / datasheet in `datasheets/` if present; the roadmap in
  `diag/DESIGN.md` already lists it as "VL53L0X/L1X (0x29)".
