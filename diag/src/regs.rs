//! One-shot boot dump of everything that decides whether the peripherals are wired up
//! the way we think. The philosophy here is "when in doubt, dump more": a diag ROM that
//! prints the actual register file is worth far more than source we *believe* configures
//! the chip. Every value is the live SFR read back after init, not a constant.

use crate::uart;
use msp430fr2476::Peripherals;

/// Decode SYSRSTIV (reset-cause vector) to text — values verified against the FR2476 datasheet
/// (Table 9-x, SYSRSTIV). This is the MCU-self reset-cause check: it turns a mystery reboot into
/// a named cause (e.g. `WDT timeout` = the stress-sweep overrun we hit).
fn reset_cause(v: u16) -> &'static str {
    match v {
        0x00 => "none",
        0x02 => "SVS low-power entry",
        0x04 => "FRAM uncorrectable bit error",
        0x0A => "security violation",
        0x0E => "SVSH brown-out",
        0x14 => "software POR",
        0x16 => "WDT timeout",
        0x18 => "WDT password violation",
        0x20 => "PMM password violation",
        0x24 => "FLL unlock",
        _ => "reserved/other",
    }
}

// --- Device descriptor (TLV) --------------------------------------------------------------------
// Read-only info memory at fixed addresses (FR2476 datasheet Table 9-29/9-30). A volatile read here
// identifies the *actual silicon* and its per-unit revisions/serial, so boot reports the chip it is
// running on rather than a compile-time assumption ("base it on hardware, don't hardcode").
const TLV_DEVICE_ID: *const u16 = 0x1A04 as *const u16; // LE word: 0x1A04 lo, 0x1A05 hi (e.g. 832Ah)
const TLV_HW_REV: *const u8 = 0x1A06 as *const u8;
const TLV_FW_REV: *const u8 = 0x1A07 as *const u8;
const TLV_DIE_LOT: *const u32 = 0x1A0A as *const u32; // die record: lot/wafer id = per-unit serial

/// Map a TLV device ID to a chip name, or `None` if unrecognized (caller prints the raw hex, still
/// honest hardware truth). Extend this table as we validate more parts (FR2433 etc.) on silicon.
pub fn chip_name(id: u16) -> Option<&'static str> {
    match id {
        0x832A => Some("MSP430FR2476"),
        0x832B => Some("MSP430FR2475"),
        _ => None,
    }
}

/// Basic board stats gathered from hardware at boot: which chip, its revisions, and a per-unit die
/// serial. Cheap, read-only info-memory reads — no peripheral bring-up. (Measured stats — Vcc and
/// die temperature via the ADC — are the next rung, gated on the rail-ADC work; noted in
/// DIAGNOSTICS.md.) Reset cause + clock-lock live in `dump` (SYSRSTIV must be read exactly once).
pub fn stats(p: &Peripherals) {
    let id = unsafe { core::ptr::read_volatile(TLV_DEVICE_ID) };
    let lot = unsafe { core::ptr::read_volatile(TLV_DIE_LOT) };
    uart::puts(p, "\n[board]\n chip: ");
    match chip_name(id) {
        Some(n) => uart::puts(p, n),
        None => uart::puts(p, "MSP430 (unrecognized)"),
    }
    uart::puts(p, " id=");
    uart::hex16(p, id);
    uart::puts(p, "\n hw rev=");
    uart::hex8(p, unsafe { core::ptr::read_volatile(TLV_HW_REV) });
    uart::puts(p, " fw rev=");
    uart::hex8(p, unsafe { core::ptr::read_volatile(TLV_FW_REV) });
    uart::puts(p, "\n die id=");
    uart::hex16(p, (lot >> 16) as u16);
    uart::hex16(p, lot as u16);
    uart::puts(p, "\n ");
    vcc_temp(p);
    uart::putc(p, b'\n');
}

/// Print `Vcc=X.XXV  temp=NC` — the ADC-measured rail + die temperature, sampled *now* (internal
/// channels, no external wiring). Approximate but high-signal. Shared by the boot `[board]` block
/// and the periodic health telemetry, so these live numbers stay visible even when the one-shot
/// boot output is missed (e.g. `just monitor` attaching mid-dump) — and because rail sag / thermal
/// drift are exactly the trends a supervisor should watch over time, not just at boot.
pub fn vcc_temp(p: &Peripherals) {
    let mv = crate::adc::dvcc_mv(p);
    uart::puts(p, "Vcc=");
    uart::dec(p, mv / 1000);
    uart::putc(p, b'.');
    let cv = (mv % 1000) / 10; // centivolts, two digits
    if cv < 10 {
        uart::putc(p, b'0');
    }
    uart::dec(p, cv);
    uart::puts(p, "V  temp=");
    let t = crate::adc::die_temp_c(p);
    if t < 0 {
        uart::putc(p, b'-');
        uart::dec(p, (-t) as u16);
    } else {
        uart::dec(p, t as u16);
    }
    uart::puts(p, "C");
}

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

    // Why did we start? A brown-out/PUC vs a clean POR changes what state to trust. Read once
    // (reading SYSRSTIV clears the reported flag), print raw, then decode the cause.
    let rst = p.sys.sysrstiv().read().bits();
    row(p, "SYSRSTIV", rst);
    uart::puts(p, " reset: ");
    uart::puts(p, reset_cause(rst));
    uart::putc(p, b'\n');

    // Clock: CSCTL7 carries the FLL (un)lock + fault flags. Nonzero unlock bits = bad UART/I2C timing.
    uart::puts(p, "[clock]\n");
    row(p, " CSCTL1", p.cs.csctl1().read().bits());
    row(p, " CSCTL2", p.cs.csctl2().read().bits());
    row(p, " CSCTL3", p.cs.csctl3().read().bits());
    row(p, " CSCTL4", p.cs.csctl4().read().bits());
    row(p, " CSCTL5", p.cs.csctl5().read().bits());
    row(p, " CSCTL6", p.cs.csctl6().read().bits());
    // CSCTL7 FLLUNLOCK bits (0x0300) = the live FLL lock state. Zero = locked (clean UART/I2C
    // timing); nonzero = the DCO isn't tracking, which garbles the console. MCU-self clock verdict.
    let csctl7 = p.cs.csctl7().read().bits();
    row(p, " CSCTL7", csctl7);
    uart::puts(p, " FLL: ");
    uart::puts(p, if csctl7 & 0x0300 == 0 { "LOCKED\n" } else { "UNLOCKED\n" });

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
