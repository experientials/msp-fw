//! Small helpers. `delay_ms` is a crude NOP spin (~1 MHz MCLK); good enough for POST
//! visuals, not for precise timing.

use msp430::asm;

pub fn delay_ms(ms: u16) {
    let mut i = 0;
    while i < ms {
        // ~1000 cycles per outer step at 1 MHz; the inner count is approximate.
        let mut j = 0u16;
        while j < 200 {
            asm::nop();
            j += 1;
        }
        i += 1;
    }
}
