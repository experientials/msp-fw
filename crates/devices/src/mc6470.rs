//! MC6470 accelerometer (0x4C) — the accel half of the mCube MC6470 eCompass (6DOF IMU 13 Click,
//! MIKROE-4228). **Third shared [`Device`]** — ported from the proven diag-local `diag/src/mc6470.rs`
//! (registers, ±8 g/14-bit scaling, gravity-sanity math), re-expressed against the portable
//! `embedded_hal::i2c::I2c` seam so diag and prod consume ONE driver.
//!
//! Reuses the existing transaction shapes (register write + `write_read` register read) — no new bus
//! plumbing beyond APDS. Both halves of the eCompass live here: [`Mc6470`] (accel, 0x4C) and
//! [`Mc6470Mag`] (magnetometer, 0x0C) — separate I²C addresses and register maps, one physical chip.
//!
//! **No WHO_AM_I.** The MC6470 exposes no signature register, so [`identify`](Mc6470::identify) only
//! confirms it answers the register protocol (more than a bare address ACK). The high-signal identity
//! check is functional — [`self_test`](Mc6470::self_test): at rest |a| must be ~1 g. Registers +
//! scaling verified against the MC6470 datasheet via diag's known-good driver: ±8 g over 14-bit
//! signed → ~1024 LSB/g. Integer-only (no float / no soft-sqrt in the verdict).

use crate::{Device, Error};
use embedded_hal::i2c::{I2c, SevenBitAddress};

pub const ADDR: SevenBitAddress = 0x4C;

const REG_MODE: u8 = 0x07; // OPCON: 0x01 = wake/active
const REG_RANGE: u8 = 0x20; // (range<<4)|resolution
const REG_XOUT: u8 = 0x0D; // 6 bytes: X/Y/Z, little-endian, 14-bit sign-extended to 16
const MODE_WAKE: u8 = 0x01;
const RANGE_8G_14BIT: u8 = 0x25; // (0b010 << 4) | 0b101

/// ±8 g over 14-bit signed (±8191) → counts per g.
pub const LSB_PER_G: i32 = 1024;
// Gravity acceptance on the SQUARED magnitude (no sqrt in the verdict): 0.7 g .. 1.3 g.
const LO_LSB: i32 = LSB_PER_G * 7 / 10;
const HI_LSB: i32 = LSB_PER_G * 13 / 10;
const G_LO2: i32 = LO_LSB * LO_LSB;
const G_HI2: i32 = HI_LSB * HI_LSB;

/// Bounded poll for the accel to wake (MODE_WAKE has latency; the first samples read all-zero, which
/// is never real gravity). Each read is a bounded I²C txn, so this spans the wake latency without a
/// busy delay; on a warm part the first read is already non-zero.
const WAKE_TRIES: u16 = 64;

/// A raw acceleration sample in signed counts (~±8191 = ±8 g at the ±8 g/14-bit setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accel {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

impl Accel {
    /// Squared magnitude in LSB² (no sqrt — for the gravity-band verdict).
    pub fn sumsq(&self) -> i32 {
        (self.x as i32) * (self.x as i32)
            + (self.y as i32) * (self.y as i32)
            + (self.z as i32) * (self.z as i32)
    }
    /// Vector magnitude |a| in milli-g (integer sqrt; for reporting, not the verdict).
    pub fn magnitude_mg(&self) -> i32 {
        isqrt(self.sumsq() as u32) as i32 * 1000 / LSB_PER_G
    }
    /// At-rest gravity sanity: |a| within 0.7–1.3 g. Catches a dead axis / cold joint / unwoken part
    /// that a bare ACK can't. Checked on the squared magnitude to avoid a sqrt in the decision.
    pub fn is_gravity(&self) -> bool {
        let s = self.sumsq();
        s > G_LO2 && s < G_HI2
    }
}

/// Zero-sized marker; behaviour is in the methods (monomorphised, LTO-stripped if unused).
pub struct Mc6470;

impl Mc6470 {
    /// Power on the accel (±8 g/14-bit, then wake). Idempotent; `Err(Bus)` on NACK.
    pub fn enable<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        bus.write(ADDR, &[REG_RANGE, RANGE_8G_14BIT])?;
        bus.write(ADDR, &[REG_MODE, MODE_WAKE])?;
        Ok(())
    }

    /// Read X/Y/Z as signed counts. Does NOT enable — caller enables once (or use [`measure`]).
    pub fn read_xyz<I: I2c>(bus: &mut I) -> Result<Accel, Error<I::Error>> {
        let mut d = [0u8; 6];
        bus.write_read(ADDR, &[REG_XOUT], &mut d)?;
        Ok(Accel {
            x: i16::from_le_bytes([d[0], d[1]]),
            y: i16::from_le_bytes([d[2], d[3]]),
            z: i16::from_le_bytes([d[4], d[5]]),
        })
    }
}

impl Device for Mc6470 {
    const NAME: &'static str = "MC6470 accel";
    const ADDR: SevenBitAddress = ADDR;
    type Reading = Accel;

    /// No WHO_AM_I on this part — confirm it answers a register read (more than a bare address ACK).
    /// The strong identity check is functional: [`self_test`](Self::self_test).
    fn identify<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        Self::read_xyz(bus).map(|_| ())
    }

    /// Enable, then poll for a fully-woken sample. `NotReady` if it never wakes within the budget
    /// (only bites the first cold-boot pass; the next read is clean).
    ///
    /// A cold wake brings the axis output registers online one at a time — a read caught mid-wake
    /// looks like e.g. `(265, 0, 0)` (X latched, Y/Z still zero), which would read as a bogus
    /// sub-1 g vector. A fully-woken accel at rest has real data (with noise) on the axes, so require
    /// **≥2 non-zero axes** to accept a sample — this rejects the `(x,0,0)`/`(0,0,0)` partials while
    /// tolerating one genuinely-zero axis. (Observed on the FR2476 bench: prod read the accel once at
    /// boot and caught the X-only partial; diag, reading repeatedly when warm, never did.)
    fn measure<I: I2c>(bus: &mut I) -> Result<Self::Reading, Error<I::Error>> {
        Self::enable(bus)?;
        for _ in 0..WAKE_TRIES {
            let a = Self::read_xyz(bus)?;
            let awake = (a.x != 0) as u8 + (a.y != 0) as u8 + (a.z != 0) as u8;
            if awake >= 2 {
                return Ok(a);
            }
        }
        Err(Error::NotReady)
    }

    /// Functional identity: a woken sample whose magnitude is ~1 g. `Identity` if a valid sample is
    /// out of the gravity band (present but wrong/faulty), propagating bus/not-ready errors.
    fn self_test<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        let a = Self::measure(bus)?;
        if a.is_gravity() {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }
}

// ===========================================================================================
// MC6470 magnetometer (0x0C) — the mag half of the same eCompass. Separate I²C address + register
// map (MC6470 datasheet APS-048-0033v1.7 §8 "Magnetometer Register Interface", Table 15 p.36).
// ===========================================================================================

pub const MAG_ADDR: SevenBitAddress = 0x0C;

const MREG_WHO_AM_I: u8 = 0x0F; // "Who I am" — reads 0x49 (Table 15)
const MAG_WHO_AM_I: u8 = 0x49;
const MREG_OUTX_L: u8 = 0x10; // OUTX/Y/Z at 0x10..0x15, signed 16-bit, little-endian (§8 header)
const MREG_STATUS: u8 = 0x18; // Status: DRDY = bit 6
const MSTATUS_DRDY: u8 = 0x40;
const MREG_CTRL1: u8 = 0x1B; // PC = bit7 (active), FS = bit1 (force); POR default 0x0A (FS=1)
const MCTRL1_ACTIVE_FORCE: u8 = 0x8A; // 0x0A | PC — active, force state, default ODR
const MREG_CTRL3: u8 = 0x1D; // FORCE = bit6 (trigger a synchronous measurement)
const MCTRL3_FORCE: u8 = 0x40;

/// Bounded poll for a forced magnetometer conversion to complete (STATUS.DRDY). Each iteration is a
/// bounded read; the cap only bounds the worst case.
const MAG_DRDY_TRIES: u16 = 64;

/// Magnetometer sensitivity: 0.15 µT/LSB (datasheet Table 4). So field in centi-µT = counts × 15.
const MAG_CENTI_UT_PER_LSB: i32 = 15;

/// A magnetic-field sample in signed counts (0.15 µT/LSB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagField {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

impl MagField {
    fn sumsq(&self) -> u32 {
        (self.x as i32 * self.x as i32
            + self.y as i32 * self.y as i32
            + self.z as i32 * self.z as i32) as u32
    }
    /// Field magnitude |B| in **centi-µT** (hundredths of a µT): `isqrt(sumsq) × 15`. Earth's field
    /// (~25–65 µT) lands around 2500–6500 here — a plausibility/liveness signal.
    pub fn magnitude_centi_ut(&self) -> i32 {
        isqrt(self.sumsq()) as i32 * MAG_CENTI_UT_PER_LSB
    }
}

/// Zero-sized marker for the magnetometer half.
pub struct Mc6470Mag;

impl Mc6470Mag {
    /// Enter active + force state (PC=1). Idempotent; `Err(Bus)` on NACK.
    pub fn enable<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        bus.write(MAG_ADDR, &[MREG_CTRL1, MCTRL1_ACTIVE_FORCE])?;
        Ok(())
    }
}

impl Device for Mc6470Mag {
    const NAME: &'static str = "MC6470 mag";
    const ADDR: SevenBitAddress = MAG_ADDR;
    type Reading = MagField;

    /// WHO_AM_I: "Who I am" (0x0F) must read 0x49 (Table 15) — a real signature, unlike the accel.
    fn identify<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        let mut id = [0u8; 1];
        bus.write_read(MAG_ADDR, &[MREG_WHO_AM_I], &mut id)?;
        if id[0] == MAG_WHO_AM_I {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }

    /// Enter active/force, trigger a forced measurement (CTRL3.FORCE), poll STATUS.DRDY, then read
    /// OUTX/Y/Z (signed 16-bit LE). `NotReady` if DRDY never asserts within the budget.
    fn measure<I: I2c>(bus: &mut I) -> Result<Self::Reading, Error<I::Error>> {
        Self::enable(bus)?;
        bus.write(MAG_ADDR, &[MREG_CTRL3, MCTRL3_FORCE])?; // trigger a synchronous measurement
        let mut ready = false;
        for _ in 0..MAG_DRDY_TRIES {
            let mut st = [0u8; 1];
            bus.write_read(MAG_ADDR, &[MREG_STATUS], &mut st)?;
            if st[0] & MSTATUS_DRDY != 0 {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err(Error::NotReady);
        }
        let mut d = [0u8; 6];
        bus.write_read(MAG_ADDR, &[MREG_OUTX_L], &mut d)?;
        Ok(MagField {
            x: i16::from_le_bytes([d[0], d[1]]),
            y: i16::from_le_bytes([d[2], d[3]]),
            z: i16::from_le_bytes([d[4], d[5]]),
        })
    }
}

/// Integer sqrt (bit-by-bit), for reporting |a| in milli-g without a soft-float. (Ported from diag.)
fn isqrt(mut n: u32) -> u16 {
    let mut x = 0u32;
    let mut bit = 1u32 << 30;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if n >= x + bit {
            n -= x + bit;
            x = (x >> 1) + bit;
        } else {
            x >>= 1;
        }
        bit >>= 2;
    }
    x as u16
}
