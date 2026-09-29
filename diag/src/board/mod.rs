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
//! Each module exposes the same surface:
//!   clock_init_1mhz(p) · route_uart_pins(p) · uart_init(p) · uart_tx(p, byte)
//!   · usec_start(p) · usec_now(p) -> u16

#[cfg(feature = "fr2476")]
mod fr2476_launchpad;
#[cfg(feature = "fr2476")]
pub use fr2476_launchpad::*;

#[cfg(feature = "fr2355")]
mod fr2355_launchpad;
#[cfg(feature = "fr2355")]
pub use fr2355_launchpad::*;
