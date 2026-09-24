//! eUSCI_A0 backchannel UART, 9600 8N1. Pins routed in main.

use msp430fr2476::Peripherals;

const UCSWRST: u16 = 0x0001;
const UCSSEL_SMCLK: u16 = 0x0080;
const UCOS16: u16 = 0x0001;
const UCTXIFG: u16 = 0x0002;

pub fn init(p: &Peripherals) {
    p.e_usci_a0.uca0ctlw0().write(|w| unsafe { w.bits(UCSWRST) });
    p.e_usci_a0
        .uca0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCSSEL_SMCLK) });
    p.e_usci_a0.uca0brw().write(|w| unsafe { w.bits(6) });
    // UCBRSx=0x20, UCBRFx=8, UCOS16=1 (TI baud table, 1 MHz / 9600).
    p.e_usci_a0
        .uca0mctlw()
        .write(|w| unsafe { w.bits(0x2000 | (8 << 4) | UCOS16) });
    p.e_usci_a0
        .uca0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
}

pub fn putc(p: &Peripherals, c: u8) {
    while p.e_usci_a0.uca0ifg().read().bits() & UCTXIFG == 0 {}
    p.e_usci_a0.uca0txbuf().write(|w| unsafe { w.bits(c as u16) });
}

pub fn puts(p: &Peripherals, s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            putc(p, b'\r');
        }
        putc(p, b);
    }
}

pub fn hex8(p: &Peripherals, v: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    putc(p, HEX[(v >> 4) as usize & 0xF]);
    putc(p, HEX[(v & 0xF) as usize]);
}

pub fn hex16(p: &Peripherals, v: u16) {
    hex8(p, (v >> 8) as u8);
    hex8(p, v as u8);
}

pub fn dec(p: &Peripherals, mut v: u16) {
    if v == 0 {
        putc(p, b'0');
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = 0;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        putc(p, buf[i]);
    }
}

/// Signed decimal (`dec` is unsigned). Accel axes are ±8191, so `-v` never overflows i16.
pub fn dec_i16(p: &Peripherals, v: i16) {
    if v < 0 {
        putc(p, b'-');
        dec(p, (-v) as u16);
    } else {
        dec(p, v as u16);
    }
}

/// Signed hundredths as a fixed-point decimal, e.g. `-1234 -> "-12.34"`. For sensor values kept
/// as centi-units (no float on msp430): temperature in centi-°C, humidity in centi-%RH.
pub fn fixed2(p: &Peripherals, centi: i32) {
    let mut v = centi;
    if v < 0 {
        putc(p, b'-');
        v = -v;
    }
    dec(p, (v / 100) as u16); // integer part (≤ a few hundred here) fits u16
    putc(p, b'.');
    let f = (v % 100) as u16;
    putc(p, b'0' + (f / 10) as u8);
    putc(p, b'0' + (f % 10) as u8);
}
