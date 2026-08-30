# diag — design & roadmap

The **central plan** for the diagnostic firmware: what a booted `diag` does, the cycle it
repeats, and how new device tests slot in. Usage/build lives in [README.md](README.md);
this file guides *direction*.

## What it is

A standalone **diagnostic ROM** for the MSP430 (spirit of the Amiga diag ROM): flash it to
confirm the connected hardware is present and healthy, read the verdict on the **backchannel
UART** and a **visual display**, then flash the real product firmware. It is deliberately
*not* the product firmware — it takes the I²C **master** role to probe devices, whereas the
product supervisor is largely an I²C **slave** (see [../FIRMWARE-API.md](../FIRMWARE-API.md)).

## Objectives

Diag exists to answer "is this board wired and populated the way it should be, and is it
healthy?" — for a specific product/board config (Bob, Ziloo, or a module). Four jobs:

1. **Scan the buses** — I²C by address, SPI by chip-select + ID — to enumerate what's actually
   present, and flag anything **new or changed** vs. what's expected.
2. **Validate against the expected config.** Each product/board has a known device set (declared
   alongside the PAC / board config + [connections.toml](../board/connections.toml)). Diag checks
   the *right* devices are present at the *right* addresses — not just "something answered."
   Missing or unexpected devices = **config drift**, reported explicitly.
3. **Check the MSP430 itself** — clock/FLL locked, rails, key peripheral registers sane (the live
   SFR register dump). The MCU is the first thing that must be healthy.
4. **Check each identified device's state** — beyond presence: read its ID/WHO_AM_I and a
   health/status register, so a device that's *present-but-wrong* or *present-but-faulty* is caught.

This makes diag a **config + health checker** (bench bring-up *and* CI/production board
verification), not only a power-on light show.

## Boot → cycle model

Every reset runs the same shape. **Init once, then loop a self-contained POST forever** so a
technician can plug/unplug on the bench and watch it re-test live.

```
reset
 └─ init (once)                         [main.rs]
     ├─ stop watchdog
     ├─ clock: DCO/FLL -> 1 MHz (SCG0 retune, wait for FLL lock)
     ├─ pin mux: P1.2/1.3 -> UCB0 I2C, P1.4/1.5 -> UCA0 UART; release LOCKLPM5
     ├─ enable device rails (e.g. IS31 SDB high on P2.5)
     ├─ uart::init   (9600 8N1 backchannel)
     └─ i2c::init    (100 kHz master; runs bus-recovery first)
 └─ loop  { POST cycle ; delay ~3 s }   [diag::run]
```

### The POST cycle (one pass)

```
1. banner / heartbeat
2. MCU SELF-CHECK — clock/FLL locked, rails, key SFRs (register dump). MCU healthy first.  [obj 3]
3. BUS HEALTH     — idle SDA/SCL (P1IN); if SDA stuck low -> recover, report recovered/short.
4. SCAN           — I2C by address (0x08..0x77); SPI/known devices by CS + ID. List present.[obj 1]
5. CONFIG CHECK   — compare found vs the EXPECTED device set for the active Bob/Ziloo config; [obj 2]
                    report missing / unexpected (drift).
6. DEVICE STATE   — per expected device: ID/WHO_AM_I + status/health read (catches present-  [obj 4]
                    but-wrong / present-but-faulty), report PASS/FAIL.
7. SUMMARY        — "N/M passed" + any config drift, over UART.
8. VISUAL         — pass/fail verdict on the display (OLED / LED matrix) if present.
9. delay, repeat.
```

Steps 2–4 exist today; **steps 5–6 (config-drift + device-state) are the direction being built**
toward the objectives. They are separate on purpose: the **scan** surfaces *anything* wired (even
unexpected); the **config check** compares that against the product's expected set (drift
detection); **device state** proves we can actually talk to each expected chip (ID + status), not
just that an address ACKs. The **expected device set** is a per-product/board manifest (Bob vs
Ziloo vs module), declared alongside the PAC / board config + `connections.toml`.

## Adding a test (the extension contract)

A device test is a `fn(&Peripherals) -> bool` plus one line in the registry. Keep the shape
**probe → identify → exercise**, and always report a clear PASS/FAIL:

```rust
// src/<chip>.rs — the register-level driver (mirrors is31.rs)
fn test_apds9960(p: &Peripherals) -> bool {
    if !i2c::probe(p, 0x39) { return false; }          // presence
    let mut id = [0u8; 1];
    if !i2c::read_reg(p, 0x39, 0x92, &mut id) { return false; } // identify (WHO_AM_I)
    id[0] == 0xAB                                        // exercise/verify
}

// src/diag.rs — register it
static TESTS: &[(&str, fn(&Peripherals) -> bool)] = &[
    ("LED matrix (IS31FL3730)", is31::test),
    ("APDS-9960 gesture",       apds9960::test),   // <-- one line to add a device
];
```

Rules for a test:
- **Never hang.** Use the bounded I²C helpers (`SPIN`-limited); a dead device must FAIL, not wedge.
- **Prove a read, not just an ACK** where the chip has an ID register.
- Put chip addresses/pins in [../board/connections.toml](../board/connections.toml) (the single
  source of truth) — don't scatter magic numbers.
- Keep drivers in their own `src/<chip>.rs`; `diag.rs` only orchestrates.

## Output & feedback channels

- **Backchannel UART (9600 8N1)** — the detailed log (bus levels, scan, per-test PASS/FAIL,
  summary, optional register dumps). Primary channel; always available.
- **Visual display** — the pass/fail verdict at a glance for a technician with no terminal.
  Currently the IS31 LED matrix (check/cross glyph); the SSD1306 OLED is slated to become the
  status display. If the display itself is the failing device, UART is the fallback.

## Built-in diagnostics (hard-won, keep them)

- **Bus idle levels** (`P1IN`): distinguishes "no pull-ups / not powered" (`L`) from a healthy
  idle bus (`H`) — turns a silent scan into a directed answer.
- **Stuck-bus recovery**: if SDA is held low, bit-bang up to 9 SCL pulses + STOP to free a
  wedged slave; a failure to release => hardware short, reported as such.
- **Register dump**: live SFRs (clock, ports, eUSCI) over UART for deep debugging.
- **Bounded spins everywhere**: no I²C path can hang the POST loop.
- **Clock backoff as escalation** (planned): when a full scan yields *nothing* (all NACK) or looks
  flaky, retry the scan at a lower I²C clock (100 kHz → ~10 kHz via `UCB0BRW`). Weak pull-ups, long
  bench wiring, or high bus capacitance make edges rise too slowly for 100 kHz; a device that only
  answers at 10 kHz is itself a *finding* (signal-integrity problem), so **report which clock
  succeeded** rather than silently retrying. Scope it carefully: backoff addresses signal integrity,
  **not** the false-ACK cascade — a stuck-low SDA reads as an ACK at any clock, so that's the
  recovery path's job, not the backoff's. Don't conflate the two.

## Roadmap — tests to slot in (rough priority)

1. **OLED SSD1306 @0x3C** — becomes the status display (init + "hello" + verdict glyphs).
2. **Sensors on the bench** — APDS-9960 (0x39, WHO_AM_I 0xAB), VL53L0X/L1X (0x29),
   ICM-42605 IMU (0x68/69, WHO_AM_I 0x42).
3. **Voltage rails** — VSOM/CHARGE via ADC (P1.6/P1.7) once wired; report levels + thresholds.
4. **GPIO / presence** — RCWL-0516 motion (digital out), sensor INT lines.
5. **Chip portability** — move off direct PAC pokes onto the feature-gated **board crate**
   (`fr2433`/`fr2476`) so one source builds per-chip; role/config from FRAM.

## Design invariants

- **Init once, POST forever** — every pass is self-contained and idempotent.
- **Never hang** — bounded waits; a fault reports, it doesn't freeze.
- **Self-diagnosing** — the firmware tells you *why* (bus level, recovered vs short), not just
  pass/fail.
- **Full-address scan, always** — the scan sweeps the entire 7-bit range (`0x08..0x77`) on every
  pass, never a narrowed "known-addresses-only" subset. Diag's whole job is to catch a device that
  *isn't where it should be* — wrong strap, address conflict, an unexpected part. A narrowed scan
  can't report drift it never looked for, so narrowing it defeats objective 1. If a probe is
  unreliable, fix the probe (below), never shrink the range to hide the symptom.
- **Reliable, independent probes** — a probe must return the same answer every call, regardless of
  what the previous probe did. If probe N can be corrupted by probe N-1's residue (e.g. a slave left
  holding SDA low, then misread as a false ACK), that's a probe bug to fix *at the source* — a
  deterministic completion event (wait for `UCTXIFG0 | UCNACKIFG`, i.e. the ACK bit actually
  sampled — not merely `UCTXSTT` clearing, which is *before* the ACK is sampled), not a reason to
  skip addresses or reach for the recovery path on every call.
- **Right-sized for the diag target (FR2476), not FR2433** — diag runs on the FR2476 dev board
  (64 KB FRAM), so it can afford the `embedded-graphics` + `ssd1306` stack: ~16 KB, taking the
  full ROM to ~21 KB text. Watch this — that stack does **not** fit FR2433's 15.5 KB FRAM. If diag
  is ever needed on FR2433, drop `embedded-graphics` for `ssd1306` raw-command/terminal mode (a few
  hundred bytes); the embedded-hal shim ([src/hal.rs](src/hal.rs)) stays either way. The graphics
  weight is diag's to spend — product firmware keeps its own tight budget.
- **Single source of truth for pins** — [../board/connections.toml](../board/connections.toml).
