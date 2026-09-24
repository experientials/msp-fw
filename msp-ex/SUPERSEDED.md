# ⚠ SUPERSEDED — `msp-ex/` is a 2022 POC, not the current design

This whole directory is an **early proof-of-concept** (KiCad symbol exported by Eeschema
**2022/10/01**, C firmware for PlatformIO, `slau802.pdf`). It was dropped into the repo on
2026-08-29 as a starting reference. **Do not use it for pinout or architecture** — the shipping
`diag`/`prod` firmware diverged from it deliberately.

## What's wrong in `TEST-FR2476.png` / `TEST-FR2476TPTR.svg`

The symbol **reverses the eUSCI_B0/B1 roles** and shows buses that don't exist on the MSP:

| | This POC render (WRONG) | Canonical — `crates/bsp/connections.toml` |
|---|---|---|
| **SENSOR** (MSP master) | eUSCI_B1, P3.2/P3.6 (`SENSOR_SDA_B1`) | **eUSCI_B0, P1.2/P1.3** |
| **STEM** (MSP slave) | eUSCI_B0-alt, P4.5/P4.6 (`STEM_SDA_B0`) | **eUSCI_B1, P3.2/P3.6** |
| **SYS_I2C** | on MSP pins P4.3/P4.4 (`SYS_SDA_B1`) | **not on the MSP at all** |
| **P1.2/P1.3** | "nand sim" (SPI-NAND idea) | the SENSOR bus |

It's also internally inconsistent (SENSOR *and* SYS both drawn on eUSCI_B1 — the same peripheral).

## The canonical facts

- **MSP is on exactly two I²C buses — Stem + Sensor — and nothing else.** No SYS_I2C, no PMIC bus.
- **Stem** = eUSCI_B1 (P3.2/P3.6), MSP always **slave** (SoM/RP2040 masters).
- **Sensor** = eUSCI_B0 (P1.2/P1.3), master **switches** (MSP in Sensing / SoM in Passive).
- **SYS_I2C** is a **SoM-side** bus (RTC/EEPROM/codec on the bench carrier's I²C2; PMIC on I²C1).
- The **"unified I²C"** idea (all devices on one bus) and the **"nand sim"** SPI-NAND idea are both **dead**.

Sources of truth: `crates/bsp/connections.toml` (pin registry) and
`ziloo/Hardware/stem/STEM-EXPANDER.md` ("MSP430 supervisor bus scope"). The PNG/SVG here carry a
`SUPERSEDED` banner pointing back to these.

## Open task

The **original KiCad project** that produced this symbol is **not in any repo** (only the exports are
here). Locating it is tracked in `../NOTES.md` — if found, it should be corrected to match
`connections.toml`, or formally retired.
