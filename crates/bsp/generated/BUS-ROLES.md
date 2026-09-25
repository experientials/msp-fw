<!-- GENERATED — DO NOT EDIT. Source: crates/bsp/connections.toml. Regenerate: `just bsp docs` (scripts/gen-bsp-docs.py). -->

# MSP430 BSP — I2C bus roles (bob-929)

> Generated from [`crates/bsp/connections.toml`](../connections.toml) by `scripts/gen-bsp-docs.py`. Edit the TOML, then run `just bsp docs` — never edit this file by hand.

**The MSP is on exactly these I2C buses and nothing else** — never SYS_I2C or the PMIC bus (those are SoM-side). Master model below is the canonical fact; see also `ziloo/Hardware/stem/STEM-EXPANDER.md`.

## Sensor I2C — `eUSCI_B0`

- **Role:** **master switches** (MSP ↔ SoM). MSP MASTERS it in Sensing mode; RELEASES it so the SoM masters it in Passive mode.

| Pin | Function | Signal | Status |
|---|---|---|---|
| P1.2 | UCB0SDA | Sensor/LED I2C SDA | active |
| P1.3 | UCB0SCL | Sensor/LED I2C SCL | active |

## Stem I2C — `eUSCI_B1`

- **Role:** MSP is always the **slave**. MSP is ALWAYS the slave; the SoM (RT core) or the RP2040 always masters it.

| Pin | Function | Signal | Status |
|---|---|---|---|
| P3.2 | UCB1SDA | Stem I2C SDA (MCU = SLAVE; SoM/RP2040 masters) | planned |
| P3.6 | UCB1SCL | Stem I2C SCL (MCU = SLAVE) | planned |
