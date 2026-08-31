//! Free-running millisecond clock — the time base the cooperative [`sched`] loop needs.
//!
//! TB0 counts continuously from ACLK (REFO, 32768 Hz — routed by `main::clock_init_1mhz`). We
//! never take the timer interrupt; `now_ms` just polls TB0R and folds the 16-bit delta into a
//! 32-bit millisecond count. That keeps the "no ISR" property of diag today (the port-interrupt /
//! wake path is a later step) and is robust to long tasks: the only requirement is that the loop
//! reads the clock at least once per TB0 wrap (~2 s at 32768 Hz), which the POST loop beats by
//! orders of magnitude even while a full I²C scan runs.
//!
//! The delta is accumulated with a sub-millisecond carry so there is **no rounding drift**:
//! d ticks contribute `d * 1000 / 32768` ms exactly, remainder carried to the next call.

use msp430fr2476::Peripherals;

const TBSSEL_ACLK: u16 = 0x0100; // TBSSEL_1: clock TB0 from ACLK
const MC_CONTINUOUS: u16 = 0x0020; // MC_2: count up to 0xFFFF and wrap
const TBCLR: u16 = 0x0004; // clear counter + divider on start

pub struct Clock {
    last: u16, // previous TB0R sample
    ms: u32,   // whole milliseconds since start()
    sub: u32,  // fractional-ms carry, in units of (1/32768 s), pre-scaled by 1000
}

impl Clock {
    /// Start TB0 free-running from ACLK and return a zeroed clock. ACLK must already be live
    /// (main's `clock_init_1mhz` routes ACLK to REFO before this is called).
    pub fn start(p: &Peripherals) -> Self {
        p.tb0
            .tb0ctl()
            .write(|w| unsafe { w.bits(TBSSEL_ACLK | MC_CONTINUOUS | TBCLR) });
        Self { last: 0, ms: 0, sub: 0 }
    }

    /// Monotonic milliseconds since [`start`]. Cheap; call it every scheduler pass.
    ///
    /// [`start`]: Clock::start
    pub fn now_ms(&mut self, p: &Peripherals) -> u32 {
        let t = p.tb0.tb0r().read().bits();
        let d = t.wrapping_sub(self.last) as u32; // 0..=65535 ticks since last read
        self.last = t;
        // d*1000 <= 6.55e7, plus a remainder < 32768 -> no u32 overflow.
        self.sub += d * 1000;
        self.ms += self.sub >> 15; // / 32768 -> whole ms
        self.sub &= 0x7FFF; // keep the sub-ms remainder (< 32768)
        self.ms
    }
}
