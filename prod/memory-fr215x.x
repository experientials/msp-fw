/* MSP430FR215x / FR235x memory map — families `fr215x` (FR2155, production dual-I²C baseline) and
 * `fr235x` (FR2355 dev LaunchPad, a superset with SAC). Both share the SAME map (verified against the
 * TI toolchain linker scripts msp430fr2155.ld / msp430fr2355.ld): 32 KB program FRAM + 4 KB SRAM.
 * Note vs fr247x: SRAM is 4 KB here (FR2476 dev part has 8 KB) — the production part is the tighter
 * gate. Gate budgets 32 KB / 4 KB. Emitted to OUT_DIR by build.rs. */
MEMORY
{
  RAM     : ORIGIN = 0x2000, LENGTH = 0x1000   /* 4 KB SRAM                             */
  ROM     : ORIGIN = 0x8000, LENGTH = 0x7F80   /* 32 KB program FRAM (main), END 0xFF7F */
  /* msp430-rt emits a 16-word vector table ending at 0x10000, so VECTORS spans 0xFFE0-0xFFFF
     (reset vector at 0xFFFE). Revisit with the PAC `rt` feature + device.x when real ISRs land. */
  VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020
}
