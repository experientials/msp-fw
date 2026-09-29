#![no_std]
#![feature(asm_experimental_arch)] // core::arch::asm! for SCG0 (SR bit) during the FLL retune (board)

//! `bsp` — the shared low-level layer both **diag** and **prod** build on (STEM-DIRECTION.md:
//! "board/hal must be a shared crate"; "diag is a special case on top of the normal prod baseline").
//!
//! Contents:
//! - the **chip-select PAC alias** (`bsp::pac`) — re-exports the one selected svd2rust PAC;
//! - **`board`** — the semantic peripheral map (clock, console UART, Leaf-I2C pins + accessor,
//!   µs timer), keyed per `<chip>_<module>`;
//! - **`hal`** — the `embedded-hal` `I2c` seam the shared `crates/devices` drivers talk to;
//! - **`i2c`** — the Leaf-I2C (UCB0) driver with bounded, stuck-bus-recovering transfers.
//!
//! The chip is picked by a mutually-exclusive cargo feature that the consuming crate forwards.

#[cfg(feature = "fr2476")]
pub use msp430fr2476 as pac;
#[cfg(feature = "fr2355")]
pub use msp430fr2355 as pac;

#[cfg(all(feature = "fr2476", feature = "fr2355"))]
compile_error!("features fr2476 and fr2355 are mutually exclusive — select exactly one chip");
#[cfg(not(any(feature = "fr2476", feature = "fr2355")))]
compile_error!("bsp: select a chip feature (fr2476 or fr2355) via the consuming crate (diag/prod)");

pub mod board;
pub mod hal;
pub mod i2c;
