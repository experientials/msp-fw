//! eUSCI_A0 backchannel UART, 9600 8N1 — the report channel (and the on-demand trigger via RX).
//! TX ported from diag's proven uart.rs; RX added so a char on the backchannel triggers a re-scan.
//! Pins routed in main (P1.4/P1.5 → UCA0).

use crate::pac::Peripherals;

const UCSWRST: u16 = 0x0001;
const UCSSEL_SMCLK: u16 = 0x0080;
const UCOS16: u16 = 0x0001;
const UCTXIFG: u16 = 0x0002;
const UCRXIFG: u16 = 0x0001;

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

/// Print a hundredths value as a fixed-point `D.DD` (e.g. 2519 → "25.19"). Integer-only, no float.
/// Ported from diag/src/uart.rs — for the Si7021 T/RH centi values.
pub fn fixed2(p: &Peripherals, centi: i32) {
    let mut v = centi;
    if v < 0 {
        putc(p, b'-');
        v = -v;
    }
    dec(p, (v / 100) as u16); // integer part (a few hundred at most here) fits u16
    putc(p, b'.');
    let f = (v % 100) as u16;
    putc(p, b'0' + (f / 10) as u8);
    putc(p, b'0' + (f % 10) as u8);
}

/// A byte is waiting in the RX buffer (non-blocking) — used to poll for the on-demand trigger.
pub fn rx_ready(p: &Peripherals) -> bool {
    p.e_usci_a0.uca0ifg().read().bits() & UCRXIFG != 0
}

/// Read the pending RX byte (reading UCA0RXBUF clears UCRXIFG). Call only when `rx_ready`.
pub fn getc(p: &Peripherals) -> u8 {
    p.e_usci_a0.uca0rxbuf().read().bits() as u8
}
