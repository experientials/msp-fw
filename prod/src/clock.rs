//! FR247x clock init — MCLK = SMCLK = DCODIV = 1 MHz (FLL ref = REFO), ACLK = REFO.
//! Ported verbatim from diag's proven `clock_init_1mhz` (same FR2476 CS). Disables the FLL (SCG0)
//! while retuning and waits for re-lock before switching the clocks — required for a clean 9600
//! baud UART; the loose "close enough" version garbles the first bytes.

use crate::pac::Peripherals;

const SELREF_REFOCLK: u16 = 0x0010;
// DCO trim: only FR2476 (fr247x) uses the MANUAL trim; the FR2355/FR2155 family (fr235x/fr215x) and
// FR2433 (fr24xx) use the FACTORY trim (DCORSEL range only) or their FLL won't lock (diag lesson).
// So these manual-trim consts exist only for fr247x — cfg them out elsewhere to keep builds clean.
// (Interim: the per-chip trim really belongs in bsp::board — CONV-1/#5.)
#[cfg(feature = "fr247x")]
const DCOFTRIMEN: u16 = 0x0080;
#[cfg(feature = "fr247x")]
const DCOFTRIM0: u16 = 0x0010;
#[cfg(feature = "fr247x")]
const DCOFTRIM1: u16 = 0x0020;
const DCORSEL_0: u16 = 0x0000;
const FLLD_0: u16 = 0x0000;
const SELMS_DCOCLKDIV: u16 = 0x0000;
const SELA_REFOCLK: u16 = 0x0100;
const FLLUNLOCK: u16 = 0x0300; // FLLUNLOCK0 | FLLUNLOCK1 (CSCTL7)

/// Bring MCLK/SMCLK to a locked 1 MHz off the DCO+FLL. Call once at boot before UART/I2C init.
pub fn init_1mhz(p: &Peripherals) {
    unsafe { core::arch::asm!("bis #0x40, r2", options(nomem, nostack)) }; // SCG0=1: FLL off
    p.cs.csctl3().modify(|r, w| unsafe { w.bits(r.bits() | SELREF_REFOCLK) });
    // Trim: FR2476 (fr247x) uses the manual DCOFTRIM; the FR2355/FR2155 family + FR2433 use the
    // FACTORY trim (1 MHz DCORSEL range only, no DCOFTRIMEN) or the FLL never locks.
    #[cfg(feature = "fr247x")]
    p.cs
        .csctl1()
        .write(|w| unsafe { w.bits(DCOFTRIMEN | DCOFTRIM0 | DCOFTRIM1 | DCORSEL_0) });
    #[cfg(not(feature = "fr247x"))]
    p.cs.csctl1().write(|w| unsafe { w.bits(DCORSEL_0) });
    p.cs.csctl2().write(|w| unsafe { w.bits(FLLD_0 + 30) }); // FLLN=30 -> 1 MHz
    msp430::asm::nop();
    msp430::asm::nop();
    msp430::asm::nop();
    unsafe { core::arch::asm!("bic #0x40, r2", options(nomem, nostack)) }; // SCG0=0: FLL on
    // Bounded wait — don't hang boot if the FLL never asserts lock (a silent hang is worse than a
    // slightly-off clock we can see garbled and then fix).
    let mut spins = 0u16;
    while p.cs.csctl7().read().bits() & FLLUNLOCK != 0 {
        spins += 1;
        if spins > 20000 {
            break;
        }
    }
    p.cs.csctl4().write(|w| unsafe { w.bits(SELMS_DCOCLKDIV | SELA_REFOCLK) });
    for _ in 0..8000u16 {
        msp430::asm::nop(); // let the DCO settle before clocking the UART
    }
}
