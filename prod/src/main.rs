#![no_main]
#![no_std]
#![feature(asm_experimental_arch)] // core::arch::asm! for SCG0 in clock.rs (fr247x); harmless otherwise

//! bob-929 PRODUCT firmware (Rust) — **Stembus node**, primary dev target MSP430FR2476 (FR247x).
//!
//! DEV SEQUENCING (Henrik, 2026-09-21): developed now on the on-hand **FR2476** boards — a **dual-I2C**
//! part (2× eUSCI_B), so it exercises the full node role: a **Stem-bus I2C SLAVE** control surface to
//! the SoM (regmap.rs) *and* a local **sensor-bus I2C MASTER**. The production dual-I2C part is the
//! **FR2155** (FR215x); the port is a PAC/memory.x swap once an FR2355 board (its strict superset) is
//! available to sign off — the one thing FR2476 can't test is the 2nd eCOMP (1 vs 2). The cheap 1× I2C
//! **FR2433** slave-only node is a separate build (the family axis) added later. See prod/DESIGN.md +
//! docs/MCU_SELECTION.md.
//!
//! `just prod build` gates the image against the FR247x 32 KB toolchain window / 8 KB SRAM
//! (scripts/size-check.sh).
//!
//! STATUS: scaffold only. Boots, holds the watchdog, unlocks GPIO, and idles. The node duties below
//! are stubs — see prod/README.md and derive them from STEM-EXPANDER.md / I2C-API.md / objectives.
//!
//! Register access goes through the vendored `msp430fr2476` PAC (typed peripherals), same family as
//! diag — so diag's clock.rs / uart.rs / i2c.rs are direct references. Diag runs eUSCI_B as a MASTER;
//! the MCU-bus surface here must be an I2C SLAVE (a different code path), while the sensor bus reuses
//! diag's master path. `p.cs` is the clock system; `p.e_usci_b0`/`p.e_usci_b1` are the two I2C blocks.

extern crate panic_msp430; // infinitely-looping panic handler (a hang trips the watchdog → reset)

use msp430_rt::entry;

// Family axis: exactly one family feature selects the PAC (and, via build.rs, the memory map).
#[cfg(not(any(feature = "fr247x", feature = "fr24xx")))]
compile_error!("prod: enable one family feature — `--features fr247x` (default) or `--features fr24xx`");
#[cfg(all(feature = "fr247x", feature = "fr24xx"))]
compile_error!("prod: enable exactly ONE family feature (fr247x XOR fr24xx), not both");

// OPERATING MODES (DESIGN.md): compile in ≥1 mode; sensing needs a 2nd I²C so it's dual-I²C only.
#[cfg(not(any(feature = "mode-passive", feature = "mode-sensing")))]
compile_error!("prod: enable at least one mode — `--features mode-passive` and/or `mode-sensing`");
#[cfg(all(feature = "fr24xx", feature = "mode-sensing"))]
compile_error!("prod: `mode-sensing` requires a dual-I²C family — FR2433 (fr24xx) is Passive-only");

#[cfg(feature = "fr247x")]
use msp430fr2476::Peripherals; // FR2476/FR2475 — dual-I²C (2× eUSCI_B)
#[cfg(feature = "fr24xx")]
use msp430fr2433::Peripherals; // FR2433 — single-I²C slave-only

mod model; // Runtime MSP430 model detection + pin mapping (one image per compatible family).
mod regmap; // I²C-slave register-map contract (PCA9698 emulation + Thepia extensions). See regmap.rs.

// FR247x bring-up modules (the primary dev target). The FR24xx path is still the idle scaffold, so
// these are cfg-gated to fr247x — they use the FR2476 PAC directly (the per-family bus glue). The
// shared, portable device drivers live in `crates/devices`; only this glue is chip-specific.
#[cfg(feature = "fr247x")]
mod clock;
#[cfg(feature = "fr247x")]
mod hal;
#[cfg(feature = "fr247x")]
mod i2c;
#[cfg(feature = "fr247x")]
mod enumerate;
#[cfg(feature = "fr247x")]
mod status;
#[cfg(feature = "fr247x")]
mod mode;
#[cfg(feature = "fr247x")]
mod stem;
// UART logging is the `console` feature (dev only). Off = silent production image; state then lives
// only in the debug registers (served by the I2C-slave surface). See status.rs / the console question.
#[cfg(all(feature = "fr247x", feature = "console"))]
mod uart;

/// Compiled-in identity stamp (see build.rs). Printed at boot once the UART stub lands.
const FW_BUILD: &str = env!("PROD_BUILD");

// Watchdog control (common across MSP430 FRAM parts). WDTPW is the write password; WDTHOLD stops it.
const WDTPW: u16 = 0x5A00;
const WDTHOLD: u16 = 0x0080;
// PM5CTL0.LOCKLPM5 — set out of reset on FRAM parts; must be cleared to make GPIO live.
const LOCKLPM5: u16 = 0x0001;

#[entry]
fn main() -> ! {
    // steal() (not take()) to match diag and avoid pulling in the critical-section machinery; the
    // scaffold is single-threaded with no ISRs yet, so exclusive access is trivially upheld.
    let p = unsafe { Peripherals::steal() };

    // Hold the watchdog during bring-up. The shipping node will instead run the WDT in reset mode as
    // a hang backstop (see diag/src/main.rs WDT_BACKSTOP) and pet it from the main loop. (The only
    // per-family delta in the scaffold: the WDT peripheral field name — fr247x `wdt_a` vs fr24xx
    // `watchdog_timer`; same UCS/WDTCTL register underneath.)
    #[cfg(feature = "fr247x")]
    p.wdt_a.wdtctl().write(|w| unsafe { w.bits(WDTPW | WDTHOLD) });
    #[cfg(feature = "fr24xx")]
    p.watchdog_timer.wdtctl().write(|w| unsafe { w.bits(WDTPW | WDTHOLD) });

    // Unlock the I/O from the LPM5 default so port config takes effect (both families have PMM).
    p.pmm.pm5ctl0().modify(|r, w| unsafe { w.bits(r.bits() & !LOCKLPM5) });

    // Keep the build stamp live for the family paths below.
    let _ = FW_BUILD;

    #[cfg(feature = "fr247x")]
    run_fr247x(&p);

    #[cfg(feature = "fr24xx")]
    run_fr24xx(&p);
}

/// FR247x (FR2476/FR2475) node bring-up: clock → pins → UART → sensor-bus I2C master → boot banner →
/// enumerate the bus → report state on demand over the backchannel UART (s = re-scan, r = re-report).
/// This is the first working prod function; the I2C-slave surface + sensor `measure()` come next.
#[cfg(feature = "fr247x")]
fn run_fr247x(p: &Peripherals) -> ! {
    // Route only the UART pins (P1.4/P1.5 → UCA0) here — needed for `console`, harmless otherwise.
    // The SENSOR I²C pins (P1.2/P1.3 → UCB0) are owned by the MODE machine: acquired in Sensing,
    // released (tri-stated) in Passive. Never routed unconditionally, so a Passive node stays off the bus.
    const P1_UART_PINS: u8 = 0x30; // BIT4|BIT5

    clock::init_1mhz(p);
    p.p1.p1sel1().modify(|r, w| unsafe { w.bits(r.bits() & !P1_UART_PINS) });
    p.p1.p1sel0().modify(|r, w| unsafe { w.bits(r.bits() | P1_UART_PINS) });

    let model = model::detect();

    // Enter the boot-default operating mode. In Sensing this ACQUIRES the sensor bus (B0 master); in
    // Passive it ensures the bus is RELEASED (the SoM owns it). Runtime-switchable below.
    let mut mach = mode::ModeMachine::boot(p);

    // Sensor scan runs ONLY in Sensing (Passive never masters the sensor bus). It populates the
    // debug registers the SoM reads over the I2C-slave surface. UART reporting below is `console`-gated.
    #[allow(unused_mut)]
    let mut last = if mach.is_sensing() {
        enumerate::scan(p)
    } else {
        enumerate::Scan::absent()
    };

    // The Stem-bus I2C-slave surface (eUSCI_B1): the SoM reads this register file. `rf` holds the
    // live status snapshot (+ PCA9698 shadow); `slave` is the per-transaction state. Polled below.
    let mut rf = stem::RegFile::new(if mach.is_sensing() {
        status::Status::from_scan(model, &last)
    } else {
        status::Status::booted(model) // Passive: not enumerated (didn't master the bus)
    });
    let mut slave = stem::Slave::new();
    stem::init(p);

    // Dev observability: banner + human report + on-demand commands over the backchannel UART.
    #[cfg(feature = "console")]
    {
        uart::init(p);
        uart::puts(p, "\n== prod ");
        uart::puts(p, FW_BUILD);
        uart::puts(p, " (");
        uart::puts(p, model_name(model));
        uart::puts(p, ") mode=");
        uart::puts(p, mach.current().name());
        uart::puts(p, " ==\n");
        if !model.matches_build_family() {
            uart::puts(p, "!! WRONG-FAMILY FLASH: this image is fr247x — detected part is not FR247x\n");
        }
        uart::puts(p, "commands: s=scan  r=report  d=debug regs  m=switch mode\n");
        enumerate::report(p, &last);
        rf.status.dump(p);
        loop {
            stem::poll(p, &mut slave, &mut rf); // service the SoM/Stem master
            if uart::rx_ready(p) {
                match uart::getc(p) {
                    b's' | b'S' => {
                        if mach.is_sensing() {
                            last = enumerate::scan(p);
                            enumerate::report(p, &last);
                            rf.status = status::Status::from_scan(model, &last);
                        } else {
                            uart::puts(p, "  (passive: not mastering the sensor bus)\n");
                        }
                    }
                    b'r' | b'R' => enumerate::report(p, &last),
                    b'd' | b'D' => rf.status.dump(p),
                    b'm' | b'M' => {
                        let next = mach.current().toggled();
                        mach.switch(p, next); // acquire/release the sensor bus for the new mode
                        uart::puts(p, "mode -> ");
                        uart::puts(p, mach.current().name());
                        uart::putc(p, b'\n');
                        if mach.is_sensing() {
                            last = enumerate::scan(p);
                            enumerate::report(p, &last);
                            rf.status = status::Status::from_scan(model, &last);
                        } else {
                            last = enumerate::Scan::absent();
                            uart::puts(p, "  sensor bus released (SoM owns it)\n");
                            rf.status = status::Status::booted(model);
                        }
                    }
                    b'\r' | b'\n' => {}
                    _ => uart::puts(p, "  (s=scan r=report d=debug m=mode)\n"),
                }
            }
        }
    }

    // Production (console off): state lives in the register file, served over the eUSCI_B1 I2C-slave
    // surface. Poll it forever — this is the node's whole job until wake/INT logic lands.
    #[cfg(not(feature = "console"))]
    {
        // Production: the boot-default mode is active (its entry action already ran in `boot`);
        // runtime mode switches arrive over the register interface (TODO: SoM-facing mode register).
        let _ = (&last, &mach);
        loop {
            stem::poll(p, &mut slave, &mut rf);
        }
    }
}

/// FR247x model name for the banner (no core::fmt on this budget).
#[cfg(all(feature = "fr247x", feature = "console"))]
fn model_name(m: model::Model) -> &'static str {
    match m {
        model::Model::Fr2476 => "FR2476",
        model::Model::Fr2475 => "FR2475",
        model::Model::Fr2433 => "FR2433",
        model::Model::Unknown(_) => "UNKNOWN",
    }
}

/// FR24xx (FR2433) single-I²C slave-only node — bring-up TODO. Idle scaffold for now: model detect,
/// then idle. Real bring-up (FR2433 clock/UART/USCI-slave) lands with an FR2433 board on the bench.
#[cfg(feature = "fr24xx")]
fn run_fr24xx(p: &Peripherals) -> ! {
    let _ = model::detect();
    // TODO: FR2433 bring-up — clock, UART banner, USCI_B0 I2C SLAVE surface (regmap), ADC/GPIO.
    loop {
        let _ = p;
        msp430::asm::nop();
    }
}
