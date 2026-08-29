//! One-shot boot dump of everything that decides whether the peripherals are wired up
//! the way we think. The philosophy here is "when in doubt, dump more": a diag ROM that
//! prints the actual register file is worth far more than source we *believe* configures
//! the chip. Every value is the live SFR read back after init, not a constant.

use crate::uart;
use msp430fr2476::Peripherals;

/// `NAME=XXXX` on its own line — the workhorse for a raw 16-bit register.
fn row(p: &Peripherals, name: &str, val: u16) {
    uart::puts(p, name);
    uart::putc(p, b'=');
    uart::hex16(p, val);
    uart::putc(p, b'\n');
}

/// Decode one pin's function-select pair (PxSEL1:PxSEL0) to its 2-bit value and meaning.
/// `bit` is the pin's bit mask within the port. 01 = primary module function.
fn pin(p: &Peripherals, name: &str, sel1: u16, sel0: u16, dir: u16, bit: u16, want: &str) {
    let code = (((sel1 & bit) != 0) as u8) << 1 | ((sel0 & bit) != 0) as u8;
    uart::puts(p, "  ");
    uart::puts(p, name);
    uart::puts(p, " sel=");
    uart::putc(p, b'0' + (code >> 1));
    uart::putc(p, b'0' + (code & 1));
    uart::puts(p, if code == 1 { " module" } else { " GPIO/alt" });
    uart::puts(p, " dir=");
    uart::putc(p, if dir & bit != 0 { b'O' } else { b'I' });
    uart::puts(p, "  want=");
    uart::puts(p, want);
    uart::putc(p, b'\n');
}

/// Dump reset cause, clock system, port mux/direction, and both eUSCI blocks.
/// Call once after all init has run; the values are the ground truth to reason from.
pub fn dump(p: &Peripherals) {
    uart::puts(p, "\n--- register dump (live SFRs) ---\n");

    // Why did we start? A brown-out/PUC vs a clean POR changes what state to trust.
    row(p, "SYSRSTIV", p.sys.sysrstiv().read().bits());

    // Clock: CSCTL7 carries the FLL (un)lock + fault flags. Nonzero unlock bits = bad UART/I2C timing.
    uart::puts(p, "[clock]\n");
    row(p, " CSCTL1", p.cs.csctl1().read().bits());
    row(p, " CSCTL2", p.cs.csctl2().read().bits());
    row(p, " CSCTL3", p.cs.csctl3().read().bits());
    row(p, " CSCTL4", p.cs.csctl4().read().bits());
    row(p, " CSCTL5", p.cs.csctl5().read().bits());
    row(p, " CSCTL6", p.cs.csctl6().read().bits());
    row(p, " CSCTL7", p.cs.csctl7().read().bits());

    // Ports: raw registers first, then the decoded verdict for the pins we depend on.
    // Port SFRs are byte-wide (u8); widen to u16 for the shared helpers.
    let p1s1 = u16::from(p.p1.p1sel1().read().bits());
    let p1s0 = u16::from(p.p1.p1sel0().read().bits());
    let p1dir = u16::from(p.p1.p1dir().read().bits());
    uart::puts(p, "[port1]\n");
    row(p, " P1SEL1", p1s1);
    row(p, " P1SEL0", p1s0);
    row(p, " P1DIR ", p1dir);
    row(p, " P1OUT ", u16::from(p.p1.p1out().read().bits()));
    row(p, " P1REN ", u16::from(p.p1.p1ren().read().bits()));
    row(p, " P1IN  ", u16::from(p.p1.p1in().read().bits()));
    pin(p, "P1.2", p1s1, p1s0, p1dir, 0x04, "UCB0SDA (01)");
    pin(p, "P1.3", p1s1, p1s0, p1dir, 0x08, "UCB0SCL (01)");
    pin(p, "P1.4", p1s1, p1s0, p1dir, 0x10, "UCA0TXD (01)");
    pin(p, "P1.5", p1s1, p1s0, p1dir, 0x20, "UCA0RXD (01)");

    let p2s1 = u16::from(p.p2.p2sel1().read().bits());
    let p2s0 = u16::from(p.p2.p2sel0().read().bits());
    let p2dir = u16::from(p.p2.p2dir().read().bits());
    uart::puts(p, "[port2]\n");
    row(p, " P2SEL1", p2s1);
    row(p, " P2SEL0", p2s0);
    row(p, " P2DIR ", p2dir);
    row(p, " P2OUT ", u16::from(p.p2.p2out().read().bits()));
    row(p, " P2IN  ", u16::from(p.p2.p2in().read().bits()));
    pin(p, "P2.5", p2s1, p2s0, p2dir, 0x20, "SDB GPIO out hi (00,O)");

    // eUSCI_B0 (I2C): CTLW0 shows mode/master/clock; STATW busy/arb; IFG the NACK/TX flags.
    uart::puts(p, "[eUSCI_B0 I2C]\n");
    row(p, " UCB0CTLW0", p.e_usci_b0.ucb0ctlw0().read().bits());
    row(p, " UCB0BRW  ", p.e_usci_b0.ucb0brw().read().bits());
    row(p, " UCB0I2CSA", p.e_usci_b0.ucb0i2csa().read().bits());
    row(p, " UCB0STATW", p.e_usci_b0.ucb0statw().read().bits());
    row(p, " UCB0IFG  ", p.e_usci_b0.ucb0ifg().read().bits());

    // eUSCI_A0 (UART): confirms the console we're reading this on is set as intended.
    uart::puts(p, "[eUSCI_A0 UART]\n");
    row(p, " UCA0CTLW0", p.e_usci_a0.uca0ctlw0().read().bits());
    row(p, " UCA0BRW  ", p.e_usci_a0.uca0brw().read().bits());
    row(p, " UCA0MCTLW", p.e_usci_a0.uca0mctlw().read().bits());

    row(p, "PM5CTL0", p.pmm.pm5ctl0().read().bits());
    uart::puts(p, "--- end dump ---\n");
}
