# 202 Combi — Supervisor MCU Selection

Tracking for the MCU planned on the [202 Combi Camera Module](../Hardware/202/202-MODULE.md).
The v0.2 board reserves space for an MCU but does not populate one.

## Role

The MCU is a **low-power supervisor**: it stays awake to monitor the board (rails / events)
**while the main board is powered off**, and wakes the system on a trigger. It also does
housekeeping over the on-board buses — SD card slot (SPI), TCA9534 I/O expander (I²C),
and the TXS0104 level shifters.

Selection axes that matter for this role: **always-on average current**, **wake sources
(comparator / ADC window)**, **serial channels** (I²C + SPI needed simultaneously),
**GPIO count**, and **sourcing for JLCPCB assembly**.

## Feature comparison

| Feature | FR2422 (planned) | FR2433 | FR2476 |
|---|---|---|---|
| Family | FR24xx value | FR24xx value | FR247x (mixed-signal) |
| FRAM | 8 KB | 16 KB | **64 KB** |
| SRAM | 2 KB | 4 KB | **8 KB** |
| Max CPU | 16 MHz | 16 MHz | 16 MHz |
| GPIO | 15 (VQFN-20) | 19 (VQFN-24) | **43** (LQFP-48) |
| ADC | 8-ch 10-bit | 8-ch 10-bit | 12-ch **12-bit** |
| ADC window comparator (wake w/o CPU) | ✅ | ✅ | ✅ |
| Analog comparator (eCOMP + 6-bit DAC) | — | — | **✅** |
| eUSCI serial | 2 (1×A+1×B) | **3** (2×A+1×B) | **4** (2×A+2×B) |
| Timers | 2×TA3 | 4× (2×TA3+2×TA2) | 4×TA3 + 1×TB7 |
| RTC | 16-bit ctr | 16-bit ctr | 16-bit ctr |
| Standby (LPM3.5) | ~710 nA | ~710 nA | ~710 nA |
| Packages | TSSOP-16, VQFN-20 | VQFN-24, DSBGA-24 | VQFN-32/40, LQFP-48 |
| Voltage | 1.8–3.6 V | 1.8–3.6 V | 1.8–3.6 V |
| Temp | –40…85 °C | –40…85 °C | **–40…105 °C** |

## Supervisor notes

- **Wake-on-threshold**: all three can monitor a signal and wake without CPU intervention.
  The value line (FR2422/FR2433) uses the **ADC window comparator** (see `slaa890a.pdf`);
  the FR2476 adds a true **analog eCOMP + 6-bit DAC** that runs at lower average current
  because it doesn't clock the ADC. For a rail/voltage watchdog that must sit idle for long
  periods, eCOMP is the better primitive.
- **Serial budget**: the board needs I²C (expander) **and** SPI (SD) at once. FR2422's
  2 eUSCI leaves no spare (e.g. no debug UART); FR2433 (3) and FR2476 (4) leave headroom.
- **Compute-through-power-loss**: FR2476 + TIDM-FRAM-CTPL (see `sszt426.pdf`) supports
  state restoration across power failure — relevant if the supervisor must persist state
  when the main board cuts power.

## Cost & availability tracking

> Last checked: **2026-08-29**. Prices are ballpark unit cost; verify before ordering.
> Refresh method at bottom.

| Part (order code) | Pkg | FRAM | Distributor | Stock | Unit price | JLCPCB/LCSC |
|---|---|---|---|---|---|---|
| **FR2422** MSP430FR2422IRHLR | VQFN-20 | 8 KB | Digikey | ~3,100 | $1.35 @1 · ~$0.68 @1k | thin on LCSC |
| **FR2433** MSP430FR2433IRGER | VQFN-24 | 16 KB | LCSC | **~17,900** | **from ~$0.54** | ✅ well-stocked |
| " | " | " | Mouser | ~1,600 (+6k on order) | $2.25 @1 | — |
| " | " | " | Digikey | in stock | $2.19 @1 | — |
| **FR2476** MSP430FR2476TRHBR | VQFN-40 | 64 KB | LCSC | **~111 (pre-order)** | ~$1.23 | ⚠️ pre-order only |
| " | " | " | Digikey | in stock | ~£2.36 / ~$3.0 | — |

**Availability verdict:**
- FR2433 — abundant and cheap everywhere incl. LCSC → lowest supply risk for JLCPCB.
- FR2422 — fine on Western distributors, weak on LCSC.
- FR2476 — **supply risk**: LCSC shows it as a pre-order part with ~100 units. Best
  technical fit but would need a stock check (or Western-sourced consigned parts) before
  committing to a JLCPCB assembly run.

## Current state (2026-08-29)

- **2× MSP430FR2476 eval boards on hand** → firmware development is easy on the 2476 today.
- Henrik found the **FR2433** interesting on review; it remains the low-supply-risk option.
- **Port caveat:** developing on the 2476 (eCOMP + 64 KB FRAM) risks firmware that won't
  drop onto a 2433 (no analog comparator, 16 KB / 4 KB). Develop-on-2476 → ship-on-2433 is
  viable **only** if the design avoids eCOMP-dependent monitoring and stays within 2433's
  memory. Decide the production target early rather than letting the eval boards default it.

## Recommendation (open)

- If the supervisor is **simple** (poll rails via ADC window, drive SD + expander): **FR2433**
  — best availability/price, enough serial + GPIO, ADC window comparator covers wake-on-threshold.
- If it needs **true analog monitoring at minimum idle current** or **compute-through-power-loss**:
  **FR2476** — but resolve the LCSC/JLCPCB supply risk first.
- **FR2422** is the weakest of the three: least memory, 2 eUSCI is tight, and no LCSC edge.

## Datasheets & app notes (in this folder)

- `msp430fr2422.pdf` — FR2422 datasheet
- `msp430fr2433.pdf` — FR2433 datasheet
- `msp430fr2476.pdf` — FR2476 datasheet
- `slaa890a.pdf` — FR2xx/FR4xx ADC + **window comparator monitoring without CPU**
- `sszt426.pdf` — FR2476 **compute-through-power-loss** state restoration (TIDM-FRAM-CTPL)

## Refresh method

- TI datasheets: `https://www.ti.com/lit/ds/symlink/<part>.pdf`
- Price/stock: LCSC (JLCPCB assembly), Mouser, Digikey — search the order code above.
- Update the "Last checked" date and the stock/price cells when re-verified.
