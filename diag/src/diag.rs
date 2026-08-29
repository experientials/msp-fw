//! POST runner: bus scan + per-device tests, reported over UART and shown on the matrix.
//! Add a device by adding a test function and a line in `run`.

use crate::{i2c, is31, uart, util};
use msp430fr2476::Peripherals;

// 8x8 status glyphs (orientation may be mirrored/rotated on real hardware; fix once seen).
const GLYPH_CHECK: [u8; 8] = [0x01, 0x03, 0x06, 0x8C, 0xD8, 0x70, 0x20, 0x00];
const GLYPH_CROSS: [u8; 8] = [0x00, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x00];

fn test_led(p: &Peripherals) -> bool {
    if !is31::present(p) {
        uart::puts(p, "      hint: 0x60-0x63 silent -> check JP1 open + EB powered\n");
        return false;
    }
    if !is31::init(p) {
        return false;
    }
    // All pixels on, then a checker — reveals dead rows/cols and byte orientation.
    let all = [0xFFu8; 8];
    let mut checker = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        checker[i] = if i & 1 == 0 { 0xAA } else { 0x55 };
        i += 1;
    }
    if !is31::show(p, &all) {
        return false;
    }
    util::delay_ms(400);
    is31::show(p, &checker);
    util::delay_ms(400);
    true
}

fn scan(p: &Peripherals) {
    // Idle bus level (P1IN reflects the real pad even when muxed to UCB0):
    //   L = no pull-ups reaching us -> EB unpowered or SDA/SCL not connected.
    //   H = healthy idle bus -> if still nothing ACKs, suspect JP1/SDB/address, not wiring.
    let lv = p.p1.p1in().read().bits();
    uart::puts(p, "bus idle SDA=");
    uart::putc(p, if lv & 0x04 != 0 { b'H' } else { b'L' }); // P1.2
    uart::puts(p, " SCL=");
    uart::putc(p, if lv & 0x08 != 0 { b'H' } else { b'L' }); // P1.3
    uart::puts(p, "\nI2C scan:");
    let mut found = 0u16;
    let mut a = 0x08u8;
    while a <= 0x77 {
        if i2c::probe(p, a) {
            uart::puts(p, " 0x");
            uart::hex8(p, a);
            found += 1;
        }
        a += 1;
    }
    if found == 0 {
        uart::puts(p, " (none)");
    }
    uart::puts(p, "\n");
}

pub fn run(p: &Peripherals) {
    uart::puts(p, "\n=== bob-929 diag POST ===\n");
    scan(p);

    let total = 1u16;
    let mut passed = 0u16;

    uart::puts(p, "  LED matrix (IS31FL3730 @0x60-63)");
    if test_led(p) {
        uart::puts(p, "  PASS\n");
        passed += 1;
    } else {
        uart::puts(p, "  FAIL\n");
    }

    uart::puts(p, "summary: ");
    uart::dec(p, passed);
    uart::puts(p, "/");
    uart::dec(p, total);
    uart::puts(p, " passed\n");

    // Visual verdict, if the matrix is alive to show it.
    if is31::present(p) {
        is31::show(
            p,
            if passed == total { &GLYPH_CHECK } else { &GLYPH_CROSS },
        );
    }
}
