//! eUSCI_B0 I2C master, 100 kHz — the bus prod masters to enumerate/read sensors. Polled/blocking
//! with bounded waits so a stuck bus reports failure instead of hanging. Ported from diag's proven
//! i2c.rs (same FR2476 eUSCI_B). Pins routed in main (P1.2/P1.3 → UCB0).
//!
//! NOTE: on the FR247x this is the *sensor-master* bus. The MCU-bus I2C SLAVE surface (eUSCI_B1,
//! answering the SoM) is a separate driver — not this file.

use crate::pac::Peripherals;

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
const UCRXIFG0: u16 = 0x0001;

/// Poll bound. One I2C byte @100 kHz is ~90 µs; this bails in well under a ms of "stuck".
const SPIN: u16 = 4000;

const SDA_BIT: u8 = 0x04; // P1.2 / UCB0SDA
const SCL_BIT: u8 = 0x08; // P1.3 / UCB0SCL

fn bitdelay() {
    for _ in 0..30u16 {
        msp430::asm::nop();
    }
}

fn sda_high(p: &Peripherals) -> bool {
    p.p1.p1in().read().bits() & SDA_BIT != 0
}

/// Idle levels of the bus pads (P1IN reflects the real pad even when muxed to eUSCI): `(sda, scl)`
/// high?. A low SDA at idle = a wedged bus (a slave holding it) → every probe false-ACKs.
pub fn bus_levels(p: &Peripherals) -> (bool, bool) {
    let lv = p.p1.p1in().read().bits();
    (lv & SDA_BIT != 0, lv & SCL_BIT != 0)
}

/// Recover a wedged bus: bit-bang SCL up to 9 pulses until SDA releases, issue a manual STOP, hand
/// the pins back to eUSCI. No-op if SDA already high. Returns true if SDA ends high.
pub fn recover(p: &Peripherals) -> bool {
    if sda_high(p) {
        return true;
    }
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() & !(SDA_BIT | SCL_BIT)) });
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) });
    p.p1.p1dir().modify(|r, w| unsafe { w.bits((r.bits() & !SDA_BIT) | SCL_BIT) });

    let mut i = 0u8;
    while i < 9 && !sda_high(p) {
        p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() & !SCL_BIT) });
        bitdelay();
        p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) });
        bitdelay();
        i += 1;
    }
    // Manual STOP: with SCL high, drive SDA low then release (rising edge = STOP).
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() & !SDA_BIT) });
    p.p1.p1dir().modify(|r, w| unsafe { w.bits(r.bits() | SDA_BIT) });
    bitdelay();
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) });
    bitdelay();
    p.p1.p1dir().modify(|r, w| unsafe { w.bits(r.bits() & !SDA_BIT) });
    bitdelay();

    let freed = sda_high(p);
    // Resync the master state machine (UCSWRST toggle) unless init() already holds it in reset.
    if ctlw0(p) & UCSWRST == 0 {
        p.e_usci_b0.ucb0ctlw0().modify(|r, w| unsafe { w.bits(r.bits() | UCSWRST) });
        p.e_usci_b0.ucb0ctlw0().modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
    }
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | SDA_BIT | SCL_BIT) });
    freed
}

pub fn init(p: &Peripherals) {
    p.e_usci_b0.ucb0ctlw0().write(|w| unsafe { w.bits(UCSWRST) }); // hold in reset (releases pins)
    recover(p); // free a slave holding SDA low before we master the bus
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

/// Address-only probe: START + addr(W) + STOP. `true` if the device ACKs. A stuck bus returns
/// `false` rather than hanging. (TI's recommended eUSCI_B presence-probe order: read UCNACKIFG only
/// AFTER the STOP completes.)
pub fn probe(p: &Peripherals, addr: u8) -> bool {
    if !sda_high(p) {
        recover(p);
    }
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
            break;
        }
    }
    stop(p);
    let acked = ifg(p) & UCNACKIFG == 0;
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(UCNACKIFG | UCTXIFG0)) });
    acked
}

/// Write `data` to `addr` (caller includes any register pointer as data[0]). `false` on NACK or
/// timeout; never hangs. An empty `data` is an address-only presence probe (see hal.rs).
pub fn write(p: &Peripherals, addr: u8, data: &[u8]) -> bool {
    if data.is_empty() {
        return probe(p, addr);
    }
    if !sda_high(p) {
        recover(p);
    }
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

/// Set the register pointer at `addr` (transmitter, no STOP), repeated-START as receiver, and read
/// `buf.len()` bytes into `buf`. `false` on NACK/timeout; never hangs. Ported verbatim from diag's
/// proven `i2c::read_reg` (same FR2476 eUSCI_B) — the register-read primitive the shared device
/// drivers use via `hal::EusciI2c`'s `write_read`.
pub fn read_reg(p: &Peripherals, addr: u8, reg: u8, buf: &mut [u8]) -> bool {
    read_reg_pumped(p, addr, reg, buf, || {})
}

/// Like [`read_reg`], but calls `pump` at the top of every spin-wait. Used by the B0↔B1 loopback
/// self-test (`stem::loopback_selftest`) to service the eUSCI_B1 SLAVE (`stem::poll`) while this
/// master reads it on the SAME MCU — without the pump the slave's clock-stretch deadlocks the blocking
/// master. Normal callers use [`read_reg`] (`pump = || {}`, which the optimiser drops). Behaviourally
/// identical to the ported diag `i2c::read_reg` apart from the injected pump.
pub fn read_reg_pumped(
    p: &Peripherals,
    addr: u8,
    reg: u8,
    buf: &mut [u8],
    mut pump: impl FnMut(),
) -> bool {
    if buf.is_empty() {
        return false;
    }
    if !sda_high(p) {
        recover(p);
    }
    // Phase 1: write the register pointer (transmitter), no STOP.
    p.e_usci_b0.ucb0i2csa().write(|w| unsafe { w.bits(addr as u16) });
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(UCNACKIFG | UCTXIFG0)) });
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCTR | UCTXSTT) });
    let mut n = 0u16;
    while ifg(p) & (UCTXIFG0 | UCNACKIFG) == 0 {
        pump();
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
    p.e_usci_b0.ucb0txbuf().write(|w| unsafe { w.bits(reg as u16) });
    n = 0;
    while ifg(p) & UCTXIFG0 == 0 {
        pump();
        n += 1;
        if n >= SPIN {
            stop(p);
            return false;
        }
    }
    // Phase 2: repeated START as receiver.
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits((r.bits() & !UCTR) | UCTXSTT) });
    let len = buf.len();
    for i in 0..len {
        if i == len - 1 {
            // last byte: wait for the repeated-START/address to go out, then arm NACK+STOP.
            n = 0;
            while ctlw0(p) & UCTXSTT != 0 {
                pump();
                n += 1;
                if n >= SPIN {
                    stop(p);
                    return false;
                }
            }
            p.e_usci_b0
                .ucb0ctlw0()
                .modify(|r, w| unsafe { w.bits(r.bits() | UCTXSTP) });
        }
        n = 0;
        while ifg(p) & UCRXIFG0 == 0 {
            pump();
            n += 1;
            if n >= SPIN {
                stop(p);
                return false;
            }
        }
        buf[i] = p.e_usci_b0.ucb0rxbuf().read().bits() as u8;
    }
    n = 0;
    while ctlw0(p) & UCTXSTP != 0 {
        pump();
        n += 1;
        if n >= SPIN {
            break;
        }
    }
    true
}

/// Pointer-less receiver read: START(R) + read `buf.len()` bytes + STOP. No register write first —
/// for devices that return a result to a bare read (e.g. the Si7021 no-hold measurement result and
/// its ID/firmware-rev sequences). Ported verbatim from diag's proven `i2c::read`. Bounded/self-
/// healing like `read_reg`'s receive phase: never hangs; returns false on NACK (device busy/absent)
/// or timeout — which is exactly the signal a no-hold poll needs ("not ready yet").
pub fn read(p: &Peripherals, addr: u8, buf: &mut [u8]) -> bool {
    if buf.is_empty() {
        return false;
    }
    if !sda_high(p) {
        recover(p);
    }
    p.e_usci_b0.ucb0i2csa().write(|w| unsafe { w.bits(addr as u16) });
    p.e_usci_b0
        .ucb0ifg()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(UCNACKIFG | UCRXIFG0)) });
    // Receiver (UCTR=0) + START.
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits((r.bits() & !UCTR) | UCTXSTT) });
    let len = buf.len();
    for i in 0..len {
        if i == len - 1 {
            // Last byte (also the only byte when len==1): wait for the address to clear, then arm
            // NACK+STOP so the eUSCI NACKs the final byte and releases the bus.
            let mut n = 0u16;
            while ctlw0(p) & UCTXSTT != 0 {
                n += 1;
                if n >= SPIN {
                    stop(p);
                    return false;
                }
            }
            p.e_usci_b0
                .ucb0ctlw0()
                .modify(|r, w| unsafe { w.bits(r.bits() | UCTXSTP) });
        }
        let mut n = 0u16;
        while ifg(p) & UCRXIFG0 == 0 {
            n += 1;
            if n >= SPIN {
                stop(p);
                return false;
            }
        }
        buf[i] = p.e_usci_b0.ucb0rxbuf().read().bits() as u8;
    }
    let mut n = 0u16;
    while ctlw0(p) & UCTXSTP != 0 {
        n += 1;
        if n >= SPIN {
            break;
        }
    }
    true
}
