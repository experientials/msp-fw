//! FR2476 LaunchPad (MSP-EXP430FR2476) target.
//! Backchannel UART = eUSCI_A0 on P1.4/P1.5; µs base = Timer_A (TA0); manual DCO trim; 8 KB RAM.

use crate::pac::Peripherals;

// Clock-system bits (msp430fr2476.h).
const SELREF_REFOCLK: u16 = 0x0010;
const DCOFTRIMEN: u16 = 0x0080;
const DCOFTRIM0: u16 = 0x0010;
const DCOFTRIM1: u16 = 0x0020;
const DCORSEL_0: u16 = 0x0000;
const FLLD_0: u16 = 0x0000;
const SELMS_DCOCLKDIV: u16 = 0x0000;
const SELA_REFOCLK: u16 = 0x0100;
const FLLUNLOCK: u16 = 0x0300; // FLLUNLOCK0 | FLLUNLOCK1 (CSCTL7)

/// MCLK = SMCLK = DCODIV = 1 MHz (FLL ref = REFO), ACLK = REFO. Disables the FLL (SCG0) while
/// retuning and waits for re-lock before switching the clocks — required for a clean 9600 baud
/// UART; the loose "close enough" version garbles the first bytes.
pub fn clock_init_1mhz(p: &Peripherals) {
    unsafe { core::arch::asm!("bis #0x40, r2", options(nomem, nostack)) }; // SCG0=1: FLL off
    p.cs.csctl3().modify(|r, w| unsafe { w.bits(r.bits() | SELREF_REFOCLK) });
    p.cs
        .csctl1()
        .write(|w| unsafe { w.bits(DCOFTRIMEN | DCOFTRIM0 | DCOFTRIM1 | DCORSEL_0) });
    p.cs.csctl2().write(|w| unsafe { w.bits(FLLD_0 + 30) }); // FLLN=30 -> 1 MHz
    msp430::asm::nop();
    msp430::asm::nop();
    msp430::asm::nop();
    unsafe { core::arch::asm!("bic #0x40, r2", options(nomem, nostack)) }; // SCG0=0: FLL on
    while p.cs.csctl7().read().bits() & FLLUNLOCK != 0 {} // wait for FLL lock
    p.cs.csctl4().write(|w| unsafe { w.bits(SELMS_DCOCLKDIV | SELA_REFOCLK) });
    for _ in 0..8000u16 {
        msp430::asm::nop(); // let the DCO settle before clocking the UART
    }
}

// Console-UART pins: P1.4/P1.5 -> UCA0 (SEL1:SEL0 = 01). Leaf-I2C pins are routed in board::mod.
const P1_UART_PINS: u8 = 0x30; // BIT4|BIT5
pub fn route_console_uart_pins(p: &Peripherals) {
    p.p1.p1sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P1_UART_PINS) }); // SEL1=0
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | P1_UART_PINS) }); // SEL0=1
}

// Console UART = eUSCI_A0, 9600 8N1 @ 1 MHz SMCLK (the FR2476 LaunchPad backchannel).
const UCSWRST: u16 = 0x0001;
const UCSSEL_SMCLK: u16 = 0x0080;
const UCOS16: u16 = 0x0001;
const UCTXIFG: u16 = 0x0002;

pub fn console_uart_init(p: &Peripherals) {
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

#[inline]
pub fn console_uart_tx(p: &Peripherals, c: u8) {
    while p.e_usci_a0.uca0ifg().read().bits() & UCTXIFG == 0 {}
    p.e_usci_a0.uca0txbuf().write(|w| unsafe { w.bits(c as u16) });
}

/// Snapshot the console UART's config regs (CTLW0, BRW, MCTLW) for the SFR dump — reads the
/// backchannel instance (UCA0 here) so the dump is correct per module, not hardcoded.
pub fn console_uart_regs(p: &Peripherals) -> (u16, u16, u16) {
    (
        p.e_usci_a0.uca0ctlw0().read().bits(),
        p.e_usci_a0.uca0brw().read().bits(),
        p.e_usci_a0.uca0mctlw().read().bits(),
    )
}

// µs time base: TA0 free-running off SMCLK (1 MHz -> 1 µs/tick, wraps every 65.536 ms).
const TASSEL_SMCLK: u16 = 0x0200; // TASSEL_2
const MC_CONTINUOUS: u16 = 0x0020; // MC_2
const TACLR: u16 = 0x0004;
pub fn usec_start(p: &Peripherals) {
    p.ta0
        .ta0ctl()
        .write(|w| unsafe { w.bits(TASSEL_SMCLK | MC_CONTINUOUS | TACLR) });
}
#[inline]
pub fn usec_now(p: &Peripherals) -> u16 {
    p.ta0.ta0r().read().bits()
}
