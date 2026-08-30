//! POST runner: bus scan (surfaces anything wired) + a registry of KNOWN devices checked by
//! presence and WHO_AM_I. Add a device with one line in `DEVICES`. Reported over UART, with a
//! pass/fail glyph on the LED matrix when present.

use crate::{i2c, is31, oled, uart};
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
    Dev { name: "ICM-42605 IMU ", addr: 0x68, id_reg: 0x75, id_val: 0x42 },
    Dev { name: "IS31FL3730 LED", addr: 0x60, id_reg: NO_ID, id_val: 0x00 },
];

fn scan(p: &Peripherals) {
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
    let mut a = 0x08u8;
    while a <= 0x77 {
        if i2c::probe(p, a) {
            uart::puts(p, " 0x");
            uart::hex8(p, a);
            found += 1;
        }
        a += 1;
    }
    if found == 0 {
        uart::puts(p, " (none)");
    }
    uart::puts(p, "\n");
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

pub fn run(p: &Peripherals) {
    // The build stamp rides on every banner (not just the boot line) so the verifier — or a
    // technician who attached the monitor late — can confirm which firmware is running within
    // one ~3 s cycle. `env!` resolves it from build.rs's DIAG_BUILD at compile time.
    uart::puts(p, "\n=== bob-929 diag POST · build ");
    uart::puts(p, env!("DIAG_BUILD"));
    uart::puts(p, " ===\n");
    scan(p);
    let (present, total, faults) = check_devices(p);

    uart::puts(p, "summary: ");
    uart::dec(p, present);
    uart::puts(p, "/");
    uart::dec(p, total);
    uart::puts(p, " present");
    if faults > 0 {
        uart::puts(p, ", ");
        uart::dec(p, faults);
        uart::puts(p, " FAULTY");
    }
    uart::puts(p, "\n");

    // Visual verdict on EVERY display present, but ALSO self-check: report each write's PASS/FAIL
    // and probe the bus level right after, so we can see which write (if any) leaves the bus
    // wedged. OK = nothing that answered is faulty. UART stays authoritative.
    let ok = faults == 0;
    uart::puts(p, "display:\n");
    uart::puts(p, "  OLED ");
    uart::puts(
        p,
        match oled::show_status(p, ok, present, total) {
            oled::Status::Ok => "rendered",
            oled::Status::InitFail => "init FAILED",
            oled::Status::FlushFail => "flush FAILED",
        },
    );
    uart::puts(p, "\n");
    bus_lvl(p, "  bus after OLED: ");
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
