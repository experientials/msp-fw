# Which MSP430 to design in for the production boards

## Recommendation

**Default to the MSP430FR2433 (VQFN-24, `MSP430FR2433IRGER`) as the production part**, and treat
the FR2476 as a *variant* to populate only on boards that genuinely need true analog monitoring or
compute-through-power-loss. This matches how the repos already frame it: `pac/README.md` labels
`msp430fr2433` the **"production default node"** and `msp430fr2476` the **"battery-monitor variant"**.

Why FR2433 for the batch you're about to order:

- **Supply risk is the deciding axis for a JLCPCB/LCSC run.** FR2433 is abundant and cheap
  (~17.9k in stock on LCSC, from ~$0.54; also stocked at Mouser/Digikey). The FR2476 shows on LCSC
  as a **pre-order part with ~100 units** — a real risk if you're committing to an assembly run.
  (`bob-929/docs/MCU_SELECTION.md`, "Cost & availability", last checked 2026-08-29.)
- **It's technically sufficient for the supervisor role**: 16 KB FRAM / 4 KB SRAM, 3× eUSCI
  (2×A + 1×B), 19 GPIO, and an **ADC window comparator** that can wake the system on a rail
  threshold *without clocking the CPU*. That covers "poll rails, drive SD (SPI) + I/O expander (I²C),
  wake on trigger."
- **Firmware fits with margin.** The firmware budget is **8 KB image / 1 KB RAM**
  (`msp-fw/FIRMWARE-API.md`); FR2433's 16 KB/4 KB clears that comfortably. (FR2422, the earlier
  "planned" part, has only 8 KB FRAM / 2 KB SRAM — too tight, and weak on LCSC. Don't pick it.)

Pick **FR2476** instead only if the supervisor must do **true analog comparison at minimum idle
current** (eCOMP + 6-bit DAC, which the value line lacks) or **state restoration across power loss**
(TIDM-FRAM-CTPL). If you go FR2476, **resolve the LCSC/JLCPCB stock question first** or plan on
Western-sourced consigned parts.

## Firmware constraints to settle BEFORE committing the order

These are the things that can bite between "works on the eval board" and "works on the production
part," so decide them now, not after the boards arrive:

1. **You are developing on FR2476 eval boards, but planning to ship FR2433 — verify the port.**
   Two FR2476 LaunchPads are on the bench (one FR2433 LaunchPad too). Firmware written against
   FR2476 features **may not drop onto an FR2433**. Specifically:
   - **eCOMP / analog comparator is FR2476-only.** If wake-on-threshold is built on eCOMP + DAC, it
     will *not* run on FR2433 — you must implement it with the **ADC window comparator** instead
     (see `slaa890a.pdf`). Choose the monitoring primitive to be the FR2433-compatible one if
     FR2433 is the ship target. (`MCU_SELECTION.md`, "Port caveat".)
   - **Memory:** stay within 16 KB FRAM / 4 KB SRAM. Not a problem at the 8 KB/1 KB budget, but any
     use of the FR2476's 64 KB FRAM (e.g. CTPL state) won't port.

2. **Dual-I²C is the sharp edge — FR2433 has only ONE eUSCI_B (hardware I²C).** The msp-fw product
   intent is a **Stem I²C** slave *and* a separate **Sensor I²C** master running at the same time
   (`HARDWARE.md`; `board/connections.toml` plans UCB0 = sensor bus, **UCB1** = Stem bus). FR2476
   has 2×eUSCI_B so it can do both in hardware; **FR2433 has 1×eUSCI_B and cannot.** On FR2433 the
   second bus must be **software/GPIO I²C** — which `FIRMWARE-API.md` already anticipates
   ("Some MSP430 chips only have one I²C port… run over GPIO", `sysSoftI2C`/`sensorSoftMonitor`).
   - If the *production* board only needs **one** I²C (e.g. the 202 Combi supervisor role: I²C
     expander + SPI SD card), FR2433's single UCB is fine — its 3rd eUSCI leaves a spare for a debug
     UART. **Confirm the real production bus topology before committing:** one I²C → FR2433 clean;
     two independent I²C → either accept soft-I²C on FR2433 or use FR2476.

3. **I²C pin-mux differs between the chips — re-verify, don't assume.** All the confirmed pin work
   (UCB0 SDA/SCL = **P1.2/P1.3**, UCA0 UART = P1.4/P1.5) is grounded against the **FR2476**
   datasheet. NOTES.md open item: *"Verify FR2433 I²C pins from its datasheet (different chip)
   before trusting P1.2/P1.3 there."* Do that before laying out an FR2433 board.

4. **Package field in the connection registry is wrong — fix before layout.** `board/connections.toml`
   says `RHB VQFN-40` for the FR2476, but RHB is the **32-pin** package; the order code tracked for
   the battery-monitor variant is `MSP430FR2476TRHBR` (VQFN-40 is RHA). NOTES.md flags this as an
   open item. Nail down the exact package/order code for whichever part you order.

5. **Interrupts: firmware isn't using the real vector table yet.** `memory.x` currently uses the
   legacy 16-word vector layout with only the reset vector — fine while no ISRs run. The event-driven
   runtime (port-change interrupts, Stem MSG) needs the PAC's `rt` feature / full `device.x` vector
   table. This is a firmware-maturity note, not a chip-selection blocker, but it's not yet exercised
   on hardware. (`TOOLCHAIN.md`; NOTES.md — `diag` compiles clean but is *not yet flashed/HW-tested*.)

6. **Tooling is chip-parameterized and ready for both.** PACs are vendored for **both** FR2433 and
   FR2476 (`pac/`), and the board crate selects the chip by cargo feature, so one firmware source
   builds per-chip binaries. No off-the-shelf HAL exists for the FR24xx family (the crates.io
   FR2476 PAC is yanked; `msp430fr2x5x-hal` is FR2355-only) — the "HAL" is thin PAC wrappers.
   Switching the ship target between FR2433/FR2476 is a supported build path, not a rewrite.

## Bottom line

- **Order FR2433** for the production batch if the board needs at most one hardware I²C bus and no
  true analog comparator — lowest supply risk, cheapest, enough memory/serial/GPIO, firmware fits.
- **Before you commit:** (a) confirm the production **I²C bus count** (one vs. two — the dual-I²C
  case forces soft-I²C on FR2433 or pushes you to FR2476), (b) make sure wake-on-threshold uses the
  **ADC window comparator**, not FR2476 eCOMP, (c) **re-verify FR2433 I²C pin-mux** against its own
  datasheet, and (d) fix the package/order-code in `board/connections.toml`.
- **Choose FR2476** only for a monitoring/CTPL-heavy variant, and **only after** resolving its
  LCSC/JLCPCB stock situation.

## Key references

- `bob-929/docs/MCU_SELECTION.md` — the decision doc: feature comparison, supply/price, port caveat.
- `bob-929/docs/DEV_BOARDS.md` — bench inventory (2× FR2476 + 1 FR2433 LaunchPads on hand).
- `msp-fw/pac/README.md` — FR2433 = "production default node", FR2476 = "battery-monitor variant".
- `msp-fw/FIRMWARE-API.md` — 8 KB image / 1 KB RAM budget; soft-I²C fallback for single-I²C chips.
- `msp-fw/HARDWARE.md` + `msp-fw/board/connections.toml` — dual-bus (Stem + Sensor) intent, pin map.
- `msp-fw/NOTES.md` — open items: verify FR2433 I²C pins; wrong package field; diag not yet HW-tested.
- `msp-fw/TOOLCHAIN.md` — per-chip build via cargo feature; interrupt/vector-table maturity.
- Datasheets in `bob-929/docs/`: `msp430fr2433.pdf`, `msp430fr2476.pdf`, `msp430fr2422.pdf`,
  `slaa890a.pdf` (ADC window comparator), `sszt426.pdf` (FR2476 compute-through-power-loss).
