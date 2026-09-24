//! VL53L0X time-of-flight ranging sensor (I²C 0x29). **Fourth shared [`Device`]** — identity level.
//!
//! **Scope: identity, not ranging (yet).** The VL53L0X has a real signature — `MODEL_ID` (0xC0) reads
//! `0xEE` — so [`identify`](Vl53l0x::identify) is a strong WHO_AM_I check (diag validates the same in
//! its inventory + stress runner). A full ranging measurement needs ST's multi-hundred-register init
//! / tuning sequence, which requires the ST datasheet/API (NOT in `datasheets/`) — it is a tracked
//! TODO (`NOTES.md`: "VL53L0X: range-a-target exercise, beyond ID-only"). Rather than guess that
//! sequence, [`measure`](Vl53l0x::measure) reads the identity registers (model/revision/module) as the
//! on-demand state — useful for inventory / config-drift — and ranging is deferred until the datasheet
//! lands. Register facts cited from the in-repo VL53L0X notes and `crates/devices` `KNOWN` table.

use crate::{Device, Error};
use embedded_hal::i2c::{I2c, SevenBitAddress};

pub const ADDR: SevenBitAddress = 0x29;

const REG_MODEL_ID: u8 = 0xC0; // reads 0xEE
const REG_REVISION_ID: u8 = 0xC1; // reads 0xAA on the common part
const REG_MODULE_ID: u8 = 0xC2; // module id (value varies by module)
const MODEL_ID: u8 = 0xEE;

/// Identity readout (what we can faithfully read today; ranging is a tracked TODO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    pub model: u8,    // 0xC0 — expect 0xEE
    pub revision: u8, // 0xC1
    pub module: u8,   // 0xC2
}

/// Zero-sized marker; behaviour is in the methods (monomorphised, LTO-stripped if unused).
pub struct Vl53l0x;

impl Vl53l0x {
    /// Read the three identity registers (model / revision / module).
    pub fn read_ids<I: I2c>(bus: &mut I) -> Result<Ids, Error<I::Error>> {
        let mut m = [0u8; 1];
        let mut r = [0u8; 1];
        let mut mo = [0u8; 1];
        bus.write_read(ADDR, &[REG_MODEL_ID], &mut m)?;
        bus.write_read(ADDR, &[REG_REVISION_ID], &mut r)?;
        bus.write_read(ADDR, &[REG_MODULE_ID], &mut mo)?;
        Ok(Ids {
            model: m[0],
            revision: r[0],
            module: mo[0],
        })
    }
}

impl Device for Vl53l0x {
    const NAME: &'static str = "VL53L0X ToF";
    const ADDR: SevenBitAddress = ADDR;
    /// Identity today (model/revision/module); a ranging reading (mm + status) lands with the init
    /// sequence — see the module docs.
    type Reading = Ids;

    /// WHO_AM_I: `MODEL_ID` (0xC0) must read `0xEE`.
    fn identify<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        let mut id = [0u8; 1];
        bus.write_read(ADDR, &[REG_MODEL_ID], &mut id)?;
        if id[0] == MODEL_ID {
            Ok(())
        } else {
            Err(Error::Identity)
        }
    }

    /// On-demand state: the identity registers (until ranging is implemented). `Identity` if the
    /// model register doesn't read `0xEE` (present but not a VL53L0X).
    fn measure<I: I2c>(bus: &mut I) -> Result<Self::Reading, Error<I::Error>> {
        let ids = Self::read_ids(bus)?;
        if ids.model == MODEL_ID {
            Ok(ids)
        } else {
            Err(Error::Identity)
        }
    }
}

// ===========================================================================================
// Coarse ranging for the SUPERVISOR's wake-on-approach role — NOT precision app ranging.
//
// Ported from Pololu's VL53L0X Arduino library (github.com/pololu/vl53l0x-arduino, VL53L0X.cpp/.h) —
// itself a faithful reduction of ST's VL53L0X API. This is the minimal single-shot path: DataInit +
// SPAD/ref setup + the default tuning blob + ref calibration, then single-shot range reads. We
// deliberately SKIP the measurement-timing-budget recomputation (~200 lines of Q-format timing math
// in the ST/Pololu code) — the tuning-blob defaults (~33 ms budget) range fine for coarse threshold/
// trend detection, which is all the supervisor needs. Accuracy is uncalibrated/coarse by design.
//
// Stateful: the sensor yields a device-specific `stop_variable` during init that each range read must
// write back, so ranging is a handle (`Vl53l0xRanging`) created by `init`, unlike the stateless
// identity `Device` above.
// ===========================================================================================

const REG_SYSRANGE_START: u8 = 0x00;
const REG_SYSTEM_SEQUENCE_CONFIG: u8 = 0x01;
const REG_MSRC_CONFIG_CONTROL: u8 = 0x60;
const REG_FINAL_RANGE_MIN_COUNT_RATE: u8 = 0x44; // Q9.7 signal-rate limit
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_0: u8 = 0xB0;
const REG_DYNAMIC_SPAD_REF_EN_START_OFFSET: u8 = 0x4F;
const REG_DYNAMIC_SPAD_NUM_REQUESTED_REF_SPAD: u8 = 0x4E;
const REG_GLOBAL_CONFIG_REF_EN_START_SELECT: u8 = 0xB6;
const REG_SYSTEM_INTERRUPT_CONFIG_GPIO: u8 = 0x0A;
const REG_GPIO_HV_MUX_ACTIVE_HIGH: u8 = 0x84;
const REG_SYSTEM_INTERRUPT_CLEAR: u8 = 0x0B;
const REG_RESULT_INTERRUPT_STATUS: u8 = 0x13;
const REG_RESULT_RANGE_STATUS: u8 = 0x14; // range value at +10 = 0x1E (16-bit, mm)
const REG_VHV_CONFIG_EXTSUP_HV: u8 = 0x89;

/// Bounded poll count for the sensor's status/measurement flags. Each iteration is one bounded I²C
/// register read (~hundreds of µs), so this spans a single-shot measurement (~33 ms) without a timer.
const POLL: u16 = 4000;

/// A range reading at/above this (mm) means "no target in the cone" (VL53L0X reports ~8190 out of
/// range). Used by the classifier to distinguish "nobody there" from a real distance.
pub const NO_TARGET_MM: u16 = 2000;

/// Pololu `DefaultTuningSettings` (VL53L0X.cpp load_tuning_settings) as an ordered (reg, val) list —
/// includes the 0xFF bank switches. Applied verbatim in order.
const TUNING: &[(u8, u8)] = &[
    (0xFF, 0x01), (0x00, 0x00),
    (0xFF, 0x00), (0x09, 0x00), (0x10, 0x00), (0x11, 0x00),
    (0x24, 0x01), (0x25, 0xFF), (0x75, 0x00),
    (0xFF, 0x01), (0x4E, 0x2C), (0x48, 0x00), (0x30, 0x20),
    (0xFF, 0x00), (0x30, 0x09), (0x54, 0x00), (0x31, 0x04), (0x32, 0x03), (0x40, 0x83),
    (0x46, 0x25), (0x60, 0x00), (0x27, 0x00), (0x50, 0x06), (0x51, 0x00), (0x52, 0x96),
    (0x56, 0x08), (0x57, 0x30), (0x61, 0x00), (0x62, 0x00), (0x64, 0x00), (0x65, 0x00), (0x66, 0xA0),
    (0xFF, 0x01), (0x22, 0x32), (0x47, 0x14), (0x49, 0xFF), (0x4A, 0x00),
    (0xFF, 0x00), (0x7A, 0x0A), (0x7B, 0x00), (0x78, 0x21),
    (0xFF, 0x01), (0x23, 0x34), (0x42, 0x00), (0x44, 0xFF), (0x45, 0x26), (0x46, 0x05),
    (0x40, 0x40), (0x0E, 0x06), (0x20, 0x1A), (0x43, 0x40),
    (0xFF, 0x00), (0x34, 0x03), (0x35, 0x44),
    (0xFF, 0x01), (0x31, 0x04), (0x4B, 0x09), (0x4C, 0x05), (0x4D, 0x04),
    (0xFF, 0x00), (0x44, 0x00), (0x45, 0x20), (0x47, 0x08), (0x48, 0x28), (0x67, 0x00),
    (0x70, 0x04), (0x71, 0x01), (0x72, 0xFE), (0x76, 0x00), (0x77, 0x00),
    (0xFF, 0x01), (0x0D, 0x01),
    (0xFF, 0x00), (0x80, 0x01), (0x01, 0xF8),
    (0xFF, 0x01), (0x8E, 0x01), (0x00, 0x01), (0xFF, 0x00), (0x80, 0x00),
];

/// An initialised VL53L0X ranging handle (holds the device's `stop_variable`).
pub struct Vl53l0xRanging {
    stop_variable: u8,
}

impl Vl53l0xRanging {
    fn wr<I: I2c>(bus: &mut I, reg: u8, val: u8) -> Result<(), Error<I::Error>> {
        bus.write(ADDR, &[reg, val])?;
        Ok(())
    }
    fn rd<I: I2c>(bus: &mut I, reg: u8) -> Result<u8, Error<I::Error>> {
        let mut b = [0u8; 1];
        bus.write_read(ADDR, &[reg], &mut b)?;
        Ok(b[0])
    }
    fn wr16<I: I2c>(bus: &mut I, reg: u8, val: u16) -> Result<(), Error<I::Error>> {
        bus.write(ADDR, &[reg, (val >> 8) as u8, val as u8])?; // 16-bit regs are big-endian
        Ok(())
    }
    fn rd16<I: I2c>(bus: &mut I, reg: u8) -> Result<u16, Error<I::Error>> {
        let mut b = [0u8; 2];
        bus.write_read(ADDR, &[reg], &mut b)?;
        Ok(((b[0] as u16) << 8) | b[1] as u16)
    }

    /// Run the minimal init and return a ranging handle. `Identity` if the model ID isn't 0xEE;
    /// `NotReady` if a calibration/SPAD step never completes within the poll budget.
    pub fn init<I: I2c>(bus: &mut I) -> Result<Self, Error<I::Error>> {
        if Self::rd(bus, REG_MODEL_ID)? != MODEL_ID {
            return Err(Error::Identity);
        }
        // DataInit: 2V8 I/O, standard-mode I2C, capture stop_variable, disable MSRC limit checks.
        let v = Self::rd(bus, REG_VHV_CONFIG_EXTSUP_HV)?;
        Self::wr(bus, REG_VHV_CONFIG_EXTSUP_HV, v | 0x01)?;
        Self::wr(bus, 0x88, 0x00)?;
        Self::wr(bus, 0x80, 0x01)?;
        Self::wr(bus, 0xFF, 0x01)?;
        Self::wr(bus, 0x00, 0x00)?;
        let stop_variable = Self::rd(bus, 0x91)?;
        Self::wr(bus, 0x00, 0x01)?;
        Self::wr(bus, 0xFF, 0x00)?;
        Self::wr(bus, 0x80, 0x00)?;
        let msrc = Self::rd(bus, REG_MSRC_CONFIG_CONTROL)?;
        Self::wr(bus, REG_MSRC_CONFIG_CONTROL, msrc | 0x12)?;
        Self::wr16(bus, REG_FINAL_RANGE_MIN_COUNT_RATE, 32)?; // 0.25 MCPS in Q9.7
        Self::wr(bus, REG_SYSTEM_SEQUENCE_CONFIG, 0xFF)?;

        // StaticInit: SPAD info + reference-SPAD map.
        let (spad_count, aperture) = Self::get_spad_info(bus)?;
        let mut ref_spad_map = [0u8; 6];
        bus.write_read(ADDR, &[REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_0], &mut ref_spad_map)?;
        Self::wr(bus, 0xFF, 0x01)?;
        Self::wr(bus, REG_DYNAMIC_SPAD_REF_EN_START_OFFSET, 0x00)?;
        Self::wr(bus, REG_DYNAMIC_SPAD_NUM_REQUESTED_REF_SPAD, 0x2C)?;
        Self::wr(bus, 0xFF, 0x00)?;
        Self::wr(bus, REG_GLOBAL_CONFIG_REF_EN_START_SELECT, 0xB4)?;
        let first = if aperture { 12u8 } else { 0 };
        let mut enabled = 0u8;
        for i in 0..48u8 {
            if i < first || enabled == spad_count {
                ref_spad_map[(i / 8) as usize] &= !(1 << (i % 8));
            } else if (ref_spad_map[(i / 8) as usize] >> (i % 8)) & 1 != 0 {
                enabled += 1;
            }
        }
        let mut wm = [0u8; 7];
        wm[0] = REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_0;
        wm[1..7].copy_from_slice(&ref_spad_map);
        bus.write(ADDR, &wm)?;

        // Default tuning settings.
        for &(reg, val) in TUNING {
            Self::wr(bus, reg, val)?;
        }

        // Interrupt config: new-sample-ready on GPIO, active low, clear.
        Self::wr(bus, REG_SYSTEM_INTERRUPT_CONFIG_GPIO, 0x04)?;
        let g = Self::rd(bus, REG_GPIO_HV_MUX_ACTIVE_HIGH)?;
        Self::wr(bus, REG_GPIO_HV_MUX_ACTIVE_HIGH, g & !0x10)?;
        Self::wr(bus, REG_SYSTEM_INTERRUPT_CLEAR, 0x01)?;

        // Disable MSRC + TCC steps (0xE8), then reference calibration (VHV + phase).
        Self::wr(bus, REG_SYSTEM_SEQUENCE_CONFIG, 0xE8)?;
        Self::wr(bus, REG_SYSTEM_SEQUENCE_CONFIG, 0x01)?;
        Self::single_ref_calibration(bus, 0x40)?;
        Self::wr(bus, REG_SYSTEM_SEQUENCE_CONFIG, 0x02)?;
        Self::single_ref_calibration(bus, 0x00)?;
        Self::wr(bus, REG_SYSTEM_SEQUENCE_CONFIG, 0xE8)?;

        Ok(Self { stop_variable })
    }

    fn get_spad_info<I: I2c>(bus: &mut I) -> Result<(u8, bool), Error<I::Error>> {
        Self::wr(bus, 0x80, 0x01)?;
        Self::wr(bus, 0xFF, 0x01)?;
        Self::wr(bus, 0x00, 0x00)?;
        Self::wr(bus, 0xFF, 0x06)?;
        let t = Self::rd(bus, 0x83)?;
        Self::wr(bus, 0x83, t | 0x04)?;
        Self::wr(bus, 0xFF, 0x07)?;
        Self::wr(bus, 0x81, 0x01)?;
        Self::wr(bus, 0x80, 0x01)?;
        Self::wr(bus, 0x94, 0x6B)?;
        Self::wr(bus, 0x83, 0x00)?;
        let mut ok = false;
        for _ in 0..POLL {
            if Self::rd(bus, 0x83)? != 0x00 {
                ok = true;
                break;
            }
        }
        if !ok {
            return Err(Error::NotReady);
        }
        Self::wr(bus, 0x83, 0x01)?;
        let tmp = Self::rd(bus, 0x92)?;
        let count = tmp & 0x7F;
        let aperture = (tmp >> 7) & 0x01 != 0;
        Self::wr(bus, 0x81, 0x00)?;
        Self::wr(bus, 0xFF, 0x06)?;
        let t2 = Self::rd(bus, 0x83)?;
        Self::wr(bus, 0x83, t2 & !0x04)?;
        Self::wr(bus, 0xFF, 0x01)?;
        Self::wr(bus, 0x00, 0x01)?;
        Self::wr(bus, 0xFF, 0x00)?;
        Self::wr(bus, 0x80, 0x00)?;
        Ok((count, aperture))
    }

    fn single_ref_calibration<I: I2c>(bus: &mut I, vhv: u8) -> Result<(), Error<I::Error>> {
        Self::wr(bus, REG_SYSRANGE_START, 0x01 | vhv)?;
        let mut ok = false;
        for _ in 0..POLL {
            if Self::rd(bus, REG_RESULT_INTERRUPT_STATUS)? & 0x07 != 0 {
                ok = true;
                break;
            }
        }
        if !ok {
            return Err(Error::NotReady);
        }
        Self::wr(bus, REG_SYSTEM_INTERRUPT_CLEAR, 0x01)?;
        Self::wr(bus, REG_SYSRANGE_START, 0x00)?;
        Ok(())
    }

    /// Single-shot range in millimetres. `NotReady` if the measurement doesn't complete within the
    /// poll budget. A returned value ≥ [`NO_TARGET_MM`] means "no target in the cone".
    pub fn read_range<I: I2c>(&self, bus: &mut I) -> Result<u16, Error<I::Error>> {
        Self::wr(bus, 0x80, 0x01)?;
        Self::wr(bus, 0xFF, 0x01)?;
        Self::wr(bus, 0x00, 0x00)?;
        Self::wr(bus, 0x91, self.stop_variable)?;
        Self::wr(bus, 0x00, 0x01)?;
        Self::wr(bus, 0xFF, 0x00)?;
        Self::wr(bus, 0x80, 0x00)?;
        Self::wr(bus, REG_SYSRANGE_START, 0x01)?;
        // wait for start bit to clear
        let mut ok = false;
        for _ in 0..POLL {
            if Self::rd(bus, REG_SYSRANGE_START)? & 0x01 == 0 {
                ok = true;
                break;
            }
        }
        if !ok {
            return Err(Error::NotReady);
        }
        // wait for a sample
        ok = false;
        for _ in 0..POLL {
            if Self::rd(bus, REG_RESULT_INTERRUPT_STATUS)? & 0x07 != 0 {
                ok = true;
                break;
            }
        }
        if !ok {
            return Err(Error::NotReady);
        }
        let range = Self::rd16(bus, REG_RESULT_RANGE_STATUS + 10)?;
        Self::wr(bus, REG_SYSTEM_INTERRUPT_CLEAR, 0x01)?;
        Ok(range)
    }
}

/// Coarse attention classification from a short range history — the supervisor's wake-on-approach
/// primitive. A single-zone ToF gives distance + trend but NO bearing, so lateral motion ("adjacent
/// / passing by") shows up as a transient at ~constant range, not as a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    NoTarget,    // nothing in the cone
    Approaching, // range shrinking (moving toward the device)
    Receding,    // range growing (moving away)
    Stationary,  // in range, roughly constant distance (present / adjacent-passing)
}

/// Tracks recent ranges to classify approach/recede/threshold. Fixed 8-sample ring, integer-only.
pub struct RangeTracker {
    samples: [u16; Self::N],
    len: usize,
    head: usize,
    /// "Near" threshold in mm — below this AND present counts as within-range (attention).
    pub near_mm: u16,
    /// Trend hysteresis in mm — |Δ| below this is treated as stationary (rejects sensor noise).
    pub hysteresis_mm: u16,
}

impl RangeTracker {
    const N: usize = 8;

    pub const fn new(near_mm: u16, hysteresis_mm: u16) -> Self {
        Self {
            samples: [0; Self::N],
            len: 0,
            head: 0,
            near_mm,
            hysteresis_mm,
        }
    }

    /// Feed a fresh range (mm). A no-target reading (≥ [`NO_TARGET_MM`]) resets the history so a
    /// re-entry starts a fresh trend instead of averaging across the gap.
    pub fn push(&mut self, mm: u16) {
        if mm >= NO_TARGET_MM {
            self.len = 0;
            self.head = 0;
            return;
        }
        self.samples[self.head] = mm;
        self.head = (self.head + 1) % Self::N;
        if self.len < Self::N {
            self.len += 1;
        }
    }

    /// Classify the current trend. Compares the mean of the older half vs the newer half of the
    /// window: newer noticeably smaller ⇒ approaching, larger ⇒ receding, else stationary.
    pub fn classify(&self) -> Attention {
        if self.len == 0 {
            return Attention::NoTarget;
        }
        if self.len < 4 {
            return Attention::Stationary; // not enough history for a trend yet
        }
        // Reconstruct chronological order from the ring, then split into older/newer halves.
        let half = self.len / 2;
        let mut older = 0i32;
        let mut newer = 0i32;
        for k in 0..self.len {
            // oldest sample is at (head - len), walking forward
            let idx = (self.head + Self::N - self.len + k) % Self::N;
            if k < half {
                older += self.samples[idx] as i32;
            } else {
                newer += self.samples[idx] as i32;
            }
        }
        let older_avg = older / half as i32;
        let newer_avg = newer / (self.len - half) as i32;
        let delta = newer_avg - older_avg; // negative ⇒ getting closer
        if delta <= -(self.hysteresis_mm as i32) {
            Attention::Approaching
        } else if delta >= self.hysteresis_mm as i32 {
            Attention::Receding
        } else {
            Attention::Stationary
        }
    }

    /// The most recent range (mm), or `None` if no target currently tracked.
    pub fn last(&self) -> Option<u16> {
        if self.len == 0 {
            None
        } else {
            Some(self.samples[(self.head + Self::N - 1) % Self::N])
        }
    }

    /// Is the tracked target within the "near" threshold?
    pub fn is_near(&self) -> bool {
        self.last().map(|mm| mm < self.near_mm).unwrap_or(false)
    }
}
