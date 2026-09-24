//! eUSCI_B1 I²C-**SLAVE** — the Stem-bus surface the SoM (and other masters) read/write our register
//! interface on. This is the transport regmap.rs flags as the TODO: it dispatches a master's
//! command-pointer + read/write bytes onto [`regmap`] via [`RegFile`].
//!
//! **Polled, no ISR** (matching prod's single-threaded model). eUSCI_B auto-clock-stretches SCL until
//! firmware services the TX/RX buffer, so [`poll`] just needs to be called often in the main loop;
//! latency shows up as clock-stretch on the master, not lost data. (A long CPU-bound stretch — e.g.
//! the console VL53L0X burst — will stall a concurrent master until it returns; a production build
//! would service this more aggressively. Documented, acceptable for bring-up.)
//!
//! **What's backed today:** the debug/identity window (0x30–0x3F) is REAL — served by
//! [`Status::read_reg`], so the SoM reads exactly what the bench UART dump shows (identity, build id,
//! status, dev-count, fault). The PCA9698 GPIO banks (IP/OP/IOC/PI/MSK) are a coherent **firmware
//! shadow** — writes persist and read back — but have **no physical pin backing yet** (that needs the
//! bank↔port pin map in `model::PinMap`; IP reads 0, OP/IOC/PI/MSK are shadow). Voltages (0x2C/0x2D)
//! read 0 until the rail ADC lands. This is the wire contract, live, minus the not-yet-defined pins.
//!
//! Pins: P3.2 = UCB1SDA, P3.6 = UCB1SCL (`connections.toml` stem_sda/stem_scl).

use crate::regmap::{self, ShadowState};
use crate::status::Status;
use crate::pac::Peripherals;

/// eUSCI_B1 slave address. PCA9698 parts answer a strap-selected address in 0x20–0x27; we default to
/// the group base. TODO(confirm-on-doc): pin this against I2C-API.md's canonical Stem-node address.
pub const STEM_ADDR: u16 = 0x20;

// ── CTLW0 / I2COA0 / IFG bit definitions (eUSCI_B, slau445 §24) ──
const UCSWRST: u16 = 0x0001;
const UCMODE_3: u16 = 0x0600; // I2C mode
const UCSYNC: u16 = 0x0100;
const UCOAEN: u16 = 0x0400; // I2COA0: own-address enable
const UCRXIFG0: u16 = 0x0001;
const UCTXIFG0: u16 = 0x0002;
const UCSTTIFG: u16 = 0x0004; // START matching our own address
const UCSTPIFG: u16 = 0x0008;
const UCNACKIFG: u16 = 0x0020;

const SDA_BIT: u8 = 0x04; // P3.2
const SCL_BIT: u8 = 0x40; // P3.6

/// Configure eUSCI_B1 as an I²C slave at [`STEM_ADDR`] and route P3.2/P3.6 to it. Independent of the
/// eUSCI_B0 sensor-master bus (different peripheral + pins), so it never disturbs enumeration.
pub fn init(p: &Peripherals) {
    // Route P3.2/P3.6 to the primary module function (SEL1=0, SEL0=1 = UCB1SDA/UCB1SCL).
    p.p3.p3sel1().modify(|r, w| unsafe { w.bits(r.bits() & !(SDA_BIT | SCL_BIT)) });
    p.p3.p3sel0().modify(|r, w| unsafe { w.bits(r.bits() | (SDA_BIT | SCL_BIT)) });

    p.e_usci_b1.ucb1ctlw0().write(|w| unsafe { w.bits(UCSWRST) }); // hold in reset
    p.e_usci_b1
        .ucb1ctlw0()
        .modify(|r, w| unsafe { w.bits(r.bits() | UCMODE_3 | UCSYNC) }); // I2C SLAVE (no UCMST)
    p.e_usci_b1.ucb1i2coa0().write(|w| unsafe { w.bits(STEM_ADDR | UCOAEN) });
    p.e_usci_b1.ucb1ctlw0().modify(|r, w| unsafe { w.bits(r.bits() & !UCSWRST) }); // release
    p.e_usci_b1.ucb1ifg().write(|w| unsafe { w.bits(0) }); // clear stale flags
}

/// The register file behind the slave: the debug/identity [`Status`] (real) plus the PCA9698 GPIO
/// shadow (coherent, not yet pin-backed). One `read`/`write` maps a decoded [`regmap::RegSel`] to it.
pub struct RegFile {
    /// Debug/identity/status snapshot (0x30–0x3F). Replace on each rescan so the SoM sees fresh state.
    pub status: Status,
    shadow: ShadowState,
    op: [u8; regmap::BANKS as usize],  // Output Port (0x08–0x0F)
    ioc: [u8; regmap::BANKS as usize], // I/O Config / direction (0x18–0x1F)
}

impl RegFile {
    pub fn new(status: Status) -> Self {
        Self {
            status,
            shadow: ShadowState::default(),
            op: [0; regmap::BANKS as usize],
            ioc: [0; regmap::BANKS as usize],
        }
    }

    /// Read the byte a master gets for command index `reg`.
    fn read(&self, reg: u8) -> u8 {
        match regmap::decode(reg) {
            // Debug/identity window — the real state (identical to the bench UART dump).
            regmap::RegSel::Extended(_) => self.status.read_reg(reg),
            // GPIO banks — firmware shadow (no physical pins yet). IP has no input backing → 0.
            regmap::RegSel::InputPort(_) => 0,
            regmap::RegSel::OutputPort(b) => self.op[b as usize],
            regmap::RegSel::IoConfig(b) => self.ioc[b as usize],
            regmap::RegSel::PolarityInv(b) => self.shadow.polarity[b as usize],
            regmap::RegSel::IntMask(b) => self.shadow.int_mask[b as usize],
            regmap::RegSel::InitCode => self.shadow.init_code,
            // Voltages need the rail ADC (not wired); everything else reads 0 for now.
            _ => 0,
        }
    }

    /// Apply a master write of `val` to command index `reg`. Read-only regions are ignored.
    fn write(&mut self, reg: u8, val: u8) {
        match regmap::decode(reg) {
            regmap::RegSel::OutputPort(b) => self.op[b as usize] = val,
            regmap::RegSel::IoConfig(b) => self.ioc[b as usize] = val,
            regmap::RegSel::PolarityInv(b) => self.shadow.polarity[b as usize] = val,
            regmap::RegSel::IntMask(b) => self.shadow.int_mask[b as usize] = val,
            regmap::RegSel::InitCode => self.shadow.init_code = val,
            // InputPort / debug window / mode regs: read-only or not-yet-backed → ignore.
            _ => {}
        }
    }
}

/// Per-transaction slave state: the current command pointer, the auto-increment flag, and whether the
/// next received byte is the command byte (first after a START) vs write data.
pub struct Slave {
    reg_ptr: u8,
    ai: bool,
    first_byte: bool,
}

impl Slave {
    pub const fn new() -> Self {
        Self {
            reg_ptr: 0,
            ai: false,
            first_byte: true,
        }
    }
}

/// Service any pending eUSCI_B1 slave events (non-blocking). Call frequently from the main loop.
///
/// Protocol (PCA9698, I2C-API.md §7.3): after our address, the master's FIRST written byte is the
/// command register — low 6 bits = pointer, [`regmap::CMD_AI`] = auto-increment. Subsequent written
/// bytes go to the pointed register (stepping the bank under AI); a master read returns the pointed
/// register (also stepping under AI).
pub fn poll(p: &Peripherals, s: &mut Slave, rf: &mut RegFile) {
    let ifg = p.e_usci_b1.ucb1ifg().read().bits();

    if ifg & UCSTTIFG != 0 {
        s.first_byte = true; // next RX byte is the command; a read keeps the existing pointer
        clear(p, UCSTTIFG);
    }
    if ifg & UCNACKIFG != 0 {
        clear(p, UCNACKIFG);
    }
    if ifg & UCRXIFG0 != 0 {
        let b = p.e_usci_b1.ucb1rxbuf().read().bits() as u8; // read clears UCRXIFG
        if s.first_byte {
            s.reg_ptr = b & regmap::CMD_REG_MASK;
            s.ai = b & regmap::CMD_AI != 0;
            s.first_byte = false;
        } else {
            rf.write(s.reg_ptr, b);
            if s.ai {
                s.reg_ptr = regmap::next_ai(s.reg_ptr);
            }
        }
    }
    if ifg & UCTXIFG0 != 0 {
        let v = rf.read(s.reg_ptr);
        p.e_usci_b1.ucb1txbuf().write(|w| unsafe { w.bits(v as u16) }); // write clears UCTXIFG
        if s.ai {
            s.reg_ptr = regmap::next_ai(s.reg_ptr);
        }
    }
    if ifg & UCSTPIFG != 0 {
        clear(p, UCSTPIFG);
        s.first_byte = true;
    }
}

fn clear(p: &Peripherals, bit: u16) {
    p.e_usci_b1.ucb1ifg().modify(|r, w| unsafe { w.bits(r.bits() & !bit) });
}
