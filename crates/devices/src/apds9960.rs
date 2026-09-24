//! APDS-9960 — ambient light / proximity / gesture front-end. **First real [`Device`] in the shared
//! library**: the standard `identify` + `measure` convention, generic over `embedded_hal::i2c::I2c`,
//! so diag (identity/self-test) and prod (read proximity on demand) consume ONE driver instead of
//! forking. Ported from the proven diag-local `diag/src/apds.rs` (register map + enable/poll flow),
//! re-expressed against the portable bus seam.
//!
//! Scope matches diag: the **proximity** engine only (PON+PEN → poll STATUS.PVALID → read PDATA).
//! Proximity alone proves the analog front-end + IR LED drive are alive — enough for a POST and for
//! "report state on demand". Gesture (GMODE + the photodiode FIFO) is deliberately left out.
//!
//! Register/bit values: `diag/src/apds.rs` (in-repo, hardware-verified) and the APDS-9960 datasheet
//! register table; the WHO_AM_I entry mirrors `KNOWN` (id_reg 0x92 → 0xAB).

use crate::{Device, Error};
use embedded_hal::i2c::{I2c, SevenBitAddress};

pub const ADDR: SevenBitAddress = 0x39;

// --- register map (APDS-9960 datasheet; mirrored from diag/src/apds.rs) ---
const ID: u8 = 0x92; // read-only device ID (WHO_AM_I)
const ENABLE: u8 = 0x80; // PON | PEN | ...
const PPULSE: u8 = 0x8E; // proximity pulse length + count
const STATUS: u8 = 0x93; // PVALID in bit 1
const PDATA: u8 = 0x9C; // proximity data, 0 (far) .. 255 (near)

const PON: u8 = 0x01; // ENABLE: power on
const PEN: u8 = 0x04; // ENABLE: proximity engine
const PVALID: u8 = 0x02; // STATUS: a proximity sample is ready

/// 16 µs pulses ×8 — a usable near-field signal (the POR default is one weak pulse). From diag.
const PPULSE_CFG: u8 = 0x87;

/// Accepted device-ID values. 0xAB is the datasheet/`KNOWN` value; 0xA8 appears on some revisions.
const ID_OK: [u8; 2] = [0xAB, 0xA8];

/// Bounded poll for the first valid proximity sample after enable — each iteration is a real I²C
/// register read (bounded itself), so this can never hang; it just bounds how long we wait for the
/// engine's first cycle before returning `NotReady`.
const POLL_TRIES: u16 = 256;

/// Zero-sized marker; all behaviour is in the trait methods (monomorphised, zero-cost, LTO-stripped
/// if unused).
pub struct Apds9960;

impl Apds9960 {
    /// Power on + enable the proximity engine (idempotent). Split out so a cooperative caller can
    /// enable once and then poll `measure` repeatedly, exactly as diag's proximity task does.
    pub fn enable<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        bus.write(ADDR, &[PPULSE, PPULSE_CFG])?;
        bus.write(ADDR, &[ENABLE, PON | PEN])?;
        Ok(())
    }

    /// Read the raw device-ID register (diagnostics — lets a caller log the actual value on an
    /// `Identity` mismatch rather than guessing).
    pub fn device_id<I: I2c>(bus: &mut I) -> Result<u8, Error<I::Error>> {
        let mut id = [0u8; 1];
        bus.write_read(ADDR, &[ID], &mut id)?;
        Ok(id[0])
    }

    /// **Non-blocking** single read for cooperative pollers (diag's proximity task): return the
    /// latest proximity sample IF one is latched (`STATUS.PVALID`), else `Ok(None)`. Does NOT
    /// enable — the caller enables once and then samples the free-running engine. `Err(Bus)` means
    /// the device didn't ACK (absent / bus fault), which the caller uses to tell "gone" from
    /// "not-ready-yet". This is the primitive [`Device::measure`] polls.
    pub fn sample<I: I2c>(bus: &mut I) -> Result<Option<u8>, Error<I::Error>> {
        let mut st = [0u8; 1];
        bus.write_read(ADDR, &[STATUS], &mut st)?;
        if st[0] & PVALID == 0 {
            return Ok(None);
        }
        let mut d = [0u8; 1];
        bus.write_read(ADDR, &[PDATA], &mut d)?;
        Ok(Some(d[0]))
    }
}

impl Device for Apds9960 {
    const NAME: &'static str = "APDS-9960";
    const ADDR: SevenBitAddress = ADDR;
    /// Proximity: 0 (far) .. 255 (near).
    type Reading = u8;

    /// Confirm the right chip by its WHO_AM_I, not just an address ACK.
    fn identify<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        let id = Self::device_id(bus)?;
        if ID_OK.contains(&id) {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }

    /// One-shot proximity read: ensure the engine is on, then poll [`sample`](Self::sample)
    /// (bounded) for the first valid reading. `NotReady` if none latched within the poll budget
    /// (caller may retry). This is the self-contained "read state on demand" for prod.
    fn measure<I: I2c>(bus: &mut I) -> Result<Self::Reading, Error<I::Error>> {
        Self::enable(bus)?;
        for _ in 0..POLL_TRIES {
            if let Some(v) = Self::sample(bus)? {
                return Ok(v);
            }
        }
        Err(Error::NotReady)
    }
}
