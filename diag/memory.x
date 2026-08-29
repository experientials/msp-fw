MEMORY
{
  RAM     : ORIGIN = 0x2000, LENGTH = 0x2000   /* 8 KB SRAM                          */
  ROM     : ORIGIN = 0x8000, LENGTH = 0x7F80   /* 32 KB lower FRAM (main), END 0xFF7F */
  /* msp430-rt emits a 16-word vector table ending at 0x10000, so VECTORS spans
     0xFFE0-0xFFFF (reset vector at 0xFFFE). Diag uses no interrupts, so only the
     reset vector matters; revisit with the PAC `rt` feature + device.x if ISRs land. */
  VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020
}
