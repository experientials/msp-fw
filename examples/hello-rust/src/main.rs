#![no_main]
#![no_std]

//! hello-rust — blink LED1 (P1.0) on the LP-MSP430FR2476.
//!
//! No svd2rust PAC exists for the FR2476, so this touches the SFRs directly via
//! `volatile` writes. The addresses are taken from the TI toolchain itself (nm on a
//! linked object), not guessed — note WDTCTL is 0x01CC on the FR2xx, not the
//! classic 0x015C:
//!   WDTCTL 0x01CC, PM5CTL0 0x0130, P1DIR 0x0204, P1OUT 0x0202.

extern crate panic_msp430; // infinitely-looping panic handler

use core::ptr::{read_volatile, write_volatile};
use msp430::asm;
use msp430_rt::entry;

const WDTCTL: *mut u16 = 0x01CC as *mut u16;
const PM5CTL0: *mut u16 = 0x0130 as *mut u16;
const P1DIR: *mut u8 = 0x0204 as *mut u8;
const P1OUT: *mut u8 = 0x0202 as *mut u8;

const WDTPW_HOLD: u16 = 0x5A00 | 0x0080; // WDTPW | WDTHOLD
const LOCKLPM5: u16 = 0x0001;
const BIT0: u8 = 0x01;

#[entry]
fn main() -> ! {
    unsafe {
        write_volatile(WDTCTL, WDTPW_HOLD); // stop the watchdog
        // FR2xx: clear LOCKLPM5 to release the GPIO from the power-up hold
        write_volatile(PM5CTL0, read_volatile(PM5CTL0) & !LOCKLPM5);
        write_volatile(P1DIR, read_volatile(P1DIR) | BIT0); // P1.0 = output
    }

    loop {
        unsafe { write_volatile(P1OUT, read_volatile(P1OUT) ^ BIT0) }; // toggle LED1
        // crude delay (~0.2 s at the 1 MHz default MCLK); asm::nop keeps it from
        // being optimized away. Use a u16 counter — i32 would be two words on MSP430.
        for _ in 0u16..50_000 {
            asm::nop();
        }
    }
}

// debug builds emit calls to abort(); MSP430 has no meaningful abort.
#[no_mangle]
extern "C" fn abort() -> ! {
    panic!();
}
