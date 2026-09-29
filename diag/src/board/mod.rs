//! `board` = the **module / PCB** (STEM-DIRECTION.md "Granularity of board") — the ONLY place chip
//! + wiring specifics live. Everything else in diag is chip-agnostic (via the `crate::pac` alias),
//! so callers use `board::clock_init_1mhz(&p)` etc. regardless of target.
//!
//! A module carries one MCU, so each file fuses the **chip** facts (PAC, clock trim, timer instance)
//! and the **board/module wiring** (backchannel UART instance + pins) into one `<chip>_<module>`
//! file. The cargo feature (Cargo.toml `[features]`) names the **chip** (the PAC axis) and selects
//! exactly one module below; the module is named in the filename. If one MCU ever lands on ≥2
//! modules, split the chip facts into a `chip::` layer these build on.
//!
//! The deltas today (see FR2355-SCOPE.md): clock trim strategy, backchannel UART instance + pins,
//! the µs timer instance (Timer_A on FR2476, Timer_B on FR2355), and RAM size (that one in build.rs).
//! Future: the semantic bus map (`leaf_i2c`/`stem_i2c`/`console_uart`/`spi`) belongs here too.
//!
//! Each module exposes the same surface (role+type named):
//!   clock_init_1mhz(p) · route_console_uart_pins(p) · console_uart_init(p) · console_uart_tx(p, b)
//!   · console_uart_regs(p) · usec_start(p) · usec_now(p) -> u16
//! Chip-common role bindings (leaf_i2c, its pin route) live below, shared across modules.

#[cfg(feature = "fr2476")]
mod fr2476_launchpad;
#[cfg(feature = "fr2476")]
pub use fr2476_launchpad::*;

#[cfg(feature = "fr2355")]
mod fr2355_launchpad;
#[cfg(feature = "fr2355")]
pub use fr2355_launchpad::*;

// ── Leaf I²C (semantic role: sensors / LCD / non-MCU nodes; the MCU is master) ─────────────────
// Bus-type + role in the call (`leaf_i2c`), per the agreed board seam. On BOTH current LaunchPad
// modules Leaf = UCB0 on P1.2/P1.3, so the binding + pin route are chip-common and live here rather
// than per-module; move them into a `<chip>_<module>` file if a future board wires Leaf to a
// different eUSCI_B. The Stem-I²C (slave uplink) and SPI roles land here later the same way.
use crate::pac::Peripherals;

const P1_LEAF_I2C_PINS: u8 = 0x0C; // BIT2|BIT3 -> UCB0 SDA/SCL (P1.2/P1.3)

/// Route the Leaf-I²C pins (P1.2/P1.3 → UCB0). SEL1:SEL0 = 01 selects the primary module; clear
/// SEL1 explicitly so we never depend on reset state selecting the wrong function.
pub fn route_leaf_i2c_pins(p: &Peripherals) {
    p.p1.p1sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P1_LEAF_I2C_PINS) }); // SEL1=0
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | P1_LEAF_I2C_PINS) }); // SEL0=1
}

/// The Leaf-I²C bus as an `embedded-hal` `I2c` master — the acquisition point drivers use, so they
/// name the role, never the raw `e_usci_b0`. Backed by `hal::EusciI2c` → `crate::i2c` (UCB0).
/// `allow(dead_code)`: retained seam for `crates/devices` drivers + prod (LTO-stripped, ~0 ROM),
/// same rationale as `hal.rs`; the diag POST currently drives `crate::i2c` directly.
#[allow(dead_code)]
pub fn leaf_i2c(p: &Peripherals) -> crate::hal::EusciI2c<'_> {
    crate::hal::EusciI2c::new(p)
}
