//! APDS-9960 — diag exercises the **proximity** engine as a live health check, beyond the
//! WHO_AM_I presence entry in the device registry (`diag.rs`). Enable PON+PEN once, then read
//! PDATA; the sensor free-runs, so each read is non-blocking — a clean fit for a cooperative task
//! (see `tasks::ProximityTask`). Shared I²C bus: 0x39, SDA P1.2 / SCL P1.3. INT is left unwired
//! (polled); wiring it is the `sensor_int` entry in ../../crates/bsp/connections.toml when we want it.
//!
//! Gesture (GMODE + the 4-photodiode FIFO) is deliberately not implemented — proximity alone
//! proves the analog front-end and the LED drive are alive, which is what a POST needs.

use crate::i2c;
use msp430fr2476::Peripherals;

pub const ADDR: u8 = 0x39;

const ENABLE: u8 = 0x80; // PON | PEN | ...
const PPULSE: u8 = 0x8E; // proximity pulse length + count
const STATUS: u8 = 0x93; // PVALID in bit 1
const PDATA: u8 = 0x9C; // proximity data, 0 (far) .. 255 (near)

const PON: u8 = 0x01; // power on
const PEN: u8 = 0x04; // proximity enable
const PVALID: u8 = 0x02; // STATUS: a proximity sample is ready

/// Does the sensor ACK its address? (bounded probe — used to tell "absent" from "not-ready".)
pub fn present(p: &Peripherals) -> bool {
    i2c::probe(p, ADDR)
}

/// Power on + enable the proximity engine. Returns false if the device doesn't ACK (absent / bus
/// fault) — bounded, never hangs. Idempotent, so a task can retry it until a plugged-in sensor
/// comes up.
pub fn enable(p: &Peripherals) -> bool {
    // 16 µs pulses ×8 (0x87) for a usable near-field signal; the POR default is one weak pulse.
    if !i2c::write(p, ADDR, &[PPULSE, 0x87]) {
        return false;
    }
    i2c::write(p, ADDR, &[ENABLE, PON | PEN])
}

/// Latest proximity byte (0 = far .. 255 = near). `None` if the read NACKs or the sensor reports
/// the sample isn't valid yet (the cycle right after `enable`).
pub fn proximity(p: &Peripherals) -> Option<u8> {
    let mut st = [0u8; 1];
    if !i2c::read_reg(p, ADDR, STATUS, &mut st) || st[0] & PVALID == 0 {
        return None;
    }
    let mut d = [0u8; 1];
    if !i2c::read_reg(p, ADDR, PDATA, &mut d) {
        return None;
    }
    Some(d[0])
}
