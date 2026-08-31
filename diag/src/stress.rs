//! Stress & margin mode — built with `--features stress`, NOT part of the normal POST.
//!
//! It hammers the I²C bus two ways and reports over UART (see DIAGNOSTICS.md for the pass
//! thresholds):
//!   1. **Clock-margin sweep** — raise SCL 100 kHz → 1 MHz and, at each step, do many *verified*
//!      reads (read a known-valued ID register and compare). The highest clock with zero errors is
//!      the bus's reliability margin. Weak pull-ups / long wiring / high capacitance fail early.
//!   2. **Error-rate soak** — hold the nominal 100 kHz and read continuously for a fixed window,
//!      counting NACKs and bit-corruptions per transaction and tracking latency min/max.
//!
//! "Verified" (compare a known register value) is the point: it catches **bit corruption**, not
//! just NACKs — a plain probe can't tell a clean bus from one flipping bits.

use crate::clock::Clock;
use crate::{i2c, uart, usec};
use msp430fr2476::Peripherals;

/// A device we can read a fixed-value register from, so each transaction is checkable.
struct Target {
    addr: u8,
    reg: u8,
    val: u8,
}

// WHO_AM_I / ID registers with datasheet-constant values (same ones the POST registry verifies).
static TARGETS: &[Target] = &[
    Target { addr: 0x39, reg: 0x92, val: 0xAB }, // APDS-9960
    Target { addr: 0x29, reg: 0xC0, val: 0xEE }, // VL53L0X
];

/// SCL = SMCLK (1 MHz) / brw.
struct Step {
    khz: u16,
    brw: u16,
}
static LADDER: &[Step] = &[
    Step { khz: 100, brw: 10 },
    Step { khz: 200, brw: 5 },
    Step { khz: 333, brw: 3 },
    Step { khz: 500, brw: 2 },
    Step { khz: 1000, brw: 1 },
];

const NOMINAL_BRW: u16 = 10; // 100 kHz — the operating clock, used for the soak
const MARGIN_REPS: u16 = 200; // verified reads per target, per clock step
const REPORT_EVERY_SECS: u32 = 10; // cadence of the cumulative soak report line
const SOAK_LIMIT_SECS: u32 = 0; // 0 = soak indefinitely; else stop + print a final verdict after N s

/// One verified read. `Ok(latency_us)`, or `Err(true)` on a corrupt value / `Err(false)` on
/// NACK/timeout.
fn verified_read(p: &Peripherals, t: &Target) -> Result<u16, bool> {
    let t0 = usec::now(p);
    let mut b = [0u8; 1];
    if !i2c::read_reg(p, t.addr, t.reg, &mut b) {
        return Err(false); // NACK / timeout
    }
    let dt = usec::now(p).wrapping_sub(t0);
    if b[0] != t.val {
        return Err(true); // corrupt
    }
    Ok(dt)
}

/// Returns the highest clean clock as `(khz, brw)`, or the nominal 100 kHz if nothing was clean.
fn margin_sweep(p: &Peripherals) -> (u16, u16) {
    uart::puts(p, "\n-- I2C clock-margin sweep --\n");
    let mut best_clean_khz = 0u16;
    let mut best_clean_brw = NOMINAL_BRW;
    for s in LADDER {
        i2c::set_brw(p, s.brw);
        crate::pet_wdt(p);
        let (mut nack, mut corrupt, mut txn) = (0u16, 0u16, 0u16);
        for t in TARGETS {
            let mut consec_fail = 0u16;
            for _ in 0..MARGIN_REPS {
                // Pet per read: at a clock the bus can't do, each read burns its full SPIN
                // timeout (~tens of ms), so 400 of them would overrun the ~16 s WDT and reset the
                // board mid-step — which is why a failing high clock never printed its line.
                crate::pet_wdt(p);
                txn += 1;
                match verified_read(p, t) {
                    Ok(_) => consec_fail = 0,
                    Err(false) => {
                        nack += 1;
                        consec_fail += 1;
                    }
                    Err(true) => corrupt += 1, // responded but wrong — keep sampling; it's fast
                }
                // A clock that NACKs this many in a row is unusable — stop hammering it (also keeps
                // the timing-out case snappy instead of grinding through all reps).
                if consec_fail >= 24 {
                    break;
                }
            }
        }
        uart::puts(p, "  ");
        uart::dec(p, s.khz);
        uart::puts(p, " kHz: ");
        uart::dec(p, txn);
        uart::puts(p, " txn, ");
        uart::dec(p, nack);
        uart::puts(p, " nack, ");
        uart::dec(p, corrupt);
        uart::puts(p, " corrupt  ");
        if nack == 0 && corrupt == 0 {
            uart::puts(p, "OK");
            if s.khz > best_clean_khz {
                best_clean_khz = s.khz;
                best_clean_brw = s.brw;
            }
        } else {
            uart::puts(p, "FAIL");
        }
        uart::puts(p, "\n");
    }
    uart::puts(p, "  margin: highest clean SCL = ");
    uart::dec(p, best_clean_khz);
    uart::puts(p, " kHz\n");
    if best_clean_khz == 0 {
        // Even 100 kHz failed — soak there anyway to keep logging the errors.
        (100, NOMINAL_BRW)
    } else {
        (best_clean_khz, best_clean_brw)
    }
}

/// Running soak totals. Transaction count is split into millions + remainder so a multi-week run
/// doesn't overflow a single u32 (~12 days at ~4 k txn/s). Errors stay u32 (a board throwing that
/// many has already failed). Latency min/max are cumulative worst-case over the whole run.
struct Totals {
    txn_m: u32,
    txn: u32,
    nack: u32,
    corrupt: u32,
    lat_min: u16,
    lat_max: u16,
    any_ok: bool,
}

impl Totals {
    fn bump_txn(&mut self) {
        self.txn += 1;
        if self.txn >= 1_000_000 {
            self.txn_m += 1;
            self.txn -= 1_000_000;
        }
    }
    fn clean(&self) -> bool {
        self.nack == 0 && self.corrupt == 0
    }
}

/// Cumulative error-rate soak at nominal 100 kHz. Accumulates totals across the ENTIRE run (not
/// reset per window) and prints one report line every `REPORT_EVERY_SECS`, so a multi-day run
/// yields a single running verdict ("N errors in M txns over T s"), not a stream of isolated
/// windows. Runs indefinitely unless `SOAK_LIMIT_SECS` bounds it, in which case it prints a final
/// verdict and holds. Diverges.
fn soak(p: &Peripherals, clock: &mut Clock, khz: u16, brw: u16) -> ! {
    i2c::set_brw(p, brw); // soak at the fastest clean clock the sweep found (1 MHz if it passed)
    uart::puts(p, "\n-- I2C error-rate soak @");
    uart::dec(p, khz);
    uart::puts(p, " kHz (cumulative) --\n");
    let start_ms = clock.now_ms(p);
    let mut tot = Totals {
        txn_m: 0,
        txn: 0,
        nack: 0,
        corrupt: 0,
        lat_min: 0xFFFF,
        lat_max: 0,
        any_ok: false,
    };
    let mut window = 0u32; // txns since last report, for the rate figure
    let mut next_report = start_ms + REPORT_EVERY_SECS * 1000;
    loop {
        for t in TARGETS {
            tot.bump_txn();
            window += 1;
            match verified_read(p, t) {
                Ok(dt) => {
                    tot.any_ok = true;
                    if dt < tot.lat_min {
                        tot.lat_min = dt;
                    }
                    if dt > tot.lat_max {
                        tot.lat_max = dt;
                    }
                }
                Err(false) => tot.nack += 1,
                Err(true) => tot.corrupt += 1,
            }
        }
        crate::pet_wdt(p); // long run must keep the backstop fed

        let now = clock.now_ms(p);
        if now >= next_report {
            report(p, (now - start_ms) / 1000, &tot, window / REPORT_EVERY_SECS);
            window = 0;
            next_report += REPORT_EVERY_SECS * 1000;
        }
        if SOAK_LIMIT_SECS != 0 && (now - start_ms) >= SOAK_LIMIT_SECS * 1000 {
            uart::puts(p, "\nSOAK COMPLETE @");
            dec32(p, SOAK_LIMIT_SECS);
            uart::puts(p, "s -> ");
            uart::puts(p, if tot.clean() { "PASS" } else { "FAIL" });
            uart::puts(p, " (0-error threshold)\n");
            loop {
                crate::pet_wdt(p); // stop hammering; hold the result on screen
            }
        }
    }
}

/// One cumulative soak report line: `[t=Ns] txns=..  err=n+c  rate=../s  lat=min-max us  PASS/FAIL`.
fn report(p: &Peripherals, secs: u32, tot: &Totals, rate: u32) {
    uart::puts(p, "  [t=");
    dec32(p, secs);
    uart::puts(p, "s] txns=");
    if tot.txn_m > 0 {
        dec32(p, tot.txn_m);
        uart::puts(p, "M ");
    }
    dec32(p, tot.txn);
    uart::puts(p, "  err=");
    dec32(p, tot.nack);
    uart::puts(p, "+");
    dec32(p, tot.corrupt);
    uart::puts(p, "  rate=");
    dec32(p, rate);
    uart::puts(p, "/s  lat=");
    if tot.any_ok {
        uart::dec(p, tot.lat_min);
        uart::puts(p, "-");
        uart::dec(p, tot.lat_max);
        uart::puts(p, "us");
    } else {
        uart::puts(p, "n/a");
    }
    uart::puts(p, if tot.clean() { "  PASS\n" } else { "  FAIL\n" });
}

/// Stress entry — diverges. Characterise the bus margin once, then soak at the fastest clean clock.
pub fn run(p: &Peripherals, clock: &mut Clock) -> ! {
    uart::puts(p, "\n=== STRESS MODE: I2C margin + soak ===\n");
    crate::pet_wdt(p);
    let (khz, brw) = margin_sweep(p);
    soak(p, clock, khz, brw)
}

/// u32 decimal (uart::dec is u16-only; soak counts overflow that in seconds).
fn dec32(p: &Peripherals, mut v: u32) {
    if v == 0 {
        uart::putc(p, b'0');
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = 0;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        uart::putc(p, buf[i]);
    }
}
