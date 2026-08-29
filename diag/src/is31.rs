//! ISSI/Lumissil IS31FL3730 8x8 LED matrix driver (QFLS2-EB).
//! See bob-929/docs/IS31FL3730_EB.md. Byte->pixel orientation is TBD until seen
//! on hardware; the all-on / checker self-test will reveal it.
//!
//! Address: 7-bit `0b11000_AA`, AA set by the AD pin (datasheet Table 1):
//!   AD->GND = 0x60,  AD->SCL = 0x61,  AD->SDA = 0x62,  AD->VCC = 0x63.
//! (0x74 belongs to the IS31FL3731, a *different* chip.) On the QFLS2-EB, AD is
//! hardwired to GND (EB schematic Fig. 2) => 0x60 — but we still probe all four so
//! the same firmware works on boards with a different strap.

use crate::i2c;
use msp430fr2476::Peripherals;

const ADDR_LO: u8 = 0x60;
const ADDR_HI: u8 = 0x63;

const REG_CONFIG: u8 = 0x00;
const REG_DATA: u8 = 0x01; // matrix-1 data start (auto-increment)
const REG_UPDATE: u8 = 0x0C; // write anything to latch
const REG_PWM: u8 = 0x19;
const CONFIG_8X8: u8 = 0x00; // 8x8 matrix, normal op, audio off
const PWM_DEFAULT: u8 = 0x10; // dim but clearly visible (range 0x00..0x80)

/// First AD-strap address (0x60..=0x63) that ACKs, if any.
fn addr(p: &Peripherals) -> Option<u8> {
    let mut a = ADDR_LO;
    while a <= ADDR_HI {
        if i2c::probe(p, a) {
            return Some(a);
        }
        a += 1;
    }
    None
}

pub fn present(p: &Peripherals) -> bool {
    addr(p).is_some()
}

pub fn init(p: &Peripherals) -> bool {
    match addr(p) {
        Some(a) => {
            i2c::write(p, a, &[REG_CONFIG, CONFIG_8X8]) && i2c::write(p, a, &[REG_PWM, PWM_DEFAULT])
        }
        None => false,
    }
}

pub fn show(p: &Peripherals, frame: &[u8; 8]) -> bool {
    let a = match addr(p) {
        Some(a) => a,
        None => return false,
    };
    let mut buf = [0u8; 9];
    buf[0] = REG_DATA;
    buf[1..].copy_from_slice(frame);
    i2c::write(p, a, &buf) && i2c::write(p, a, &[REG_UPDATE, 0x00])
}
