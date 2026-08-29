#![no_main]
#![no_std]
#![feature(asm_experimental_arch)] // core::arch::asm! for SCG0 (SR bit) during FLL retune

//! bob-929 diagnostic firmware (Rust) — MSP430FR2476.
//!
//! Power-on self-test in the spirit of an Amiga diag ROM: scan the I2C bus and
//! exercise each known device, reporting over the eZ-FET backchannel UART and,
//! when present, on the IS31FL3730 LED matrix. Register access goes through the
//! vendored `msp430fr2476` PAC (typed peripherals) using proven bit values from
//! the FR2476 header — not raw pointers.

extern crate panic_msp430; // infinitely-looping panic handler

use msp430_rt::entry;
use msp430fr2476::Peripherals;

mod diag;
mod i2c;
mod is31;
mod regs;
mod uart;
mod util;

// Watchdog / power-management / clock-system bits (msp430fr2476.h).
const WDTPW: u16 = 0x5A00;
const WDTHOLD: u16 = 0x0080;
const LOCKLPM5: u16 = 0x0001;
const SELREF_REFOCLK: u16 = 0x0010;
const DCOFTRIMEN: u16 = 0x0080;
const DCOFTRIM0: u16 = 0x0010;
const DCOFTRIM1: u16 = 0x0020;
const DCORSEL_0: u16 = 0x0000;
const FLLD_0: u16 = 0x0000;
const SELMS_DCOCLKDIV: u16 = 0x0000;
const SELA_REFOCLK: u16 = 0x0100;
const FLLUNLOCK: u16 = 0x0300; // FLLUNLOCK0 | FLLUNLOCK1 (CSCTL7)

// P1SEL0 bits routing the eUSCI functions: P1.2/P1.3 -> UCB0 I2C, P1.4/P1.5 -> UCA0 UART.
const P1_EUSCI_PINS: u8 = 0x3C; // BIT2|BIT3|BIT4|BIT5
const P2_SDB: u8 = 0x20; // P2.5 -> IS31FL3730 SDB (drive high to enable). NOT P2.0 — crystal XOUT.

/// MCLK = SMCLK = DCODIV = 1 MHz (FLL ref = REFO), ACLK = REFO. Disables the FLL (SCG0)
/// while retuning and waits for re-lock before switching the clocks — required for a clean
/// 9600 baud UART; the loose "close enough" version garbles the first bytes.
fn clock_init_1mhz(p: &Peripherals) {
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

#[entry]
fn main() -> ! {
    // Single-threaded, no ISRs -> steal is fine and avoids the critical-section feature.
    let p = unsafe { Peripherals::steal() };

    p.wdt_a.wdtctl().write(|w| unsafe { w.bits(WDTPW | WDTHOLD) }); // stop watchdog
    clock_init_1mhz(&p);

    // Route P1.2..P1.5 to eUSCI. The pin function is the 2-bit pair (P1SEL1:P1SEL0);
    // 01 = primary module (UCB0 I2C on P1.2/1.3, UCA0 UART on P1.4/1.5). Reset leaves
    // P1SEL1=0, but clear the bits explicitly so we don't silently depend on that and
    // accidentally select the secondary/tertiary function.
    p.p1.p1sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P1_EUSCI_PINS) }); // SEL1=0
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | P1_EUSCI_PINS) }); // SEL0=1
    p.pmm.pm5ctl0().modify(|r, w| unsafe { w.bits(r.bits() & !LOCKLPM5) });

    // Enable the IS31FL3730 (SDB high on P2.5). Set output high before direction.
    p.p2.p2sel0().modify(|r, w| unsafe { w.bits(r.bits() & !P2_SDB) }); // ensure GPIO
    p.p2.p2out().modify(|r, w| unsafe { w.bits(r.bits() | P2_SDB) });
    p.p2.p2dir().modify(|r, w| unsafe { w.bits(r.bits() | P2_SDB) });

    uart::init(&p);
    i2c::init(&p);

    uart::puts(&p, "\n\nbob-929 diag firmware ready (FR2476, Rust).\n");
    regs::dump(&p); // one-shot: the live config we reason from for the rest of the run

    loop {
        diag::run(&p);
        util::delay_ms(3000);
    }
}

// Debug builds emit calls to abort(); MSP430 has no meaningful abort.
#[no_mangle]
extern "C" fn abort() -> ! {
    panic!();
}
