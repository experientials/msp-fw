/* MSP430FR247x (FR2476/FR2475) memory map — family feature `fr247x` (default; primary dev target).
 * FR2476: 64 KB program FRAM + 512 B info + 8 KB SRAM; FR2475: 32 KB + 512 B + 6 KB. The msp430
 * Rust/gcc toolchain links only the lower 32 KB FRAM window (0x8000–0xFF7F) without the large-memory
 * model, so ROM is 32 KB here (matches diag/memory.x — same part). Gate budgets 32 KB / 8 KB.
 * Emitted to OUT_DIR by build.rs (do not rely on this file being in the crate cwd). */
MEMORY
{
  RAM     : ORIGIN = 0x2000, LENGTH = 0x2000   /* 8 KB SRAM (FR2476; FR2475 has 6 KB — headroom)  */
  ROM     : ORIGIN = 0x8000, LENGTH = 0x7F80   /* 32 KB lower FRAM window, END 0xFF7F             */
  /* msp430-rt emits a 16-word vector table ending at 0x10000, so VECTORS spans 0xFFE0-0xFFFF
     (reset vector at 0xFFFE). Revisit with the PAC `rt` feature + device.x when real ISRs land. */
  VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020
}
