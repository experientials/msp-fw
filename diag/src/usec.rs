//! Microsecond time base for stress instrumentation — TA0 free-running off SMCLK (1 MHz), so TA0R
//! ticks at 1 µs and wraps every 65.536 ms. Used to time individual I²C transactions; every
//! measured interval is far under one wrap. Separate from the `clock` ms base (TB0/ACLK) because
//! transaction latencies are tens–hundreds of µs — below the 30.5 µs ACLK resolution.
//!
//! Only compiled into the `stress` build.

use msp430fr2476::Peripherals;

const TASSEL_SMCLK: u16 = 0x0200; // TASSEL_2
const MC_CONTINUOUS: u16 = 0x0020; // MC_2
const TACLR: u16 = 0x0004;

/// Start TA0 counting continuously at 1 µs/tick. SMCLK must be 1 MHz (main's clock_init).
pub fn start(p: &Peripherals) {
    p.ta0
        .ta0ctl()
        .write(|w| unsafe { w.bits(TASSEL_SMCLK | MC_CONTINUOUS | TACLR) });
}

/// Current µs counter (wraps at 65536). Diff two samples with `wrapping_sub` for an elapsed µs.
#[inline]
pub fn now(p: &Peripherals) -> u16 {
    p.ta0.ta0r().read().bits()
}
