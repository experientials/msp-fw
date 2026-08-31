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
const UCRXIFG0: u16 = 0x0001;

/// Poll bound. One I2C byte @100 kHz is ~90 µs (~a few hundred loop iterations at 1 MHz),
/// so this is generous but still bails in well under a millisecond of "stuck".
const SPIN: u16 = 4000;

const SDA_BIT: u8 = 0x04; // P1.2 / UCB0SDA
const SCL_BIT: u8 = 0x08; // P1.3 / UCB0SCL

fn bitdelay() {
    // ~half an I2C bit at a slow ~16 kHz — plenty to clock a stuck slave through a byte.
    for _ in 0..30u16 {
        msp430::asm::nop();
    }
}

fn sda_high(p: &Peripherals) -> bool {
    // P1IN reflects the real pad level even when the pin is muxed to eUSCI.
    p.p1.p1in().read().bits() & SDA_BIT != 0
}

/// Recover a wedged bus: if a slave is holding SDA low, temporarily bit-bang P1.2/P1.3 as
/// GPIO — pulse SCL up to 9 times (until SDA releases), then issue a manual STOP — and hand
/// the pins back to eUSCI. No-op if SDA is already high. Returns true if SDA ends high.
/// A `false` return means SDA is still stuck → hardware short, not a wedged slave.
pub fn recover(p: &Peripherals) -> bool {
    if sda_high(p) {
        return true;
    }
    // P1.2/P1.3 -> GPIO. SDA input (release + read), SCL output driven high.
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() & !(SDA_BIT | SCL_BIT)) });
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) });
    p.p1.p1dir().modify(|r, w| unsafe { w.bits((r.bits() & !SDA_BIT) | SCL_BIT) });

    let mut i = 0u8;
    while i < 9 && !sda_high(p) {
        p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() & !SCL_BIT) }); // SCL low
        bitdelay();
        p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) }); // SCL high
        bitdelay();
        i += 1;
    }

    // Manual STOP: with SCL high, drive SDA low then release it (rising edge = STOP).
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() & !SDA_BIT) });
    p.p1.p1dir().modify(|r, w| unsafe { w.bits(r.bits() | SDA_BIT) }); // SDA driven low
    bitdelay();
    p.p1.p1out().modify(|r, w| unsafe { w.bits(r.bits() | SCL_BIT) }); // SCL high
    bitdelay();
    p.p1.p1dir().modify(|r, w| unsafe { w.bits(r.bits() & !SDA_BIT) }); // release SDA -> rises
    bitdelay();

    let freed = sda_high(p);
    // Best practice: freeing the LINES above isn't enough — the eUSCI master can be left
    // mid-transaction (a STOP that never completed), which is why a finished transfer could
    // leave the bus low and force recovery every cycle. Resync the master's state machine with a
    // UCSWRST toggle (UCMODE/UCMST/UCSSEL/UCBRW are all retained). Skip it when init() is driving
    // us while already holding the peripheral in reset — its own sequence clears UCSWRST later.
    if ctlw0(p) & UCSWRST == 0 {
        p.e_usci_b0.ucb0ctlw0().modify(|r, w| unsafe { w.bits(r.bits() | UCSWRST) });
        p.e_usci_b0.ucb0ctlw0().modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
    }
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | SDA_BIT | SCL_BIT) }); // back to eUSCI
    freed
}

pub fn init(p: &Peripherals) {
    p.e_usci_b0.ucb0ctlw0().write(|w| unsafe { w.bits(UCSWRST) }); // hold in reset (releases pins)
    recover(p); // free a slave holding SDA low before we try to master the bus
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCMODE_3 | UCMST | UCSYNC | UCSSEL_SMCLK) });
    p.e_usci_b0.ucb0brw().write(|w| unsafe { w.bits(10) }); // 1 MHz / 10 = 100 kHz
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) });
}

/// Change the I²C bit clock: SCL = SMCLK (1 MHz) / `brw`. eUSCI requires UCSWRST to alter the baud
/// divider, so we toggle it; mode/master/clock-source are retained. Used by the stress margin sweep
/// to push SCL from 100 kHz (brw=10) up to 1 MHz (brw=1).
#[cfg(feature = "stress")]
pub fn set_brw(p: &Peripherals, brw: u16) {
    p.e_usci_b0
        .ucb0ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCSWRST) });
    p.e_usci_b0.ucb0brw().write(|w| unsafe { w.bits(brw) });
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
    // Self-heal: a previous probe can leave a device holding SDA low (e.g. an OLED left
    // mid-transaction by an address-only write). Free it before probing the next address,
    // otherwise every subsequent probe reads the stuck-low SDA as a false ACK.
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
    // Send the address (UCTXSTT auto-clears once it's out), then STOP, then read UCNACKIFG.
    // Order matters: UCNACKIFG is only reliable AFTER the STOP completes. Reading it earlier
    // (right when UCTXSTT clears, or on UCTXIFG0) races the ACK-bit sample and false-ACKs
    // every empty address. This is TI's recommended eUSCI_B presence-probe order.
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

/// Write `data` to `addr` (caller includes any register pointer as data[0]).
/// Returns `false` on NACK or timeout; never hangs.
pub fn write(p: &Peripherals, addr: u8, data: &[u8]) -> bool {
    // Self-heal like probe()/read_reg(): a preceding probe can leave a device holding SDA low,
    // and you can't generate a valid START on a low SDA — so free the bus first or every write
    // fails at the address phase and wedges the bus (the bug that blanked both displays).
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

/// Set the register pointer at `addr`, repeated-START, and read `buf.len()` bytes into `buf`.
/// Bounded (never hangs); returns false on NACK or timeout. Used for WHO_AM_I / status reads.
pub fn read_reg(p: &Peripherals, addr: u8, reg: u8, buf: &mut [u8]) -> bool {
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
        n += 1;
        if n >= SPIN {
            break;
        }
    }
    true
}
