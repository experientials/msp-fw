//! eUSCI_B0 I2C master, 100 kHz. Polled/blocking with bounded waits so a stuck bus
//! (SDA/SCL held low) reports failure instead of hanging the firmware. Pins routed in main.

use msp430fr2476::Peripherals;

const UCSWRST: u16 = 0x0001;
const UCMODE_3: u16 = 0x0600;
const UCMST: u16 = 0x0800;
const UCSYNC: u16 = 0x0100;
const UCSSEL_SMCLK: u16 = 0x0080;
const UCTR: u16 = 0x0010;
const UCTXSTT: u16 = 0x0002;
const UCTXSTP: u16 = 0x0004;
const UCNACKIFG: u16 = 0x0020;
const UCTXIFG0: u16 = 0x0002;

/// Poll bound. One I2C byte @100 kHz is ~90 µs (~a few hundred loop iterations at 1 MHz),
/// so this is generous but still bails in well under a millisecond of "stuck".
const SPIN: u16 = 4000;

pub fn init(p: &Peripherals) {
    p.e_usci_b0.ucb0ctlw0().write(|w| unsafe { w.bits(UCSWRST) });
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCMODE_3 | UCMST | UCSYNC | UCSSEL_SMCLK) });
    p.e_usci_b0.ucb0brw().write(|w| unsafe { w.bits(10) }); // 1 MHz / 10 = 100 kHz
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
}

fn ifg(p: &Peripherals) -> u16 {
    p.e_usci_b0.ucb0ifg().read().bits()
}

fn ctlw0(p: &Peripherals) -> u16 {
    p.e_usci_b0.ucb0ctlw0().read().bits()
}

/// Best-effort STOP, itself bounded so a wedged bus can't hang here either.
fn stop(p: &Peripherals) {
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCTXSTP) });
    let mut n = 0u16;
    while ctlw0(p) & UCTXSTP != 0 {
        n += 1;
        if n >= SPIN {
            break;
        }
    }
}

/// Address-only probe: START + addr(W) + STOP. `true` if the device ACKs.
/// A stuck bus (address never clears) returns `false` rather than hanging.
pub fn probe(p: &Peripherals, addr: u8) -> bool {
    p.e_usci_b0.ucb0i2csa().write(|w| unsafe { w.bits(addr as u16) });
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(UCNACKIFG | UCTXIFG0)) });
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCTR | UCTXSTT) });
    let mut n = 0u16;
    while ctlw0(p) & UCTXSTT != 0 {
        n += 1;
        if n >= SPIN {
            stop(p);
            return false;
        }
    }
    let acked = ifg(p) & UCNACKIFG == 0;
    stop(p);
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !UCNACKIFG) });
    acked
}

/// Write `data` to `addr` (caller includes any register pointer as data[0]).
/// Returns `false` on NACK or timeout; never hangs.
pub fn write(p: &Peripherals, addr: u8, data: &[u8]) -> bool {
    p.e_usci_b0.ucb0i2csa().write(|w| unsafe { w.bits(addr as u16) });
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(UCNACKIFG | UCTXIFG0)) });
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCTR | UCTXSTT) });

    for &byte in data {
        let mut n = 0u16;
        while ifg(p) & (UCTXIFG0 | UCNACKIFG) == 0 {
            n += 1;
            if n >= SPIN {
                stop(p);
                return false;
            }
        }
        if ifg(p) & UCNACKIFG != 0 {
            stop(p);
            p.e_usci_b0
                .ucb0ifg()
                .modify(|r, w| unsafe { w.bits(r.bits() & !UCNACKIFG) });
            return false;
        }
        p.e_usci_b0.ucb0txbuf().write(|w| unsafe { w.bits(byte as u16) });
    }
    let mut n = 0u16;
    while ifg(p) & UCTXIFG0 == 0 {
        n += 1;
        if n >= SPIN {
            stop(p);
            return false;
        }
    }
    stop(p);
    true
}
