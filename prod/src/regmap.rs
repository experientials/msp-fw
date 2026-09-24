//! I²C-slave register map — the Stembus Expander's **contract with the SoM master**.
//!
//! DERIVED (not invented) from the canonical specs — keep those authoritative, this is their
//! machine-checkable encoding:
//!   • ../I2C-API.md — the expander API: PCA9698 emulation + Thepia voltage/threshold extensions.
//!   • PCA9698 datasheet Table 3 "Register summary" — ../datasheets/reg-defs-{1,2}.png (addresses
//!     below verified pixel-for-pixel against it).
//!   • ../STEM-MSG.md — the 1-wire notification side (INT/MSG); referenced where the register map
//!     drives a message, but that transport lives elsewhere (not this module).
//!
//! The expander emulates a PCA9698 GPIO expander: five 8-bit GPIO banks addressed through a command
//! (index) register the master writes first, then reads/writes data. This module defines the address
//! space + a decoder; the eUSCI_B I²C-slave ISR (TODO in main.rs) dispatches on [`RegSel`] and
//! touches the MSP430 port registers. No behavior here yet — this is the wire contract only.

#![allow(dead_code)] // Contract definitions; the slave ISR that consumes them is not written yet.

/// Command-register (index) decode. After the slave address, the master writes one command byte:
/// the low 6 bits are the register pointer (I2C-API.md §7.3); the Auto-Increment flag makes the
/// low 3 bits (the bank index within a group) step after each data byte, for sequential bank access.
pub const CMD_REG_MASK: u8 = 0x3F;
/// Auto-Increment flag. PCA9698 §7.3 places AI in the command byte alongside the 6-bit pointer.
/// TODO(confirm-on-doc): the two register-summary images fix the pointer (D5:D0) but not AI's bit
/// position — confirm 0x80 vs 0x40 against the PCA9698 §7.3 command-register bit diagram before the
/// ISR relies on it. Encoded as the MSB per the common PCA9698 convention.
pub const CMD_AI: u8 = 0x80;

/// Addressable GPIO banks per register group. The Thepia plan EXTENDS the PCA9698 (native 5 banks /
/// 40 pins) to the **full 8 ports / 64 pins** by using its reserved bank slots 5–7 — I2C-API.md:
/// *"emulation of a PCA9698 expanded to support 8 pin ports… the standard register allocation has
/// reserved entries for bank 5–8. The firmware will support these."* So banks 0–7 are all valid in
/// the map; how many are **physically backed** depends on the part's I/O count (see
/// `model::PinMap.bank_count` — FR2433 backs ~3, FR2476 more; unbacked banks are firmware shadow).
pub const BANKS: u8 = 8;

// ── 8-port register groups (bank = low 3 bits; banks 0..=7, PCA9698-compatible + extended) ──
pub const IP_BASE: u8 = 0x00; // Input Port      — READ-ONLY, sourced directly from PxIN
pub const OP_BASE: u8 = 0x08; // Output Port     — R/W, driven directly to PxOUT
pub const PI_BASE: u8 = 0x10; // Polarity Invert — R/W, FIRMWARE-maintained (no HW), XORed into IP reads
pub const IOC_BASE: u8 = 0x18; // I/O Config     — R/W, direction. PCA: 1=input, 0=output —
                               //                  the INVERSE of MSP430 PxDIR (1=output) → PxDIR = !IOC
pub const MSK_BASE: u8 = 0x20; // Mask Interrupt — R/W, FIRMWARE-maintained (no HW); 0=unmasked, 1=masked

// ── 1-bank (single) registers ──
pub const OUTCONF: u8 = 0x28; // Output structure configuration
pub const ALLBNK: u8 = 0x29; // All-bank control (write hits every bank at once)
pub const MODE: u8 = 0x2A; // PCA9698 mode selection

// ── Thepia extensions (I2C-API.md "Voltage and more as Misc. Register") ──
pub const INIT_CODE: u8 = 0x2B; // Startup/reset init function selector; persisted to FRAM
pub const VSOM_VOLTAGE: u8 = 0x2C; // READ: voltage at P1.6 (ADC), the SoM rail
pub const CHARGE_VOLTAGE: u8 = 0x2D; // READ: voltage at P1.7 (ADC), the charge rail
pub const VSOM_THRESHOLD: u8 = 0x2E; // WRITE 16-bit: up to 3 thresholds → emits a MSG when crossed
pub const BAUD_RATE: u8 = 0x2F; // "free" — optionally set the STEM-MSG baud from the master
pub const EXT_BASE: u8 = 0x30; // 0x30..=0x3F extended / custom register window
pub const EXT_END: u8 = 0x3F;

/// Number of VSOM voltage thresholds (I2C-API.md 0x2Eh; STEM-MSG threshold index 0..=2).
pub const VSOM_THRESHOLDS: u8 = 3;

// ── Debug / status registers (Thepia extension, inside the 0x30–0x3F custom window) ──
// ADDITIVE: they never touch the PCA9698-compatible core (0x00–0x2A), so a stock PCA9698 driver
// still sees a normal expander. Read-only. Served by ONE state model (prod/src/status.rs) through
// TWO readers: the eUSCI_B1 I2C-slave ISR (the SoM, later) and the bench UART dump (now).
pub const DBG_IFACE: u8 = 0x30; // magic identifying the debug interface → DBG_IFACE_MAGIC
pub const DBG_MODEL_L: u8 = 0x31; // detected Device ID, low byte  (FR2476 = 0x2A)
pub const DBG_MODEL_H: u8 = 0x32; // detected Device ID, high byte (FR2476 = 0x83)
pub const DBG_BUILD_0: u8 = 0x33; // 32-bit build id (FNV-1a of PROD_BUILD), bytes 0..3 (LSB first)
pub const DBG_BUILD_1: u8 = 0x34;
pub const DBG_BUILD_2: u8 = 0x35;
pub const DBG_BUILD_3: u8 = 0x36;
pub const DBG_STATUS: u8 = 0x37; // status flag bits (see status::flags)
pub const DBG_DEV_COUNT: u8 = 0x38; // I²C devices found on the sensor bus (last scan)
pub const DBG_KNOWN_PRESENT: u8 = 0x39; // bitmap over devices::KNOWN (bit i = KNOWN[i] present)
pub const DBG_FAULT: u8 = 0x3A; // fault flags (reserved bits for now)
// 0x3B..=0x3F reserved (read 0).

/// Value returned at [`DBG_IFACE`] — lets a master detect the Thepia debug interface behind the
/// PCA9698 facade. (`0xD0` = "debug regs, v0"; bump on an incompatible debug-layout change.)
pub const DBG_IFACE_MAGIC: u8 = 0xD0;

/// A decoded command-register selection. The bank field (0..=7) is carried for the banked groups.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegSel {
    InputPort(u8),      // IP_BASE  + bank — read PxIN (with polarity inversion applied)
    OutputPort(u8),     // OP_BASE  + bank — R/W PxOUT
    PolarityInv(u8),    // PI_BASE  + bank — R/W firmware shadow
    IoConfig(u8),       // IOC_BASE + bank — R/W direction (inverse of PxDIR)
    IntMask(u8),        // MSK_BASE + bank — R/W firmware shadow
    OutConf,
    AllBank,
    Mode,
    InitCode,
    VsomVoltage,
    ChargeVoltage,
    VsomThreshold,
    BaudRate,
    Extended(u8),       // 0x30..=0x3F, carries the offset from EXT_BASE
    Reserved(u8),       // an unused index (e.g. a bank slot ≥ BANKS) — carries the raw reg byte
}

/// Decode a raw command byte (AI already stripped by the caller via `CMD_REG_MASK`) into a [`RegSel`].
/// With the 8-port map every bank slot 0..=7 is valid; a slot ≥ `BANKS` surfaces as `Reserved`.
pub const fn decode(reg: u8) -> RegSel {
    let bank = reg & 0x07;
    let valid_bank = bank < BANKS;
    match reg {
        0x00..=0x07 if valid_bank => RegSel::InputPort(bank),
        0x08..=0x0F if valid_bank => RegSel::OutputPort(bank),
        0x10..=0x17 if valid_bank => RegSel::PolarityInv(bank),
        0x18..=0x1F if valid_bank => RegSel::IoConfig(bank),
        0x20..=0x27 if valid_bank => RegSel::IntMask(bank),
        OUTCONF => RegSel::OutConf,
        ALLBNK => RegSel::AllBank,
        MODE => RegSel::Mode,
        INIT_CODE => RegSel::InitCode,
        VSOM_VOLTAGE => RegSel::VsomVoltage,
        CHARGE_VOLTAGE => RegSel::ChargeVoltage,
        VSOM_THRESHOLD => RegSel::VsomThreshold,
        BAUD_RATE => RegSel::BaudRate,
        EXT_BASE..=EXT_END => RegSel::Extended(reg - EXT_BASE),
        _ => RegSel::Reserved(reg),
    }
}

/// Next command index under Auto-Increment: step the bank (low 3 bits) within the current group,
/// wrapping at 8 back to the group base — matches the PCA9698 "3 LSBs auto-incremented" behavior.
pub const fn next_ai(reg: u8) -> u8 {
    (reg & !0x07) | ((reg + 1) & 0x07)
}

/// Firmware-shadowed per-bank state the emulation must keep (PI and MSK have no MSP430 HW backing;
/// IP/OP/IOC drive the native port registers directly). One byte per active bank.
#[derive(Clone, Copy, Default)]
pub struct ShadowState {
    pub polarity: [u8; BANKS as usize], // PI: XOR mask applied to IP reads
    pub int_mask: [u8; BANKS as usize], // MSK: 1 = do not raise INT for that input pin
    pub init_code: u8,                  // 0x2B, persisted
    pub vsom_threshold: [u16; VSOM_THRESHOLDS as usize], // 0x2E
}
