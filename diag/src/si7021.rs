//! Si7021 temperature + relative-humidity sensor (I²C 0x40, shared eUSCI_B0 bus, SDA P1.2 / SCL
//! P1.3). The "Gut" sensor bus part chosen in Testing/SENSORS.md ("Always on. nA standby. No
//! interrupt.").
//!
//! **No-hold-master measurement, deliberately.** The Si7021's hold-master commands (0xE5/0xE3)
//! clock-stretch SCL for the whole conversion (~12 ms). Our bounded I²C layer (`i2c`, SPIN≈4000
//! ≈ <1 ms) would time out mid-stretch and false-fail. So we use the no-hold commands: trigger the
//! measurement, release the bus, wait out the conversion with a µs-timer delay, then read the
//! result. Nothing blocks the eUSCI master while the sensor converts.
//!
//! **Family-compatible (Si7021 / HTU21D / SHT21).** All three share the no-hold commands 0xF5
//! (RH) and 0xF3 (temperature) and the 0x40 address, so this driver reads any of them. We
//! deliberately do NOT use the Si7021-only 0xE0 ("temperature from the previous RH measurement") —
//! it NACKs on an HTU21/SHT21 and would fail the whole read. Two triggers + two delays is the price
//! of working across the family; the sensor is "always on", so the extra ~11 ms is free.
//!
//! Integer-only math (no float on msp430): results are returned in hundredths (centi-°C, centi-%RH)
//! and printed via `uart::fixed2`.

use crate::{i2c, usec};
use msp430fr2476::Peripherals;

pub const ADDR: u8 = 0x40;

const CMD_MEASURE_RH_NOHOLD: u8 = 0xF5; // measure RH, no hold master mode (NACKs read until done)
const CMD_MEASURE_TEMP_NOHOLD: u8 = 0xF3; // measure T, no hold — shared by Si7021 / HTU21 / SHT21
const CMD_READ_FW_REV: [u8; 2] = [0x84, 0xB8]; // firmware revision: 0xFF = 1.0, 0x20 = 2.0
const CMD_READ_ID2: [u8; 2] = [0xFC, 0xC9]; // electronic ID, 2nd access (SNB_3 = part number)

/// SNB_3 part-number byte returned by the electronic-ID read. 0x15 = Si7021 (0x14 = Si7020,
/// 0x0D = Si7013, 0x00/0xFF = engineering samples).
pub const PART_SI7021: u8 = 0x15;

/// No-hold conversion times vary a LOT across the family at the power-on default resolution
/// (12-bit RH / 14-bit T): Si7021 ~12/11 ms, HTU21D ~16/50 ms, SHT21 ~29/**85** ms. We delay for the
/// worst case (SHT21) so the driver reads any of them — a too-short wait makes the no-hold read
/// NACK and fail. Busy delay is fine: a POST test runs to a verdict in one tick and this is far
/// shorter than the ~1.5 s bus scan beside it; the sensor is "always on", so the wait is free.
const RH_CONVERSION_MS: u16 = 35;
const TEMP_CONVERSION_MS: u16 = 90;

/// A full sample. Values in hundredths; `crc_ok` is the CRC-8 check on the RH data word — a bus
/// integrity signal, not just a value.
pub struct Reading {
    pub temp_c_centi: i32, // hundredths of a °C
    pub rh_centi: i32,     // hundredths of a %RH, clamped 0..=10000
    pub crc_ok: bool,
}

/// Does the sensor ACK its address? (bounded probe.)
pub fn present(p: &Peripherals) -> bool {
    i2c::probe(p, ADDR)
}

/// SNB_3 part-number byte (see [`PART_SI7021`]). `None` if the ID sequence NACKs / times out.
pub fn part_id(p: &Peripherals) -> Option<u8> {
    if !i2c::write(p, ADDR, &CMD_READ_ID2) {
        return None;
    }
    // 2nd-access ID returns SNB_3, SNB_2, CRC, SNB_1, SNB_0, CRC; SNB_3 is the part number.
    let mut b = [0u8; 6];
    if !i2c::read(p, ADDR, &mut b) {
        return None;
    }
    Some(b[0])
}

/// Firmware revision byte (0xFF = 1.0, 0x20 = 2.0). `None` on NACK / timeout.
pub fn firmware_rev(p: &Peripherals) -> Option<u8> {
    if !i2c::write(p, ADDR, &CMD_READ_FW_REV) {
        return None;
    }
    let mut b = [0u8; 1];
    if !i2c::read(p, ADDR, &mut b) {
        return None;
    }
    Some(b[0])
}

/// CRC-8, polynomial x⁸+x⁵+x⁴+1 (0x131), init 0x00, MSB-first — the Si7021 / SHT2x checksum over
/// the two measurement data bytes.
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

/// Trigger a no-hold measurement (`cmd`), wait out the conversion, and read 2 data bytes + CRC with
/// a bare (pointer-less) read. `None` if the trigger or read NACKs/times out — never hangs.
fn convert(p: &Peripherals, cmd: u8, wait_ms: u16) -> Option<[u8; 3]> {
    if !i2c::write(p, ADDR, &[cmd]) {
        return None;
    }
    usec::delay_ms(p, wait_ms); // bus is free while the sensor converts
    let mut b = [0u8; 3];
    if !i2c::read(p, ADDR, &mut b) {
        return None;
    }
    Some(b)
}

/// Read back both temperature and humidity (real values) as two no-hold conversions. `None` if any
/// transfer NACKs or times out (absent sensor / bus fault) — never hangs.
pub fn measure(p: &Peripherals) -> Option<Reading> {
    // Humidity. %RH = 125 * code / 65536 - 6; low 2 bits are status, not measurement — mask them.
    // In centi-%RH: 12500 * code / 65536 - 600. (12500 * 65535 fits u32.)
    let rh = convert(p, CMD_MEASURE_RH_NOHOLD, RH_CONVERSION_MS)?;
    let rh_crc_ok = crc8(&rh[..2]) == rh[2];
    let rh_code = ((((rh[0] as u16) << 8) | rh[1] as u16) & 0xFFFC) as u32;
    let rh_centi = ((12500 * rh_code / 65536) as i32 - 600).clamp(0, 10000);

    // Temperature (0xF3, family-shared — NOT the Si7021-only 0xE0; see module docs). The low 2 bits
    // are status, not measurement — mask them off (same as RH).
    // T(°C) = 175.72 * code / 65536 - 46.85; in centi-°C: 17572 * code / 65536 - 4685.
    let t = convert(p, CMD_MEASURE_TEMP_NOHOLD, TEMP_CONVERSION_MS)?;
    let t_crc_ok = crc8(&t[..2]) == t[2];
    let t_code = ((((t[0] as u16) << 8) | t[1] as u16) & 0xFFFC) as u32;
    let temp_c_centi = (17572 * t_code / 65536) as i32 - 4685;

    Some(Reading { temp_c_centi, rh_centi, crc_ok: rh_crc_ok && t_crc_ok })
}
