//! ADC bring-up for **measured** board stats: supply rail (DVCC, mV) and die temperature (°C).
//!
//! These are the MCU-self "is the board actually healthy" numbers that presence/ID checks can't
//! give. Both come from the 12-bit SAR ADC's internal channels — no external wiring:
//!   - **DVCC** via the ratio trick (datasheet §PMM, Eq. 1): sample the internal 1.5-V reference
//!     (channel A13) using **DVCC as the ADC reference**, so `DVCC = full-scale × 1.5 V ÷ result`.
//!     No accurate external reference needed — the known 1.5 V is the input, the unknown rail is
//!     the reference.
//!   - **Die temperature** via the on-chip sensor (channel A12) against the internal 1.5-V
//!     reference, two-point interpolated through the factory calibration stored in the TLV device
//!     descriptor (30 °C / 105 °C at 1.5 V). Approximate (±a few °C — ADC gain/offset cal is not
//!     applied); a health indicator, not a precision thermometer.
//!
//! Reference control is in **PMMCTL2** (`INTREFEN`/`REFVSEL`) — note that register is NOT
//! password-protected (unlike PMMCTL0). Channels/reference/resolution verified against the FR2476
//! datasheet (Table 9-19 channels, Table 9-30 TLV cal) and the vendored PAC.

use msp430fr2476::Peripherals;

// TLV factory cal: ADC readings of the temperature sensor at the internal 1.5-V reference
// (datasheet Table 9-30, ADC calibration block tag 0x11 @ 0x1A14). Read-only info memory.
const CAL_ADC_15T30: *const u16 = 0x1A1A as *const u16; //  30 °C reading @ 1.5 V ref
const CAL_ADC_15T105: *const u16 = 0x1A1C as *const u16; // 105 °C reading @ 1.5 V ref

const FULL_SCALE: u32 = 4095; // 12-bit result (0..4095)
const VREF_MV: u32 = 1500; // internal shared reference = 1.5 V

/// Enable the internal 1.5-V reference and configure the ADC core (one-shot, at boot). Leaves the
/// ADC on so `dvcc_mv`/`die_temp_c` are cheap; call before either. Reference settle happens during
/// the boot banner prints, so no explicit delay beyond a bounded ready-poll here.
pub fn init(p: &Peripherals) {
    // Internal reference: 1.5 V (REFVSEL=0), enabled. Also enable the on-chip temperature sensor
    // (TSENSOREN) — it has its own power gate separate from the reference; without it the temp
    // channel reads ~0. PMMCTL2 needs no password (unlike PMMCTL0).
    p.pmm
        .pmmctl2()
        .modify(|_, w| w.refvsel().refvsel_0().intrefen().set_bit().tsensoren().set_bit());
    // Bounded wait for the reference generator to become active.
    for _ in 0..4000 {
        if p.pmm.pmmctl2().read().refgenact().bit_is_set() {
            break;
        }
    }
    // 12-bit, sampling-timer (pulse) mode, long sample-and-hold (the temp sensor is high-impedance,
    // so it needs a generous S&H window — 256 ADCCLK cycles), core on. Clock = MODCLK (ADCSSEL
    // default, auto-requested by the ADC); single channel, single conversion (ADCCONSEQ default).
    p.adc.adcctl0().write(|w| w.adcsht().adcsht_8().adcon().set_bit());
    p.adc.adcctl1().write(|w| w.adcshp().set_bit());
    p.adc.adcctl2().write(|w| w.adcres().adcres_2());
}

/// Run one single conversion on the currently-selected channel/reference; return the 12-bit result.
/// ADCENC must be low to (re)select channel/reference, so callers write ADCMCTL0 first, then call
/// this; it re-clears ADCENC on the way out so the next measurement can reconfigure. Bounded spin —
/// never hangs on a stuck ADC.
fn convert(p: &Peripherals) -> u16 {
    p.adc.adcctl0().modify(|_, w| w.adcenc().set_bit().adcsc().set_bit());
    // Poll the completion flag (ADCIFG0 sets when ADCMEM0 is loaded; reading ADCMEM0 clears it).
    for _ in 0..20000 {
        if p.adc.adcifg().read().adcifg0().bit_is_set() {
            break;
        }
    }
    let v = p.adc.adcmem0().read().bits() & 0x0FFF;
    p.adc.adcctl0().modify(|_, w| w.adcenc().clear_bit());
    v
}

/// Supply rail DVCC in millivolts (ratio method). Returns 0 on a nonsensical zero reading.
pub fn dvcc_mv(p: &Peripherals) -> u16 {
    // VR+ = DVCC (ADCSREF=0), input = internal 1.5-V reference node (A13).
    p.adc.adcmctl0().write(|w| w.adcsref().adcsref_0().adcinch().adcinch_13());
    let r = convert(p) as u32;
    if r == 0 {
        return 0;
    }
    (FULL_SCALE * VREF_MV / r) as u16
}

/// Die temperature in whole °C from the on-chip sensor, TLV two-point interpolated. Returns 0 if the
/// calibration is blank/erased (degenerate span) — treat that as "no reading" rather than trusting it.
pub fn die_temp_c(p: &Peripherals) -> i16 {
    // VR+ = internal 1.5-V reference (ADCSREF=1), input = temperature sensor (A12).
    p.adc.adcmctl0().write(|w| w.adcsref().adcsref_1().adcinch().adcinch_12());
    let raw = convert(p) as i32;
    let c30 = unsafe { core::ptr::read_volatile(CAL_ADC_15T30) } as i32;
    let c105 = unsafe { core::ptr::read_volatile(CAL_ADC_15T105) } as i32;
    if c105 == c30 {
        return 0;
    }
    ((raw - c30) * (105 - 30) / (c105 - c30) + 30) as i16
}

/// TEMP DEBUG (remove once temp is validated): raw temp-sensor ADC + the two TLV cal points, so we
/// can see on hardware why the interpolation is off.
pub fn temp_debug(p: &Peripherals) -> (u16, u16, u16) {
    p.adc.adcmctl0().write(|w| w.adcsref().adcsref_1().adcinch().adcinch_12());
    let raw = convert(p);
    let c30 = unsafe { core::ptr::read_volatile(CAL_ADC_15T30) };
    let c105 = unsafe { core::ptr::read_volatile(CAL_ADC_15T105) };
    (raw, c30, c105)
}
