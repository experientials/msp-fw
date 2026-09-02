# FR2476 / LP-MSP430FR2476 hardware notes (hard-won)

Board-wiring and silicon-config facts that cost real bench time to (re)discover. **Meta-lesson
first, because it caused the most churn:**

> **Get board wiring from the authoritative user's guide / datasheet, not from silk labels, memory,
> or a probe.** The LaunchPad user's guide (**SLAU802**) and the device datasheet define every pin
> and every config bit. A "probe which pin moves" hunt is a last resort, not a first step — and it
> misses pins on ports you didn't think to read.

## LP-MSP430FR2476 board pins (SLAU802 — user's guide)

- **Buttons (Fig. 19 "Pushbuttons"):** **S1 = P4.0**, **S2 = P2.3** (both active-low, 47 kΩ external
  pull-up to 3V3). **S3 = RST.** Note S1/S2 are on *different ports* — an early "silk says P1.6/P2.3"
  guess was wrong and wasted a flash cycle. `diag/src/buttons.rs`.
- **Backchannel UART:** TX = **P1.4**, RX = **P1.5** (UCA0) → eZ-FET CDC. Same two pins are the
  **BSL UART** (below).
- **Temperature sensor (TMP235):** analog out on **P1.1** (A1). LED1 + RGB LED2 are PWM-driven
  (P5.x) — not used by diag.
- **32.768 kHz crystal:** P2.0/P2.1 — never GPIO. **IS31 SDB = P2.5. RCWL radar OUT = P2.4.**

## FR2476 silicon gotchas (datasheet SLASEO7)

- **ADC temperature sensor needs `TSENSOREN` in PMMCTL2** — a *separate* enable from `INTREFEN`.
  Without it the temp channel (A12) reads ~0 and any interpolation is garbage. `PMMCTL2` is **not**
  password-protected (unlike `PMMCTL0`), so no unlock dance.
- **ADC internal channels:** A12 = temp sensor, A13 = 1.5 V reference, A14 = DVSS, A15 = DVCC.
- **Measure DVCC without external parts (ratio method):** sample A13 (1.5 V ref) using **DVCC as the
  ADC reference** → `DVCC = 4095 × 1.5 V ÷ result` (12-bit). No trusted reference needed.
- **Die temperature:** sample A12 against the internal 1.5 V ref, two-point interpolate through the
  TLV factory cal: **0x1A1A = 30 °C**, **0x1A1C = 105 °C** (ADC-cal block, tag 0x11 @ 0x1A14).
- **Chip identity from the TLV device descriptor:** 16-bit device ID at **0x1A04** (LE word).
  **FR2476 = 0x832A**, **FR2475 = 0x832B**. Read it rather than hardcoding the model. Per-unit die
  serial (lot/wafer) at 0x1A0A.
- **UART BSL (production programming, datasheet Table 9-4 + SLAU550):** data on **P1.4/P1.5** (same
  pins as the console UART), **9600 8-E-1 (even parity)**. Entry via a sequence on RST/NMI + TEST, or
  — key for FR24xx — **a blank device auto-enters BSL** (empty reset vector), so a fresh chip needs
  no entry sequence. See `RPI-BUILD-FLASH.md`.

## Peripheral / device gotchas

- **6DOF IMU 13 Click (MIKROE-4228) is an mCube MC6470, NOT an ICM-42605** — accel+mag eCompass,
  I²C-only, at **0x4C (accel) + 0x0C (mag)**. If a "6DOF IMU" reads absent at 0x68, look at
  0x4C/0x0C.
- **MC6470 accel has wake latency:** after `MODE_WAKE`, the first reads return all-zeros. Poll for a
  non-zero sample (an all-zero vector is never valid gravity) rather than trusting the first read.
- **All I²C addresses ACK on a scan = SDA stuck low** (electrical), not real devices.
- **UART garbled until the DCO/FLL is a precise 1 MHz** (SCG0-off → CSCTL1/2/3 → wait
  `CSCTL7 & FLLUNLOCK == 0` → CSCTL4 → settle). "Close enough" garbles the first bytes.
- **SBW/SBW-attach resets the chip;** the `just monitor` `cat` stream is bursty (gaps are the
  monitor, not the firmware) — don't read a mid-stream gap as a firmware pause.

## Display

- The `ssd1306`/`embedded-graphics` crate stack was **removed** once diag approached the ROM budget
  (reclaimed ~16 KB). All display is now the raw-I²C driver `diag/src/ssd1306_raw.rs` (SSD1306
  128×32, 5×7 font). New display work uses raw I²C — do not re-add the graphics stack.
