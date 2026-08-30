//! embedded-hal 1.0 I2C over eUSCI_B0 — the seed of the board HAL.
//!
//! Every device driver (the OLED now; sensors later) talks to this trait object, so the
//! *diagnostic* I2C layer in [`crate::i2c`] (bounded spins, stuck-bus recovery) lives BELOW the
//! ecosystem instead of being bypassed by it. When the board crate lands, this type moves there
//! and gains the pin map + per-product device manifest; the trait surface stays the same.

use embedded_hal::i2c::{self, ErrorType, I2c, Operation, SevenBitAddress};
use msp430fr2476::Peripherals;

/// Owns "the I2C bus" for a driver. Just a `&Peripherals`, so it's `Copy` and free to hand out.
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
    /// The bounded `i2c::write` failed — a NACK or a stuck-bus timeout (it doesn't distinguish;
    /// both mean "the transfer didn't complete"). Mapped to `NoAcknowledge` for the trait.
    Bus,
    /// A transaction shape we don't implement yet (e.g. repeated-start read runs).
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
        // The OLED only ever issues single `Write` ops (command / data framed by the driver),
        // so delegating each op to the bounded `i2c::write` (full START..STOP) is correct for it.
        // Proper repeated-start handling for multi-op write-then-read (sensor register reads)
        // comes when we generalize this for objectives 2+4 — until then those return Unsupported
        // so a caller gets an honest error rather than a silently wrong transfer.
        for op in ops {
            match op {
                Operation::Write(buf) => {
                    if !crate::i2c::write(self.p, addr, buf) {
                        return Err(Error::Bus);
                    }
                }
                Operation::Read(_) => return Err(Error::Unsupported),
            }
        }
        Ok(())
    }
}
