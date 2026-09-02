//! POST runner: bus scan (surfaces anything wired) + a registry of KNOWN devices checked by
//! presence and WHO_AM_I. Add a device with one line in `DEVICES`. Reported over UART, with a
//! pass/fail glyph on the LED matrix when present.

use crate::{i2c, is31, uart};
use msp430fr2476::Peripherals;

// 8x8 status glyphs (orientation may be mirrored/rotated on real hardware; fix once seen).
const GLYPH_CHECK: [u8; 8] = [0x01, 0x03, 0x06, 0x8C, 0xD8, 0x70, 0x20, 0x00];
const GLYPH_CROSS: [u8; 8] = [0x00, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x00];

/// A known device: address, and an optional WHO_AM_I register + expected value.
/// `id_reg == NO_ID` means presence-only (no readable ID register).
struct Dev {
    name: &'static str,
    addr: u8,
    id_reg: u8,
    id_val: u8,
}
const NO_ID: u8 = 0xFF;

// The expected device set. Extend this to teach diag about a new part (see DESIGN.md).
// Later this becomes a per-product (Bob/Ziloo) manifest for config-drift checking.
static DEVICES: &[Dev] = &[
    Dev { name: "SSD1306 OLED  ", addr: 0x3C, id_reg: NO_ID, id_val: 0x00 },
    Dev { name: "VL53L0X ToF   ", addr: 0x29, id_reg: 0xC0, id_val: 0xEE },
    Dev { name: "APDS-9960     ", addr: 0x39, id_reg: 0x92, id_val: 0xAB },
    // 6DOF IMU 13 Click (MIKROE-4228) is an mCube MC6470 eCompass — accel + magnetometer as two
    // I2C sub-addresses (0x4C accel, 0x0C mag), NOT an ICM-42605. Presence-only for now; a WHO_AM_I
    // check needs the chip-ID registers verified against the datasheet (0x4C/0x0C both ACK the scan).
    Dev { name: "MC6470 accel  ", addr: 0x4C, id_reg: NO_ID, id_val: 0x00 },
    Dev { name: "MC6470 mag    ", addr: 0x0C, id_reg: NO_ID, id_val: 0x00 },
    Dev { name: "IS31FL3730 LED", addr: 0x60, id_reg: NO_ID, id_val: 0x00 },
];

/// Is this address one of our expected devices? A scan hit that isn't = config drift.
fn is_expected(addr: u8) -> bool {
    DEVICES.iter().any(|d| d.addr == addr)
}

/// Scan the bus; return the count of **unexpected** addresses (ACKed but not in the registry).
fn scan(p: &Peripherals) -> u16 {
    // Idle bus level (P1IN reflects the real pad even when muxed to UCB0):
    //   L = no pull-ups reaching us -> EB unpowered or SDA/SCL not connected.
    //   H = healthy idle bus.
    let lv = p.p1.p1in().read().bits();
    uart::puts(p, "bus idle SDA=");
    uart::putc(p, if lv & 0x04 != 0 { b'H' } else { b'L' }); // P1.2
    uart::puts(p, " SCL=");
    uart::putc(p, if lv & 0x08 != 0 { b'H' } else { b'L' }); // P1.3
    if lv & 0x04 == 0 {
        uart::puts(
            p,
            if i2c::recover(p) {
                " (recovered)"
            } else {
                " (recover FAILED - short?)"
            },
        );
    }
    uart::puts(p, "\nI2C scan:");
    let mut found = 0u16;
    let mut unexpected = 0u16;
    let mut a = 0x08u8;
    while a <= 0x77 {
        if i2c::probe(p, a) {
            uart::puts(p, " 0x");
            uart::hex8(p, a);
            if !is_expected(a) {
                uart::puts(p, "?"); // ACKs but not a known device = config drift
                unexpected += 1;
            }
            found += 1;
        }
        a += 1;
    }
    if found == 0 {
        uart::puts(p, " (none)");
    }
    if unexpected > 0 {
        uart::puts(p, "  (");
        uart::dec(p, unexpected);
        uart::puts(p, " UNEXPECTED)");
    }
    uart::puts(p, "\n");
    unexpected
}

/// Check every known device. Returns (present, total, faults):
///   present = devices that ACKed their address
///   total   = size of the registry
///   faults  = present-but-broken (wrong WHO_AM_I, or the ID read failed)
/// `faults` is the meaningful bench verdict: green when everything that answered is healthy.
/// (Missing devices become faults only once we validate against a per-product manifest — obj 2.)
fn check_devices(p: &Peripherals) -> (u16, u16, u16) {
    let total = DEVICES.len() as u16;
    let mut present = 0u16;
    let mut faults = 0u16;
    uart::puts(p, "devices:\n");
    for d in DEVICES {
        uart::puts(p, "  ");
        uart::puts(p, d.name);
        uart::puts(p, " @0x");
        uart::hex8(p, d.addr);
        uart::puts(p, ": ");
        if !i2c::probe(p, d.addr) {
            uart::puts(p, "absent\n");
            continue;
        }
        present += 1;
        if d.id_reg == NO_ID {
            uart::puts(p, "present\n");
            continue;
        }
        let mut id = [0u8; 1];
        if !i2c::read_reg(p, d.addr, d.id_reg, &mut id) {
            uart::puts(p, "present, id-read FAILED\n");
            faults += 1;
            continue;
        }
        uart::puts(p, "id=");
        uart::hex8(p, id[0]);
        if id[0] == d.id_val {
            uart::puts(p, " OK\n");
        } else {
            uart::puts(p, " MISMATCH want=");
            uart::hex8(p, d.id_val);
            uart::puts(p, "\n");
            faults += 1;
        }
    }
    (present, total, faults)
}

/// A one-shot diagnostic test: prints its own section and returns a verdict. **Reorder `TESTS` to
/// change the run order**; a future button menu / script selects a subset. This is diag's "bag of
/// tests" — distinct from the continuous `sched::Task`s (radar/proximity), which run over time
/// rather than once-to-a-verdict.
enum Outcome {
    Pass,
    Fail,
    /// Device absent / not applicable — reported, does NOT fail the verdict. This is how an
    /// optional device (e.g. the OLED on the product) is handled.
    Skip,
}

struct Test {
    name: &'static str,
    run: fn(&Peripherals) -> Outcome,
}

// The ordered test bag — reorder these lines to change what runs when.
static TESTS: &[Test] = &[
    Test { name: "bus scan", run: t_scan },
    Test { name: "device inventory", run: t_devices },
    Test { name: "MC6470 gravity", run: t_gravity },
];

/// Bus scan + config-drift: Fail if any unexpected address ACKed.
fn t_scan(p: &Peripherals) -> Outcome {
    if scan(p) > 0 {
        Outcome::Fail
    } else {
        Outcome::Pass
    }
}

/// Device inventory: presence + WHO_AM_I over the registry. Fail if anything present is faulty.
fn t_devices(p: &Peripherals) -> Outcome {
    let (present, total, faults) = check_devices(p);
    let missing = total - present;
    uart::puts(p, "  inventory: ");
    uart::dec(p, present);
    uart::puts(p, "/");
    uart::dec(p, total);
    uart::puts(p, " present");
    if missing > 0 {
        uart::puts(p, ", ");
        uart::dec(p, missing);
        uart::puts(p, " missing");
    }
    uart::puts(p, "\n");
    if faults > 0 {
        Outcome::Fail
    } else {
        Outcome::Pass
    }
}

/// MC6470 accel gravity-sanity (|a| ~ 1 g). Skip when absent (optional device), else Pass/Fail.
fn t_gravity(p: &Peripherals) -> Outcome {
    if !i2c::probe(p, crate::mc6470::ACCEL_ADDR) {
        uart::puts(p, "  MC6470 accel: absent (skip)\n");
        return Outcome::Skip;
    }
    match crate::mc6470::gravity_check(p) {
        Some(true) => Outcome::Pass,
        Some(false) => Outcome::Fail,
        None => Outcome::Skip, // not woken yet (cold-boot latency) — self-corrects next pass
    }
}

/// Run the POST test bag once. Returns `(ok, present, total)` for the caller to publish to the UI
/// (the OLED is owned by `UiTask` now, not drawn here). `ok` = no test FAILED (skips don't fail).
pub fn run(p: &Peripherals) -> (bool, u16, u16) {
    // The build stamp rides on every banner (not just the boot line) so the verifier — or a
    // technician who attached the monitor late — can confirm which firmware is running within
    // one ~3 s cycle. `env!` resolves it from build.rs's DIAG_BUILD at compile time.
    uart::puts(p, "\n=== bob-929 diag POST · build ");
    uart::puts(p, env!("DIAG_BUILD"));
    uart::puts(p, " ===\n");
    // Run the ordered test bag; each test prints its own section, here we tally the verdicts.
    // `missing`/`UNEXPECTED`/`FAULTY` detail is printed inside the individual tests. Continuous
    // radar/proximity sensing is NOT here — it lives in sched::Tasks.
    let (mut pass, mut fail, mut skip) = (0u16, 0u16, 0u16);
    for t in TESTS {
        uart::puts(p, "· "); // section header names the test (the registry identifier a menu selects)
        uart::puts(p, t.name);
        uart::puts(p, "\n");
        match (t.run)(p) {
            Outcome::Pass => pass += 1,
            Outcome::Fail => fail += 1,
            Outcome::Skip => skip += 1,
        }
    }

    uart::puts(p, "summary: ");
    uart::dec(p, pass);
    uart::puts(p, " passed");
    if fail > 0 {
        uart::puts(p, ", ");
        uart::dec(p, fail);
        uart::puts(p, " FAILED");
    }
    if skip > 0 {
        uart::puts(p, ", ");
        uart::dec(p, skip);
        uart::puts(p, " skipped");
    }
    uart::puts(p, "\n");

    // LED-matrix verdict (the OLED is rendered by UiTask from the returned verdict). OK = no test
    // FAILED (skips don't fail). The bus-level probe after the write catches a wedged bus.
    let ok = fail == 0;
    if is31::present(p) {
        let i = is31::init(p);
        let s = is31::show(p, if ok { &GLYPH_CHECK } else { &GLYPH_CROSS });
        uart::puts(p, "  IS31 init ");
        uart::puts(p, if i { "OK" } else { "FAIL" });
        uart::puts(p, " show ");
        uart::puts(p, if s { "OK" } else { "FAIL" });
        uart::puts(p, "\n");
        bus_lvl(p, "  bus after IS31: ");
    }
    (ok, pass, pass + fail + skip)
}

/// One-line SDA/SCL pad level (P1IN reflects the real line even when the pins are muxed to eUSCI).
/// Used to localize which write leaves the bus low.
fn bus_lvl(p: &Peripherals, label: &str) {
    let lv = p.p1.p1in().read().bits();
    uart::puts(p, label);
    uart::putc(p, if lv & 0x04 != 0 { b'H' } else { b'L' }); // SDA / P1.2
    uart::putc(p, b'/');
    uart::putc(p, if lv & 0x08 != 0 { b'H' } else { b'L' }); // SCL / P1.3
    uart::puts(p, "\n");
}
