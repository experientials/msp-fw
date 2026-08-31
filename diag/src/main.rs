#![no_main]
#![no_std]
#![feature(asm_experimental_arch)] // core::arch::asm! for SCG0 (SR bit) during FLL retune
// The `stress` build replaces the POST/scheduler with a bus-hammering mode, so the POST modules go
// unused there. Allow dead_code in that build rather than cfg-gate every module; the default build
// stays strict.
#![cfg_attr(feature = "stress", allow(dead_code))]

//! bob-929 diagnostic firmware (Rust) — MSP430FR2476.
//!
//! Power-on self-test in the spirit of an Amiga diag ROM: scan the I2C bus and
//! exercise each known device, reporting over the eZ-FET backchannel UART and,
//! when present, on the IS31FL3730 LED matrix. Register access goes through the
//! vendored `msp430fr2476` PAC (typed peripherals) using proven bit values from
//! the FR2476 header — not raw pointers.

extern crate panic_msp430; // infinitely-looping panic handler

use msp430_rt::entry;
use msp430fr2476::Peripherals;

mod adc;
mod apds;
mod clock;
mod diag;
mod hal;
mod i2c;
mod is31;
mod mc6470;
mod oled;
mod rcwl;
mod regs;
mod tasks;
mod uart;

#[cfg(feature = "stress")]
mod stress;
#[cfg(feature = "stress")]
mod usec;

// Watchdog / power-management / clock-system bits (msp430fr2476.h).
const WDTPW: u16 = 0x5A00;
const WDTHOLD: u16 = 0x0080;
const WDTSSEL_ACLK: u16 = 0x0020; // WDTSSEL_1: clock the WDT from ACLK (REFO, 32768 Hz here)
const WDTIS_DIV_2E19: u16 = 0x0003; // WDTIS_3: /2^19 -> ~16 s timeout at 32768 Hz
const WDTCNTCL: u16 = 0x0008; // clear the counter ("pet")
// Backstop config: watchdog (reset) mode — WDTTMSEL=0 — ACLK, ~16 s. Far longer than a worst-case
// POST cycle (3 s delay + a few s of I2C/recovery/flush), so normal running never trips it; a real
// hang (or a panic-msp430 infinite loop) does, and the board resets and retries instead of dying.
const WDT_BACKSTOP: u16 = WDTPW | WDTSSEL_ACLK | WDTIS_DIV_2E19;
const LOCKLPM5: u16 = 0x0001;
const SELREF_REFOCLK: u16 = 0x0010;
const DCOFTRIMEN: u16 = 0x0080;
const DCOFTRIM0: u16 = 0x0010;
const DCOFTRIM1: u16 = 0x0020;
const DCORSEL_0: u16 = 0x0000;
const FLLD_0: u16 = 0x0000;
const SELMS_DCOCLKDIV: u16 = 0x0000;
const SELA_REFOCLK: u16 = 0x0100;
const FLLUNLOCK: u16 = 0x0300; // FLLUNLOCK0 | FLLUNLOCK1 (CSCTL7)

// P1SEL0 bits routing the eUSCI functions: P1.2/P1.3 -> UCB0 I2C, P1.4/P1.5 -> UCA0 UART.
const P1_EUSCI_PINS: u8 = 0x3C; // BIT2|BIT3|BIT4|BIT5
const P2_SDB: u8 = 0x20; // P2.5 -> IS31FL3730 SDB (drive high to enable). NOT P2.0 — crystal XOUT.
const P2_RCWL_OUT: u8 = 0x10; // P2.4 <- RCWL-0516 radar OUT (GPIO input, port-interrupt capable).

// Boot banner (ASCII "DIAG") — a hard visual break so every reset is obvious in a scrollback.
// Note: `\<newline>` string continuation eats leading whitespace, so no art row may start with a
// space — the letters are laid out to begin on a non-space column.
const BANNER: &str = "\n\
========================================\n\
###    ###    ##    ###\n\
#  #    #    #  #   #   \n\
#  #    #    ####   # ##\n\
#  #    #    #  #   #  #\n\
###    ###   #  #    ###\n\
========================================\n";

/// MCLK = SMCLK = DCODIV = 1 MHz (FLL ref = REFO), ACLK = REFO. Disables the FLL (SCG0)
/// while retuning and waits for re-lock before switching the clocks — required for a clean
/// 9600 baud UART; the loose "close enough" version garbles the first bytes.
fn clock_init_1mhz(p: &Peripherals) {
    unsafe { core::arch::asm!("bis #0x40, r2", options(nomem, nostack)) }; // SCG0=1: FLL off
    p.cs.csctl3().modify(|r, w| unsafe { w.bits(r.bits() | SELREF_REFOCLK) });
    p.cs
        .csctl1()
        .write(|w| unsafe { w.bits(DCOFTRIMEN | DCOFTRIM0 | DCOFTRIM1 | DCORSEL_0) });
    p.cs.csctl2().write(|w| unsafe { w.bits(FLLD_0 + 30) }); // FLLN=30 -> 1 MHz
    msp430::asm::nop();
    msp430::asm::nop();
    msp430::asm::nop();
    unsafe { core::arch::asm!("bic #0x40, r2", options(nomem, nostack)) }; // SCG0=0: FLL on
    while p.cs.csctl7().read().bits() & FLLUNLOCK != 0 {} // wait for FLL lock
    p.cs.csctl4().write(|w| unsafe { w.bits(SELMS_DCOCLKDIV | SELA_REFOCLK) });
    for _ in 0..8000u16 {
        msp430::asm::nop(); // let the DCO settle before clocking the UART
    }
}

/// Pet the watchdog backstop (reload its counter). Called every scheduler pass, and inside the
/// stress runner's long loops so a multi-second soak doesn't trip the ~16 s reset.
fn pet_wdt(p: &Peripherals) {
    p.wdt_a
        .wdtctl()
        .write(|w| unsafe { w.bits(WDT_BACKSTOP | WDTCNTCL) });
}

#[entry]
fn main() -> ! {
    // Single-threaded, no ISRs -> steal is fine and avoids the critical-section feature.
    let p = unsafe { Peripherals::steal() };

    p.wdt_a.wdtctl().write(|w| unsafe { w.bits(WDTPW | WDTHOLD) }); // stop watchdog
    clock_init_1mhz(&p);

    // Route P1.2..P1.5 to eUSCI. The pin function is the 2-bit pair (P1SEL1:P1SEL0);
    // 01 = primary module (UCB0 I2C on P1.2/1.3, UCA0 UART on P1.4/1.5). Reset leaves
    // P1SEL1=0, but clear the bits explicitly so we don't silently depend on that and
    // accidentally select the secondary/tertiary function.
    p.p1.p1sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P1_EUSCI_PINS) }); // SEL1=0
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | P1_EUSCI_PINS) }); // SEL0=1
    p.pmm.pm5ctl0().modify(|r, w| unsafe { w.bits(r.bits() & !LOCKLPM5) });

    // Enable the IS31FL3730 (SDB high on P2.5). Set output high before direction.
    p.p2.p2sel0().modify(|r, w| unsafe { w.bits(r.bits() & !P2_SDB) }); // ensure GPIO
    p.p2.p2out().modify(|r, w| unsafe { w.bits(r.bits() | P2_SDB) });
    p.p2.p2dir().modify(|r, w| unsafe { w.bits(r.bits() | P2_SDB) });

    // RCWL-0516 radar OUT on P2.4: GPIO input with a pulldown, so an unplugged/idle sensor reads
    // LOW instead of floating. OUT is push-pull (drives HIGH ~3.3 V on motion), so the pulldown
    // only loads it (~µA) while asserted — it doesn't fight the driver. Read in `rcwl`; P2.4 is
    // port-interrupt capable if we later wake on motion (not enabled here — diag just polls).
    p.p2.p2sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P2_RCWL_OUT) }); // SEL=00 -> GPIO
    p.p2.p2sel0().modify(|r, w| unsafe { w.bits(r.bits() & !P2_RCWL_OUT) });
    p.p2.p2dir().modify(|r, w| unsafe { w.bits(r.bits() & !P2_RCWL_OUT) }); // input
    p.p2.p2out().modify(|r, w| unsafe { w.bits(r.bits() & !P2_RCWL_OUT) }); // OUT=0 selects pulldown
    p.p2.p2ren().modify(|r, w| unsafe { w.bits(r.bits() | P2_RCWL_OUT) }); // enable the resistor

    uart::init(&p);
    i2c::init(&p);
    adc::init(&p); // internal 1.5-V ref + ADC core, for measured Vcc/temperature in the board stats

    // Reset delineation: a chunky ASCII banner so each boot/reset is unmistakable when scrolling
    // back through a long `just monitor` capture (retro diag-ROM spirit). Build stamp rides along
    // so the reset boundary also tells you which firmware just came up.
    uart::puts(&p, BANNER);
    uart::puts(&p, "  build ");
    uart::puts(&p, env!("DIAG_BUILD"));
    uart::putc(&p, b'\n');
    // Basic board stats read from the silicon (chip id, revisions, die serial) — not a hardcoded
    // model. Then the live SFR dump (reset cause, clock lock, pin mux) we reason from all run.
    regs::stats(&p);
    regs::dump(&p);

    // Millisecond time base for the cooperative scheduler (TB0 off ACLK, polled — no ISR).
    let mut clock = clock::Clock::start(&p);

    // Arm the watchdog backstop now that init is done (init itself is deterministic and runs
    // unguarded). Every scheduler pass pets it; anything that wedges past ~16 s resets the board —
    // which is exactly the backstop cooperative scheduling needs, since one task that ignores the
    // never-block contract has no other way to be preempted.
    p.wdt_a.wdtctl().write(|w| unsafe { w.bits(WDT_BACKSTOP) });

    // Stress build: replace the POST with the I2C margin + soak runner (diverges). See stress.rs
    // + DIAGNOSTICS.md. Default build: run the cooperative scheduler.
    #[cfg(feature = "stress")]
    {
        usec::start(&p);
        stress::run(&p, &mut clock)
    }

    #[cfg(not(feature = "stress"))]
    {
        // Cooperative tasks (see tasks.rs + the `sched` crate). `cx` borrows the peripherals and
        // carries the cross-task blackboard; each task owns its own private state. The radar samples
        // continuously and accumulates a motion window; the POST reports + clears it every ~3 s.
        let mut cx = tasks::Cx::new(&p);
        let mut radar = tasks::RadarTask::new();
        let mut prox = tasks::ProximityTask::new();
        let mut post = tasks::PostTask;
        let mut slots = [
            sched::Slot::every(50, &mut radar), // self-adjusts to 5 ms while motion is asserted
            sched::Slot::every(200, &mut prox), // APDS-9960 proximity, 5 Hz
            sched::Slot::every(3000, &mut post),
        ];

        let mut next_stats = 10_000u32; // scheduler deadline telemetry every ~10 s
        loop {
            pet_wdt(&p);
            let now = clock.now_ms(&p);
            sched::tick(&mut slots, &mut cx, now);
            if now >= next_stats {
                // Per-task deadline health: worst lateness + missed-slot count. Under real load the
                // ~3 s POST hogs the loop for tens of ms, so radar/prox show that as their max-late —
                // the direct, quantified view of the "task running too long" concern.
                uart::puts(&p, "sched:");
                for s in &slots {
                    uart::puts(&p, " ");
                    uart::puts(&p, s.name());
                    uart::puts(&p, "(late");
                    uart::dec(&p, s.max_late().min(65535) as u16);
                    uart::puts(&p, "ms ovr");
                    uart::dec(&p, s.overruns().min(65535) as u16);
                    uart::puts(&p, ")");
                }
                uart::puts(&p, "\n");
                // Live board health (rail + die temp), re-measured each interval — a supervisor
                // watches these trend, and it keeps them visible if the boot [board] block scrolled
                // past or was missed on monitor attach.
                uart::puts(&p, "board: ");
                regs::vcc_temp(&p);
                uart::puts(&p, "\n");
                next_stats = now.wrapping_add(10_000);
            }
        }
    }
}

// Debug builds emit calls to abort(); MSP430 has no meaningful abort.
#[no_mangle]
extern "C" fn abort() -> ! {
    panic!();
}
