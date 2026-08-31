# Dev Boards / Bench Inventory

Physical dev boards and eval/breakout modules on hand for Talki / Ziloo prototyping.
Keep this current as boards come and go. Last updated: **2026-08-29**.

## MCU / dev boards

| Board | Part / MCU | Qty | Interface | Purpose / notes |
|---|---|---|---|---|
| MSP430 LaunchPad FR2476 | LP-MSP430FR2476 (MSP430FR2476) | 2 | USB (onboard eZ-FET debugger) | Supervisor-MCU dev target. See [MCU_SELECTION.md](MCU_SELECTION.md). |
| MSP430 LaunchPad FR2433 | MSP-EXP430FR2433 (MSP430FR2433) | 1? | USB (onboard eZ-FET debugger) | Alternate/low-supply-risk supervisor MCU. See [MCU_SELECTION.md](MCU_SELECTION.md). |
| micro:bit V2.2 | nRF52833 (BLE) | 1 | USB / edge connector / BLE | General prototyping; onboard LSM303AGR accel+mag, MEMS mic, speaker, 5×5 LED matrix. |

## Sensor / peripheral eval & breakout boards

| Board | Chip | Interface | Purpose / notes |
|---|---|---|---|
| IS31FL3730-QFLS2-EB | ISSI/Lumissil IS31FL3730 | I²C | LED matrix / dot-matrix driver eval board — relevant to Face LED display. Wiring/control notes: [IS31FL3730_EB.md](IS31FL3730_EB.md). |
| RCWL-0516 | microwave Doppler radar (RCWL-0516 family) | digital OUT | Presence / motion detection. ⚠️ verify exact part — the common one is **RCWL-0516**; confirm "Q516" marking. |
| 6DOF IMU 13 Click (MIKROE-4228) | mCube **MC6470** (accel + magnetometer eCompass) | I²C only | MikroE Click board. ⚠️ NOT an ICM-42605 (earlier error) — verified 2026-08-31: the part is the MC6470, two I²C sub-addresses **0x4C accel + 0x0C mag** (both ACK the diag bus scan), no SPI/CS. Matches HARDWARE.md's product MC6470 on the sensor bus. (Ziloo also has a `6dof-imu-9-click` — see [ziloo/Hardware/testing](../../ziloo/Hardware/testing).) |
| VL53L1 (Adafruit) | ST VL53L1X | I²C | Time-of-Flight distance, ~4 m range. |
| APDS-9960 | Broadcom APDS-9960 | I²C | Gesture + proximity + RGB + ambient light. |
| VL53L0 | ST VL53L0X | I²C | Time-of-Flight distance, ~2 m range. |
| SSD1306 OLED | Adafruit or Aliexpress | I²C | 128x32 px display |
| MCP23017 | | I²C | 16 bit Extender |

## Notes

- Most sensor breakouts are **I²C** — plan bus addressing if several run on one prototype
  (both VL53L0X and VL53L1X default to `0x29` and need XSHUT re-addressing to coexist).
- The sensing mix (2× ToF, gesture, IMU, radar presence, LED matrix) maps directly to the
  Face / presence-detection side of the product.

## Flashing on macOS (verified 2026-08-29)

The FR2476 LaunchPad flashes from this Mac (Apple Silicon) via `mspdebug` — verified end to end:
eZ-FET firmware updated, target read back as `MSP430FR2476 (id=0x0210)`. The full setup (x86_64
mspdebug + signed `libmsp430.dylib` + `DYLD_FALLBACK`) and helper scripts live in the
[msp430-macos-dev skill](../.claude/skills/msp430-macos-dev/SKILL.md). Quick flash:

```bash
.claude/skills/msp430-macos-dev/scripts/mspdebug-macos.sh tilib "prog build/firmware.elf" "run" "exit"
```

## Maintenance

- Update `Qty` and the "Last updated" date as the bench changes.
- Confirm the FR2433 LaunchPad count (marked `1?`) and the RCWL part number.
