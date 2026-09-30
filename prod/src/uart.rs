//! Backchannel UART, 9600 8N1 — the report channel (+ RX for the on-demand command trigger).
//! DUAL families delegate to `bsp::board::console_uart_*` — board-aware: UCA0/P1.4-5 on FR2476,
//! UCA1/P4.2-3 on FR2355 (the reason FR2355's console was silent before). FR2433 (fr24xx) drives its
//! own eUSCI_A0 (`usci_a0_uart_mode`) here, since `bsp::board` has no fr2433 module yet. Text
//! formatting (puts/hex/dec/fixed2) is shared. (Collapsing this fork into `board` = CONV-1.)

use crate::pac::Peripherals;

// FR2433-only low-level (dual families use bsp::board, not these).
#[cfg(feature = "fr24xx")]
const UCSWRST: u16 = 0x0001;
#[cfg(feature = "fr24xx")]
const UCSSEL_SMCLK: u16 = 0x0080;
#[cfg(feature = "fr24xx")]
const UCOS16: u16 = 0x0001;
#[cfg(feature = "fr24xx")]
const UCTXIFG: u16 = 0x0002;
#[cfg(feature = "fr24xx")]
const UCRXIFG: u16 = 0x0001;

/// FR2433's eUSCI_A0 — PAC field `usci_a0_uart_mode`; register methods are the standard `uca0*`.
#[cfg(feature = "fr24xx")]
macro_rules! uca0 {
    ($p:expr) => {
        $p.usci_a0_uart_mode
    };
}

pub fn init(p: &Peripherals) {
    #[cfg(feature = "_dual")]
    bsp::board::console_uart_init(p);
    #[cfg(feature = "fr24xx")]
    {
        uca0!(p).uca0ctlw0().write(|w| unsafe { w.bits(UCSWRST) });
        uca0!(p)
            .uca0ctlw0()
            .modify(|r, w| unsafe { w.bits(r.bits() | UCSSEL_SMCLK) });
        uca0!(p).uca0brw().write(|w| unsafe { w.bits(6) });
        // UCBRSx=0x20, UCBRFx=8, UCOS16=1 (TI baud table, 1 MHz / 9600).
        uca0!(p)
            .uca0mctlw()
            .write(|w| unsafe { w.bits(0x2000 | (8 << 4) | UCOS16) });
        uca0!(p)
            .uca0ctlw0()
            .modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
    }
}

pub fn putc(p: &Peripherals, c: u8) {
    #[cfg(feature = "_dual")]
    bsp::board::console_uart_tx(p, c);
    #[cfg(feature = "fr24xx")]
    {
        while uca0!(p).uca0ifg().read().bits() & UCTXIFG == 0 {}
        uca0!(p).uca0txbuf().write(|w| unsafe { w.bits(c as u16) });
    }
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
    #[cfg(feature = "_dual")]
    {
        bsp::board::console_uart_rx_ready(p)
    }
    #[cfg(feature = "fr24xx")]
    {
        uca0!(p).uca0ifg().read().bits() & UCRXIFG != 0
    }
}

/// Read the pending RX byte (reading RXBUF clears UCRXIFG). Call only when `rx_ready`.
pub fn getc(p: &Peripherals) -> u8 {
    #[cfg(feature = "_dual")]
    {
        bsp::board::console_uart_getc(p)
    }
    #[cfg(feature = "fr24xx")]
    {
        uca0!(p).uca0rxbuf().read().bits() as u8
    }
}
