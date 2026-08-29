#![no_main]
#![no_std]
#![feature(asm_experimental_arch)] // core::arch::asm! on msp430 (SCG0 in SR)

//! i2c-is31 — drive the IS31FL3730 8×8 LED matrix over I²C, with a UART console
//! and **timeout-guarded** I²C so a bus fault reports instead of freezing.
//!
//! Wiring (bob-929 board/connections.toml): SDA=P1.2, SCL=P1.3 → UCB0, IS31 @ 0x61.
//! UART TX=P1.4 → eZ-FET backchannel → /dev/cu.usbmodem* @ 9600 8N1. SDB=P2.5.
//! Raw registers (no PAC); addresses from `nm`, bits from the TI header.

extern crate panic_msp430;

use core::ptr::{read_volatile as rd, write_volatile as wr};
use msp430::asm;
use msp430_rt::entry;

// --- core / GPIO ---
const WDTCTL: *mut u16 = 0x01CC as *mut u16;
const PM5CTL0: *mut u16 = 0x0130 as *mut u16;
const P1SEL0: *mut u8 = 0x020A as *mut u8;
const P2DIR: *mut u8 = 0x0205 as *mut u8;
const P2OUT: *mut u8 = 0x0203 as *mut u8;
const P2SEL0: *mut u8 = 0x020B as *mut u8;

// --- clock system (CS) ---
const CSCTL1: *mut u16 = 0x0182 as *mut u16;
const CSCTL2: *mut u16 = 0x0184 as *mut u16;
const CSCTL3: *mut u16 = 0x0186 as *mut u16;
const CSCTL4: *mut u16 = 0x0188 as *mut u16;
const CSCTL7: *mut u16 = 0x018E as *mut u16;

// --- eUSCI_B0 (I²C) ---
const UCB0CTLW0: *mut u16 = 0x0540 as *mut u16;
const UCB0BRW: *mut u16 = 0x0546 as *mut u16;
const UCB0I2CSA: *mut u16 = 0x0560 as *mut u16;
const UCB0IFG: *mut u16 = 0x056C as *mut u16;
const UCB0TXBUF: *mut u8 = 0x054E as *mut u8;

// --- eUSCI_A0 (UART) ---
const UCA0CTLW0: *mut u16 = 0x0500 as *mut u16;
const UCA0BRW: *mut u16 = 0x0506 as *mut u16;
const UCA0MCTLW: *mut u16 = 0x0508 as *mut u16;
const UCA0TXBUF: *mut u8 = 0x050E as *mut u8;
const UCA0IFG: *mut u16 = 0x051C as *mut u16;

// --- bits ---
const WDTPW_HOLD: u16 = 0x5A00 | 0x0080;
const LOCKLPM5: u16 = 0x0001;
const BIT2: u8 = 0x04;
const BIT3: u8 = 0x08;
const BIT4: u8 = 0x10;
const BIT5: u8 = 0x20;
const UCSWRST: u16 = 0x0001;
const UCSSEL_SMCLK: u16 = 0x0080;
const UCSYNC: u16 = 0x0100;
const UCMODE_3: u16 = 0x0600;
const UCMST: u16 = 0x0800;
const UCTXSTT: u16 = 0x0002;
const UCTXSTP: u16 = 0x0004;
const UCTR: u16 = 0x0010;
const UCTXIFG0: u16 = 0x0002; // eUSCI_B TX flag
const UCNACKIFG: u16 = 0x0020;
const UCOS16: u16 = 0x0001;
const UCTXIFG: u16 = 0x0002; // eUSCI_A TX flag
const SELREF_REFOCLK: u16 = 0x0010;
const DCOFTRIMEN: u16 = 0x0080;
const DCOFTRIM0: u16 = 0x0010;
const DCOFTRIM1: u16 = 0x0020;
const SELA_REFOCLK: u16 = 0x0100;
const FLLUNLOCK: u16 = 0x0300;

// I²C transaction result
const R_ACK: u8 = 0;
const R_NACK: u8 = 1;
const R_TIMEOUT: u8 = 2;
const SPIN: u16 = 4000; // ~poll bound; one I²C byte @100 kHz is well under this

// --- IS31FL3730 ---
const IS31_ADDR: u16 = 0x61;
const REG_CONFIG: u8 = 0x00;
const REG_DATA: u8 = 0x01;
const REG_UPDATE: u8 = 0x0C;
const REG_PWM: u8 = 0x19;
const CONFIG_8X8: u8 = 0x00;
const PWM_DEFAULT: u8 = 0x10;

// ---------- clock: MCLK = SMCLK = precise 1 MHz (DCO+FLL, REFO ref) ----------
fn clock_init_1mhz() {
    unsafe {
        core::arch::asm!("bis #0x40, r2", options(nomem, nostack)); // SCG0=1: FLL off
        wr(CSCTL3, rd(CSCTL3) | SELREF_REFOCLK);
        wr(CSCTL1, DCOFTRIMEN | DCOFTRIM0 | DCOFTRIM1); // DCORSEL_0 = 0 → 1 MHz
        wr(CSCTL2, 30); // FLLD=1, N=30 → 1 MHz
        asm::nop();
        asm::nop();
        asm::nop();
        core::arch::asm!("bic #0x40, r2", options(nomem, nostack)); // SCG0=0: FLL on
        while (rd(CSCTL7) & FLLUNLOCK) != 0 {} // wait for lock
        wr(CSCTL4, SELA_REFOCLK); // MCLK/SMCLK = DCODIV, ACLK = REFO
    }
    // let the DCO fully settle before we clock the UART (avoids garbled first bytes)
    for _ in 0u16..8000 {
        asm::nop();
    }
}

// ---------- UART console (eUSCI_A0, 9600 8N1) ----------
fn uart_init() {
    unsafe {
        wr(P1SEL0, rd(P1SEL0) | BIT4 | BIT5);
        wr(UCA0CTLW0, UCSWRST);
        wr(UCA0CTLW0, UCSWRST | UCSSEL_SMCLK);
        wr(UCA0BRW, 6);
        wr(UCA0MCTLW, 0x2000 | (8 << 4) | UCOS16);
        wr(UCA0CTLW0, rd(UCA0CTLW0) & !UCSWRST);
    }
}
fn uart_putc(c: u8) {
    unsafe {
        while (rd(UCA0IFG) & UCTXIFG) == 0 {}
        wr(UCA0TXBUF, c);
    }
}
fn uart_puts(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}
fn uart_hex8(v: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    uart_putc(HEX[(v >> 4) as usize & 0xF]);
    uart_putc(HEX[v as usize & 0xF]);
}
fn uart_result(r: u8) {
    uart_puts(match r {
        R_ACK => "ACK",
        R_NACK => "NACK",
        _ => "TIMEOUT",
    });
}

// ---------- I²C (eUSCI_B0, 100 kHz master) — timeout-guarded ----------
fn i2c_init() {
    unsafe {
        wr(P1SEL0, rd(P1SEL0) | BIT2 | BIT3);
        wr(UCB0CTLW0, UCSWRST);
        wr(UCB0CTLW0, UCSWRST | UCMODE_3 | UCMST | UCSYNC | UCSSEL_SMCLK);
        wr(UCB0BRW, 10);
        wr(UCB0CTLW0, rd(UCB0CTLW0) & !UCSWRST);
    }
}
/// Best-effort STOP with its own bound so a stuck bus can't freeze us.
fn i2c_stop() {
    unsafe {
        wr(UCB0CTLW0, rd(UCB0CTLW0) | UCTXSTP);
        let mut n = 0u16;
        while (rd(UCB0CTLW0) & UCTXSTP) != 0 {
            n += 1;
            if n >= SPIN {
                break;
            }
        }
    }
}
/// Write `data` to `addr`. Returns R_ACK / R_NACK / R_TIMEOUT. Never hangs.
fn i2c_write(addr: u16, data: &[u8]) -> u8 {
    unsafe {
        wr(UCB0I2CSA, addr);
        wr(UCB0IFG, rd(UCB0IFG) & !(UCNACKIFG | UCTXIFG0));
        wr(UCB0CTLW0, rd(UCB0CTLW0) | UCTR | UCTXSTT);
        for &b in data {
            let mut n = 0u16;
            while (rd(UCB0IFG) & (UCTXIFG0 | UCNACKIFG)) == 0 {
                n += 1;
                if n >= SPIN {
                    i2c_stop();
                    return R_TIMEOUT;
                }
            }
            if (rd(UCB0IFG) & UCNACKIFG) != 0 {
                i2c_stop();
                wr(UCB0IFG, rd(UCB0IFG) & !UCNACKIFG);
                return R_NACK;
            }
            wr(UCB0TXBUF, b);
        }
        let mut n = 0u16;
        while (rd(UCB0IFG) & UCTXIFG0) == 0 {
            n += 1;
            if n >= SPIN {
                i2c_stop();
                return R_TIMEOUT;
            }
        }
    }
    i2c_stop();
    R_ACK
}
/// Address-only probe. R_ACK / R_NACK / R_TIMEOUT.
fn i2c_probe(addr: u16) -> u8 {
    unsafe {
        wr(UCB0I2CSA, addr);
        wr(UCB0IFG, rd(UCB0IFG) & !(UCNACKIFG | UCTXIFG0));
        wr(UCB0CTLW0, rd(UCB0CTLW0) | UCTR | UCTXSTT);
        let mut n = 0u16;
        while (rd(UCB0CTLW0) & UCTXSTT) != 0 {
            n += 1;
            if n >= SPIN {
                i2c_stop();
                return R_TIMEOUT;
            }
        }
        let acked = (rd(UCB0IFG) & UCNACKIFG) == 0;
        i2c_stop();
        wr(UCB0IFG, rd(UCB0IFG) & !UCNACKIFG);
        if acked {
            R_ACK
        } else {
            R_NACK
        }
    }
}
/// Scan 0x08..0x77; print addresses that ACK, and warn on any timeout.
fn i2c_scan() {
    uart_puts("[scan] ");
    let mut found = 0u8;
    let mut timeouts = 0u8;
    for a in 0x08u16..0x78 {
        match i2c_probe(a) {
            R_ACK => {
                uart_puts("0x");
                uart_hex8(a as u8);
                uart_putc(b' ');
                found += 1;
            }
            R_TIMEOUT => timeouts += 1,
            _ => {}
        }
    }
    if found == 0 {
        uart_puts("none");
    }
    if timeouts > 0 {
        uart_puts("(+timeouts: bus stuck low)");
    }
    uart_puts("\n");
}

// ---------- IS31FL3730 ----------
fn is31_init() -> u8 {
    let a = i2c_write(IS31_ADDR, &[REG_CONFIG, CONFIG_8X8]);
    if a != R_ACK {
        return a;
    }
    i2c_write(IS31_ADDR, &[REG_PWM, PWM_DEFAULT])
}
fn is31_show(frame: &[u8; 8]) -> u8 {
    let mut buf = [0u8; 9];
    buf[0] = REG_DATA;
    buf[1..9].copy_from_slice(frame);
    let a = i2c_write(IS31_ADDR, &buf);
    if a != R_ACK {
        return a;
    }
    i2c_write(IS31_ADDR, &[REG_UPDATE, 0x00])
}

/// Drive SDB (P2.5) HIGH to enable the IS31 (active-low shutdown).
fn sdb_enable() {
    unsafe {
        wr(P2SEL0, rd(P2SEL0) & !BIT5);
        wr(P2OUT, rd(P2OUT) | BIT5);
        wr(P2DIR, rd(P2DIR) | BIT5);
    }
}

fn delay() {
    for _ in 0u16..50_000 {
        asm::nop();
    }
}

#[entry]
fn main() -> ! {
    unsafe {
        wr(WDTCTL, WDTPW_HOLD);
        wr(PM5CTL0, rd(PM5CTL0) & !LOCKLPM5);
    }
    clock_init_1mhz();
    uart_init();
    uart_puts("\n\n[i2c-is31] boot\n");

    sdb_enable();
    uart_puts("[sdb] P2.5 HIGH\n");

    i2c_init();
    i2c_scan();

    uart_puts("[is31] init ");
    uart_result(is31_init());
    uart_puts("\n");

    let mut row: usize = 0;
    loop {
        let mut frame = [0u8; 8];
        frame[row] = 0xFF;
        uart_puts("[row ");
        uart_hex8(row as u8);
        uart_puts("] ");
        uart_result(is31_show(&frame));
        uart_puts("\n");
        row = (row + 1) % 8;
        delay();
    }
}

#[no_mangle]
extern "C" fn abort() -> ! {
    panic!();
}
