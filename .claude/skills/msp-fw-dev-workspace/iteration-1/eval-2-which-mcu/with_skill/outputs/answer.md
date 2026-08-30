# Which MSP430 to design in for the board batch

## Recommendation

**Design the production boards around the MSP430FR2433 (VQFN-24, order code
MSP430FR2433IRGER) — unless the supervisor genuinely needs true analog rail
monitoring at minimum idle current or compute-through-power-loss, in which case
use the FR2476.** For most supervisor + I/O-extender duty, FR2433 is the right call:

- **Lowest supply risk / cheapest** — ~17,900 in stock at LCSC from ~$0.54/unit, plus
  Mouser/Digikey stock. This is the only one of the three that is comfortably sourceable
  for a JLCPCB/LCSC assembly run.
- **Enough of everything for the supervisor role** — 3 eUSCI (I²C + SPI + a spare debug
  UART), 19 GPIO, and an ADC window comparator that covers wake-on-threshold without
  clocking the CPU.
- **Same ~710 nA LPM3.5 standby** as the FR2476, so you don't lose the always-on power
  budget by dropping to it.

Reach for **FR2476** (MSP430FR2476TRHBR) only if the design needs the eCOMP analog
comparator + 6-bit DAC (lower average current for a long-idle rail watchdog than the
ADC window) or FRAM compute-through-power-loss state restoration. It's the best technical
fit and it's what the eval boards on hand use — but **treat its supply as a risk**: LCSC
lists it as pre-order with ~100 units. Resolve LCSC/JLCPCB stock (or plan Western-sourced
consigned parts) before committing an assembly run. Do not let the on-hand eval boards
default your production choice.

**FR2422 is not recommended** — least memory, only 2 eUSCI (no headroom for I²C + SPI +
debug at once), and weak on LCSC. It buys you nothing over the FR2433.

## Firmware constraints to know before you commit

These are the load-bearing ones — the firmware is not yet chip-agnostic, so the MCU choice
has real firmware consequences today:

1. **The "develop-on-2476 → ship-on-2433" port caveat is the big one.** The eval boards and
   all current firmware target the FR2476, which has features the FR2433 does **not**:
   eCOMP analog comparator + DAC, 64 KB FRAM / 8 KB SRAM (vs 2433's **16 KB / 4 KB**), and
   43 GPIO (vs **19**). Shipping on 2433 is viable **only** if the design avoids
   eCOMP-dependent monitoring and stays inside 2433's memory and pin count. Decide the
   production target *now*, before firmware accretes 2476-only assumptions.

2. **The firmware is currently hard-wired to the FR2476, not yet portable.** `diag/Cargo.toml`
   pins the `msp430fr2476` PAC directly, and `diag/memory.x` declares an 8 KB SRAM / 32 KB
   FRAM window — a 2476 layout. The planned **feature-gated board crate (chip = cargo
   feature, role = FRAM config)** that would let one source build per-chip **does not exist
   yet**; there is no HAL — firmware talks to the PAC directly. Shipping on 2433 means:
   generating/using the vendored **FR2433 PAC** (`pac/msp430fr2433/` already exists), adding
   a 2433 `memory.x` (4 KB RAM / 16 KB FRAM), and building the per-chip abstraction. Budget
   that work if you pick 2433.

3. **Good news — nothing written so far blocks the 2433 port.** The current `diag` firmware
   uses only the common FR2433/FR2476 peripheral subset (eUSCI_B0 I²C, eUSCI UART) and has
   **no eCOMP/DAC/window-comparator dependencies** in its source. It is tiny and fits 2433's
   memory easily. So the codebase is still at the fork in the road, not past it.

4. **`board/connections.toml` is written entirely against the FR2476 pinout** (e.g.
   `P1.2 = UCB0SDA`, package "RHB VQFN-40", functions verified against the 2476 datasheet).
   FR2433 is a different package (VQFN-24) with a different pin-function table, so the pin map
   must be re-verified against the 2433 datasheet before boards are laid out. Add connections
   as `planned` and promote to `active` only after checking each function against the target
   chip's datasheet.

## Bottom line for the batch

- **Simple supervisor (poll rails via ADC window, drive SD/SPI + I²C expander): order FR2433.**
  Best availability, price, and supply risk; plan the small firmware port (2433 PAC + memory.x
  + start the board-crate abstraction) and re-verify `connections.toml` against the 2433 pinout.
- **Needs true analog monitoring at min idle current or compute-through-power-loss: FR2476** —
  but confirm LCSC/JLCPCB stock first; its availability is the gating risk, not the firmware.
- Either way, **lock the production part before more firmware is written**, so it doesn't
  quietly acquire 2476-only dependencies via the eval boards.

## References

- `bob-929/docs/MCU_SELECTION.md` — feature comparison, cost/availability tracking (last
  checked 2026-08-29), supervisor notes, and the port caveat. Primary decision doc.
- `msp-fw/.claude/skills/msp-fw-dev/SKILL.md` — high-level goals + chip-portability conventions
  (no HAL, PAC-direct, planned board crate).
- `msp-fw/diag/Cargo.toml`, `msp-fw/diag/memory.x` — evidence the firmware is 2476-hardcoded today.
- `msp-fw/board/connections.toml` — the FR2476 pin map that would need re-verification for 2433.
- `msp-fw/pac/msp430fr2433/`, `msp-fw/pac/msp430fr2476/` — vendored PACs for both targets.
- Datasheets/app notes in `bob-929/docs/`: `msp430fr2433.pdf`, `msp430fr2476.pdf`,
  `slaa890a.pdf` (ADC window comparator, no-CPU monitoring), `sszt426.pdf` (2476
  compute-through-power-loss).
