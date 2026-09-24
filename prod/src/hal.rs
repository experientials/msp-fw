//! embedded-hal 1.0 `I2c` over eUSCI_B0 — prod's FR247x bus impl, the per-family seam the shared
//! `crates/devices` drivers talk to. Same shape as diag/src/hal.rs (that comment: "trait surface
//! stays the same"). This is the ONLY chip-specific glue between the portable device library and
//! the FR2476 silicon.
//!
//! Write ops (incl. the zero-length presence probe used by `devices::present`) go through the
//! bounded, stuck-bus-recovering [`crate::i2c`]. Register reads — the `write_read` shape
//! `[Write(&[reg]), Read(&mut buf)]` that the shared device drivers use — map to
//! `crate::i2c::read_reg` (repeated-START). Added for the first `Device::measure` (APDS-9960).

use embedded_hal::i2c::{self, ErrorType, I2c, Operation, SevenBitAddress};
use crate::pac::Peripherals;

/// Owns "the sensor I2C bus" for the device drivers. Just a `&Peripherals`, so it's `Copy`.
#[derive(Clone, Copy)]
pub struct EusciI2c<'a> {
    p: &'a Peripherals,
}

impl<'a> EusciI2c<'a> {
    pub fn new(p: &'a Peripherals) -> Self {
        Self { p }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Error {
    /// Bounded `i2c::write`/probe failed — a NACK or stuck-bus timeout (not distinguished).
    Bus,
    /// A transaction shape not implemented yet (repeated-start reads).
    Unsupported,
}

impl i2c::Error for Error {
    fn kind(&self) -> i2c::ErrorKind {
        match self {
            Error::Bus => i2c::ErrorKind::NoAcknowledge(i2c::NoAcknowledgeSource::Unknown),
            _ => i2c::ErrorKind::Other,
        }
    }
}

impl<'a> ErrorType for EusciI2c<'a> {
    type Error = Error;
}

impl<'a> I2c<SevenBitAddress> for EusciI2c<'a> {
    fn transaction(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), Self::Error> {
        for op in ops {
            match op {
                // Empty write = presence probe (devices::present); non-empty = a real write.
                Operation::Write(buf) => {
                    if !crate::i2c::write(self.p, addr, buf) {
                        return Err(Error::Bus);
                    }
                }
                // Bare read (no register pointer) — e.g. the Si7021 no-hold result / ID sequences.
                Operation::Read(buf) => {
                    if !crate::i2c::read(self.p, addr, buf) {
                        return Err(Error::Bus);
                    }
                }
            }
        }
        Ok(())
    }

    /// Register read — the `[Write(&[reg]), Read]` shape, mapped to the repeated-START `read_reg`.
    /// Overridden (rather than routed through `transaction`) so the write→read stays a single
    /// repeated-START transfer with no STOP between, as sensors require. Only a 1-byte register
    /// pointer is supported (all current devices); a wider pointer returns `Unsupported`.
    fn write_read(&mut self, addr: u8, write: &[u8], read: &mut [u8]) -> Result<(), Self::Error> {
        if write.len() != 1 {
            return Err(Error::Unsupported);
        }
        if crate::i2c::read_reg(self.p, addr, write[0], read) {
            Ok(())
        } else {
            Err(Error::Bus)
        }
    }
}
