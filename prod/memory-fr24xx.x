/* MSP430FR2433 memory map — family feature `fr24xx` (single-I²C slave-only expander).
 * Datasheet §1.3: 15 KB program FRAM + 512 B info FRAM + 4 KB SRAM. The 512 B info FRAM (0x1800) is
 * a separate data region and is NOT mapped here — code lives in the 15 KB program FRAM window only.
 * Gate budgets 15 KB / 4 KB. Emitted to OUT_DIR by build.rs. */
MEMORY
{
  RAM     : ORIGIN = 0x2000, LENGTH = 0x1000   /* 4 KB SRAM                             */
  ROM     : ORIGIN = 0xC400, LENGTH = 0x3B80   /* 15 KB program FRAM (main), END 0xFF7F */
  /* msp430-rt emits a 16-word vector table ending at 0x10000, so VECTORS spans 0xFFE0-0xFFFF
     (reset vector at 0xFFFE). Revisit with the PAC `rt` feature + device.x when real ISRs land. */
  VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020
}
