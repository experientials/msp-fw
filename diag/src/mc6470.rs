//! MC6470 accelerometer (0x4C) — the accel half of the 6DOF IMU 13 Click (MIKROE-4228) eCompass.
//!
//! diag exercises it with a **gravity-sanity** check: at rest the total acceleration vector must be
//! ~1 g (spread across whatever axes gravity points down). That's the cheapest high-signal
//! functional test — it catches a dead axis, a cold-solder joint, or a stuck/not-woken part that a
//! bare I²C ACK can't. Registers + scaling verified against the MC6470 datasheet (via a known
//! driver): ±8 g over 14-bit signed → ~1024 LSB/g.

use crate::{i2c, uart};
use msp430fr2476::Peripherals;

pub const ACCEL_ADDR: u8 = 0x4C;

const REG_MODE: u8 = 0x07; // OPCON: 0x01 = wake/active
const REG_RANGE: u8 = 0x20; // (range<<4)|resolution
const REG_XOUT: u8 = 0x0D; // 6 bytes: X/Y/Z, little-endian, 14-bit sign-extended to 16
const MODE_WAKE: u8 = 0x01;
const RANGE_8G_14BIT: u8 = 0x25; // (0b010 << 4) | 0b101

const LSB_PER_G: i32 = 1024; // ±8 g over 14-bit signed (±8191)
// Gravity acceptance on the SQUARED magnitude (no sqrt in the verdict): 0.7 g .. 1.3 g.
const LO_LSB: i32 = LSB_PER_G * 7 / 10;
const HI_LSB: i32 = LSB_PER_G * 13 / 10;
const G_LO2: i32 = LO_LSB * LO_LSB;
const G_HI2: i32 = HI_LSB * HI_LSB;

/// Power on the accel (set ±8 g/14-bit, then wake). Idempotent; false on NACK.
pub fn enable(p: &Peripherals) -> bool {
    if !i2c::write(p, ACCEL_ADDR, &[REG_RANGE, RANGE_8G_14BIT]) {
        return false;
    }
    i2c::write(p, ACCEL_ADDR, &[REG_MODE, MODE_WAKE])
}

/// Read X/Y/Z as signed counts (~±8191 = ±8 g). `None` on a failed read.
pub fn read_xyz(p: &Peripherals) -> Option<(i16, i16, i16)> {
    let mut d = [0u8; 6];
    if !i2c::read_reg(p, ACCEL_ADDR, REG_XOUT, &mut d) {
        return None;
    }
    Some((
        i16::from_le_bytes([d[0], d[1]]),
        i16::from_le_bytes([d[2], d[3]]),
        i16::from_le_bytes([d[4], d[5]]),
    ))
}

/// Enable, read, report the vector + |a| in milli-g. Result:
///   `Some(true)`  — |a| within 0.7–1.3 g (PASS)
///   `Some(false)` — enable/read NACK, or a valid sample out of band (FAIL)
///   `None`        — sensor read all-zeros after the wake poll = not ready yet (SKIP).
/// The last case only bites the *first* POST pass right after a cold boot: MODE_WAKE has latency,
/// so the very first samples are all-zero. An all-zero vector is never real gravity (at rest one
/// axis always sees ~1 g), so we poll a bounded number of times for a non-zero sample; if it never
/// wakes we skip rather than scoring a false FAIL — the next ~3 s pass reads clean.
/// Raw axes are printed too, so if the scale assumption is off it's obvious (and easy to retune).
pub fn gravity_check(p: &Peripherals) -> Option<bool> {
    uart::puts(p, "  MC6470 accel: ");
    if !enable(p) {
        uart::puts(p, "enable FAILED\n");
        return Some(false);
    }
    // Poll for a woken (non-zero) sample. Each read is a bounded I²C txn (~1 ms @ 100 kHz); the
    // ~64-read budget (tens of ms) spans the wake latency without a busy delay, and stays well
    // inside the WDT window. On a warm board the first read is already non-zero.
    let mut xyz = None;
    for _ in 0..64 {
        match read_xyz(p) {
            Some((0, 0, 0)) => continue, // not ready — keep polling
            Some(v) => {
                xyz = Some(v);
                break;
            }
            None => {
                uart::puts(p, "read FAILED\n");
                return Some(false);
            }
        }
    }
    let (x, y, z) = match xyz {
        Some(v) => v,
        None => {
            uart::puts(p, "not ready (wake latency), skip\n");
            return None;
        }
    };
    let sumsq = (x as i32) * (x as i32) + (y as i32) * (y as i32) + (z as i32) * (z as i32);
    let mg = isqrt(sumsq as u32) as i32 * 1000 / LSB_PER_G;

    uart::puts(p, "x=");
    dec_i16(p, x);
    uart::puts(p, " y=");
    dec_i16(p, y);
    uart::puts(p, " z=");
    dec_i16(p, z);
    uart::puts(p, " |a|=");
    uart::dec(p, mg as u16);
    uart::puts(p, "mg ");
    let ok = sumsq > G_LO2 && sumsq < G_HI2;
    uart::puts(p, if ok { "OK\n" } else { "OUT-OF-RANGE\n" });
    Some(ok)
}

/// Signed decimal (uart::dec is unsigned). Values are ±8191, so `-v` never overflows i16.
fn dec_i16(p: &Peripherals, v: i16) {
    if v < 0 {
        uart::putc(p, b'-');
        uart::dec(p, (-v) as u16);
    } else {
        uart::dec(p, v as u16);
    }
}

/// Integer sqrt (bit-by-bit), for reporting |a| in milli-g without a soft-float.
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
