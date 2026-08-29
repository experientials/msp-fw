MEMORY
{
  RAM     : ORIGIN = 0x2000, LENGTH = 0x2000   /* 8 KB SRAM                        */
  ROM     : ORIGIN = 0x8000, LENGTH = 0x7F80   /* 32 KB lower FRAM (main), END 0xFF7F */
  /* msp430-rt emits a 16-word (0x20-byte) vector table that must END at 0x10000,
     so VECTORS spans 0xFFE0-0xFFFF (reset vector lands at 0xFFFE). This is the
     legacy 16-vector layout; the FR2476 has more hardware slots (0xFFA2..), but
     for a generic no-ISR app only the reset vector at the top matters. Revisit
     with a proper PAC / _sinterrupts when real interrupts are used. */
  VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020
}
