//! Si7021 / HTU21D / SHT21 temperature + relative-humidity sensor (I²C 0x40). **Second shared
//! [`Device`]** — ported from the proven diag-local `diag/src/si7021.rs` (commands, CRC, and the
//! integer conversion math), re-expressed against the portable `embedded_hal::i2c::I2c` seam so diag
//! and prod consume ONE driver.
//!
//! **No-hold measurement, poll-for-ready.** The hold-master commands (0xE5/0xE3) clock-stretch SCL
//! for the whole conversion; the bounded eUSCI layer would time out mid-stretch. So we use the
//! no-hold commands (0xF5 RH / 0xF3 T): trigger, release the bus, then **poll** — the sensor NACKs
//! reads until the conversion completes, so a read that finally ACKs *is* the result. This replaces
//! diag's fixed worst-case µs delay with datasheet no-hold polling, which needs no chip-specific
//! timer (nothing but the `I2c` bus) — the reason this can live in the portable library. Each poll
//! is a bounded read (never hangs); the loop is bounded too ([`POLL_TRIES`]).
//!
//! **Family-compatible (Si7021 / HTU21D / SHT21).** All share 0x40 + the no-hold 0xF5/0xF3 commands,
//! so `measure` reads any of them; `identify` confirms the ID sequence responds rather than pinning a
//! single part number (the SNB_3 byte is exposed via [`part_id`](Si7021::part_id) for the caller).
//!
//! Integer-only (no float on MSP430): values are hundredths (centi-°C, centi-%RH).

use crate::{present, Device, Error};
use embedded_hal::i2c::{I2c, SevenBitAddress};

pub const ADDR: SevenBitAddress = 0x40;

const CMD_MEASURE_RH_NOHOLD: u8 = 0xF5; // measure RH, no-hold (NACKs reads until done)
const CMD_MEASURE_TEMP_NOHOLD: u8 = 0xF3; // measure T, no-hold — shared by Si7021/HTU21/SHT21
const CMD_READ_FW_REV: [u8; 2] = [0x84, 0xB8]; // firmware revision: 0xFF = 1.0, 0x20 = 2.0
const CMD_READ_ID2: [u8; 2] = [0xFC, 0xC9]; // electronic ID, 2nd access (SNB_3 = part number)

/// SNB_3 part-number byte for a genuine Si7021 (0x14 = Si7020, 0x0D = Si7013). The bench part may be
/// an HTU21/SHT21 (a different byte); the driver reads those too — this is for labeling only.
pub const PART_SI7021: u8 = 0x15;

/// Bounded poll for a no-hold conversion to finish. Each iteration is one bounded read attempt that
/// NACKs (fast) until the sensor is ready, so this exits as soon as data is available; the cap only
/// bounds the worst case (SHT21 T conversion is the slowest in the family, tens of ms).
const POLL_TRIES: u16 = 256;

/// A full sample. Values in hundredths; `crc_ok` is the CRC-8 over the data word — a bus-integrity
/// signal, not just a value.
#[derive(Debug, Clone, Copy)]
pub struct Reading {
    pub temp_c_centi: i32, // hundredths of a °C
    pub rh_centi: i32,     // hundredths of a %RH, clamped 0..=10000
    pub crc_ok: bool,
}

/// Zero-sized marker; behaviour is in the methods (monomorphised, LTO-stripped if unused).
pub struct Si7021;

impl Si7021 {
    /// SNB_3 part-number byte (see [`PART_SI7021`]). `Err` if the ID sequence NACKs / times out.
    pub fn part_id<I: I2c>(bus: &mut I) -> Result<u8, Error<I::Error>> {
        bus.write(ADDR, &CMD_READ_ID2)?;
        // 2nd-access ID returns SNB_3, SNB_2, CRC, SNB_1, SNB_0, CRC; SNB_3 is the part number.
        let mut b = [0u8; 6];
        bus.read(ADDR, &mut b)?;
        Ok(b[0])
    }

    /// Firmware revision byte (0xFF = 1.0, 0x20 = 2.0). `Err` on NACK / timeout.
    pub fn firmware_rev<I: I2c>(bus: &mut I) -> Result<u8, Error<I::Error>> {
        bus.write(ADDR, &CMD_READ_FW_REV)?;
        let mut b = [0u8; 1];
        bus.read(ADDR, &mut b)?;
        Ok(b[0])
    }

    /// Trigger a no-hold measurement, then poll a bare read until the sensor ACKs (conversion done)
    /// and clocks out 2 data bytes + CRC. `NotReady` if it never completes within the poll budget.
    fn convert<I: I2c>(bus: &mut I, cmd: u8) -> Result<[u8; 3], Error<I::Error>> {
        bus.write(ADDR, &[cmd])?; // trigger; bus is free while the sensor converts
        for _ in 0..POLL_TRIES {
            let mut b = [0u8; 3];
            if bus.read(ADDR, &mut b).is_ok() {
                return Ok(b);
            }
        }
        Err(Error::NotReady)
    }
}

/// CRC-8, polynomial x⁸+x⁵+x⁴+1 (0x131), init 0x00, MSB-first — the Si7021 / SHT2x checksum over the
/// two measurement data bytes. (Ported from diag.)
fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x31 } else { crc << 1 };
        }
    }
    crc
}

impl Device for Si7021 {
    const NAME: &'static str = "Si7021 T/RH";
    const ADDR: SevenBitAddress = ADDR;
    type Reading = Reading;

    /// Confirm a T/RH-family sensor answers the electronic-ID sequence (more than a bare address
    /// ACK). Deliberately does NOT pin the part number — 0x40 + the shared no-hold commands is the
    /// contract; the exact SNB_3 (Si7021 vs HTU21/SHT21) is available via [`part_id`](Si7021::part_id).
    fn identify<I: I2c>(bus: &mut I) -> Result<(), Error<I::Error>> {
        Self::part_id(bus).map(|_| ())
    }

    /// Read both temperature and humidity as two no-hold conversions. Math ported verbatim from diag.
    fn measure<I: I2c>(bus: &mut I) -> Result<Self::Reading, Error<I::Error>> {
        // Humidity. %RH = 125*code/65536 - 6; low 2 bits are status → mask. In centi-%RH:
        // 12500*code/65536 - 600 (12500*65535 fits u32).
        let rh = Self::convert(bus, CMD_MEASURE_RH_NOHOLD)?;
        let rh_crc_ok = crc8(&rh[..2]) == rh[2];
        let rh_code = ((((rh[0] as u16) << 8) | rh[1] as u16) & 0xFFFC) as u32;
        let rh_centi = ((12500 * rh_code / 65536) as i32 - 600).clamp(0, 10000);

        // Temperature (0xF3, family-shared — NOT the Si7021-only 0xE0). Low 2 bits are status → mask.
        // T(°C) = 175.72*code/65536 - 46.85; centi-°C: 17572*code/65536 - 4685.
        let t = Self::convert(bus, CMD_MEASURE_TEMP_NOHOLD)?;
        let t_crc_ok = crc8(&t[..2]) == t[2];
        let t_code = ((((t[0] as u16) << 8) | t[1] as u16) & 0xFFFC) as u32;
        let temp_c_centi = (17572 * t_code / 65536) as i32 - 4685;

        Ok(Reading {
            temp_c_centi,
            rh_centi,
            crc_ok: rh_crc_ok && t_crc_ok,
        })
    }
}

/// Address-only presence probe (thin wrapper over [`crate::present`] at [`ADDR`]).
pub fn present_at<I: I2c>(bus: &mut I) -> bool {
    present(bus, ADDR)
}
