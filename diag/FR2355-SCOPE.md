# diag → MSP430FR2355 / FR2155 port: concrete scope

Status: **scoping (verified against datasheet + PACs, not yet implemented).**
Motivation: an FR2355 LaunchPad is on the bench; `thepia hwd msp identify` reads it as
`MSP430FR2355 (id=0x01ff)` but there is no FR2355 firmware (diag targets FR2476, prod targets
FR2433), so it can't run/verify. FR2355's 4 eUSCI (dual-I2C) is the reason it matters vs FR2433.

## Head start (already in the tree)
- **PACs exist and are real:** `pac/msp430fr2355` (24968-line `lib.rs`, `device.x`) and
  `pac/msp430fr2155`. No PAC generation needed.
- **`crates/devices`** sensor drivers are generic over `embedded-hal::I2c` → portable unchanged.
- **`DIAG_TARGET`** already exists in `diag/build.rs` as a build-stamp label (`fr2476`/`fr2433`),
  but it does **not** yet select a PAC — the PAC is hardcoded in `diag/Cargo.toml`.

## Verified deltas (sources cited)

### 1. Memory map — `memory.x` (FR2355 datasheet Rev. D, Table 6-4, p65)
| Region | FR2476 (current `diag/memory.x`) | **FR2355 / FR2155** |
|---|---|---|
| Code FRAM (ROM) | `0x8000`, len `0x7F80` (32 KB) | **same**: `0x8000–0xFFFF` (32 KB) |
| RAM | `0x2000`, len `0x2000` (**8 KB**) | **`0x2000`, len `0x1000` (4 KB)** |
| Vectors | `0xFFE0`, len `0x0020` | **`0xFF80`, len `0x0080`** |
| Info/data FRAM | (unused) | `0x1800–0x19FF` (512 B) |

→ New `diag/memory-fr2355.x`: `RAM 0x2000/0x1000`, `ROM 0x8000/0x7F80`, `VECTORS 0xFF80/0x0080`.
The **4 KB RAM (half of FR2476)** is the one budget constraint to watch; diag SRAM use today is ~2 B
static (per the size gate), so ample headroom.

### 2. Peripheral set (PAC `Peripherals` structs)
All blocks diag uses are present in FR2355 with the **same names** as FR2476: `cs` (clock),
`frctl` (FRAM), `pmm`, `sys`, `p1–p6/pj` (GPIO), `e_usci_a0/a1` (UART), `e_usci_b0/b1` (I2C),
`adc`, `wdt_a`. FR2355 adds `e_comp1`, `captio`, `icc`, `sac0–3` (diag needs none).
**FR2155 = FR2355 minus `sac0–3`** (op-amps) → one image family.

### 3. Timer — **the one peripheral delta that touches code**
FR2476 has **Timer_A** (`ta0–ta3`); FR2355/FR2155 have **only Timer_B** (`tb0–tb3`, no Timer_A).
- diag's **ms base already uses TB0/ACLK** (`clock.rs`) → unaffected.
- diag's **µs base uses TA0** (`usec.rs`, **stress build only**) → must port TA0 → a spare Timer_B
  (FR2355 has `tb1/tb2/tb3` free). Small, isolated change.

### 4. UART console pins — **the reason the wrong-family flash showed no console**
diag console = **eUSCI_A0, 9600 8N1** (`uart.rs`), pins routed in `main.rs` via `P1SEL0`.
| Signal | FR2476 (datasheet + `main.rs`) | **FR2355 (datasheet)** |
|---|---|---|
| UCA0TXD | **P1.4** | **P1.7** |
| UCA0RXD | **P1.5** | **P1.6** |
| UCB0 I2C SDA/SCL | **P1.2 / P1.3** | **P1.2 / P1.3 (same!)** |

→ `main.rs` `P1SEL0` routing changes **`0x3C` → `0xCC`** (I2C P1.2/P1.3 = `0x0C` unchanged; UART
moves P1.4/P1.5 `0x30` → P1.6/P1.7 `0xC0`). `uart.rs` register accessors (`uca0ctlw0`, `uca0brw`,
`uca0ifg`, `uca0txbuf`) are standard eUSCI_A names → **source should compile unchanged** against the
FR2355 PAC (confirm by trial compile).

### 5. Dual-I²C allocation — **Sensor I²C on the plug, Stem I²C on pins** (Henrik, 2026-09-25)
FR2355 is the **canonical stem MCU** (2×I²C supervisor; supersedes FR2476). Only **eUSCI_B0/B1** can
do I²C, so the two required buses consume **both** B modules (canonical: bench-bringup.md, bench-v1.md
"Canonical stem MCU"). The physical rule Henrik set: **the Sensor (Signal) I²C bus — MSP430 master —
must land on the board's plug** (so tinker/sensor breakouts plug in); the **Stem I²C — MSP430 slave,
SoM-facing (Stembus) — goes on header pins** (board-internal trace to the SoM, no connector needed).

eUSCI_B I²C pins (FR2355 datasheet Rev. D):
| Bus | Module | SDA / SCL pins | Role | Destination |
|---|---|---|---|---|
| **Sensor / Signal I²C** | eUSCI_B0 | **P1.2 / P1.3** | MSP430 **master** | **→ the plug** (sensors) |
| **Stem I²C (Stembus)** | eUSCI_B1 | **P4.6 / P4.7** | MSP430 **slave** (to SoM) | **→ header pins** |

- Assignment rationale: diag already drives its sensor scan on **B0 (P1.2/P1.3)**, and P1.2/P1.3 are
  the conventional BoosterPack I²C position where a Grove/Qwiic plug lands → **Sensor = B0 → plug.**
  The Stem slave bus takes **B1 (P4.6/P4.7)** on bare pins.
- **Pin-mux:** `main.rs` sets `P1SEL0=0xCC` (B0 I²C P1.2/P1.3 + A0 UART P1.6/P1.7) **and** now
  `P4SEL0 |= 0xC0` (B1 I²C P4.6/P4.7). This P4 routing is **new** vs the single-bus FR2476 diag.
- **Diag scope:** diag exercises the **Sensor** bus (B0, master) as today; **reserve B1 (P4.6/P4.7)**
  for the Stem bus so diag and prod agree on the pin plan even though the slave role is a prod
  concern. Diag can later add a Stembus-pin continuity/loopback check (bench objective #11).
- **MUST-CONFIRM before finalizing:** which eUSCI_B the **actual board's plug** is wired to. On the
  bare FR2355 LaunchPad there is no native plug (a Grove adapter lands on BoosterPack pins → confirm
  against **SLAU680**); on the product/bench board the connector wiring decides. If the plug is
  physically on B1's pins, **swap** the assignment (Sensor=B1, Stem=B0). The *rule* (Sensor→plug,
  Stem→pins) is fixed; the B0/B1 mapping follows the board.
- **SOURCE OF TRUTH = `crates/bsp/connections.toml`, NOT this doc.** The registry already carries the
  bus roles (`sensor`=eUSCI_B0 master-switch, `stem`=eUSCI_B1 slave) and is the canonical pin home
  ("keep role facts HERE"). The **FR2355 pin entries + the plug/pins destination are being authored by
  Henrik in a parallel thread** (the current file is still the FR2476 registry: `[chip] part=
  MSP430FR2476`). The pin values in the table above are **datasheet-derived and PROVISIONAL** — when
  the FR2355 `connections.toml` lands, reconcile `main.rs`/`memory-fr2355.x` against it; do not hand-
  maintain a second copy here.

## Approach — feature-gated PAC alias (recommended; NOT the speculative board-crate)
- Cargo features `chip-fr2476` (default) / `chip-fr2355`; PACs as optional deps aliased to one name
  (`#[cfg(feature="chip-fr2355")] use msp430fr2355 as pac;` etc.). `hal.rs` and the peripheral
  modules take `&pac::Peripherals`.
- Per-chip `#[cfg]` **only** where they differ: `main.rs` `P1SEL0` routing, `usec.rs` timer.
- `build.rs` selects `memory-<chip>.x`; wire `DIAG_TARGET`/feature to pick the chip (today a stamp).
- Unchanged (pending trial compile): `uart.rs`, `i2c.rs`, `adc.rs`, `clock.rs` register access;
  `crates/devices`, `crates/sched`, the diag logic.
- **Do not** build the anticipated board-crate HAL abstraction yet — get FR2355 running first; the
  trial compile tells us whether that refactor is worth it.

## File-by-file change list
| File | Change | Size |
|---|---|---|
| `diag/memory-fr2355.x` | new; values above | trivial |
| `diag/Cargo.toml` | feature-gated PAC deps + aliases | small |
| `diag/build.rs` | select `memory-<chip>.x`; `DIAG_TARGET`→chip | small |
| `diag/src/main.rs` | `#[cfg]` `P1SEL0` = `0xCC` for fr2355 | small (value known) |
| `diag/src/usec.rs` | `#[cfg]` TA0 → Timer_B for fr2355 | small |
| `diag/src/*.rs` PAC import | `msp430fr2476` → aliased `pac` | mechanical |
| `justfile`/`diag.just` | `just diag build --chip fr2355` | small |

## Still to verify (before/while implementing)
1. **Register-field parity** for `cs`/`frctl`/`adc`/`e_usci_*` between the PACs — fastest via a
   **trial compile** of diag against `msp430fr2355` (compile-driven delta discovery). Likely small
   given shared FRAM-value-line IP, but unconfirmed.
2. **MSP-EXP430FR2355 LaunchPad backchannel jumper wiring** — confirm the eZ-FET UART is bridged to
   **P1.6/P1.7** (chip UCA0). Needs the **LaunchPad user guide** (not the chip datasheet).
3. **Clock init** (`clock.rs`) CS/DCO/FRCTL register parity + FR2355 DCO settings for 1 MHz SMCLK /
   the 9600 baud divisor (`uca0brw=6`) — reconfirm the divisor holds at the FR2355 clock.

## Effort
**Small–Medium.** Every *known* delta (memory, timer, UART pins) is resolved with cited values;
the only open effort is the trial-compile register-parity pass. Once it builds, flash + verify is
already bench-ready on today's thepia (no hwd changes needed — see the reports).

## hwd-side (thepia, reported — not blocking this firmware work)
`family: unknown-family` for FR2355 → add FR2355/FR2155 to `family_of`; add an
`msp430-launchpad-fr2355` profile; surface the real detected part in `list`/`status` (C26).
