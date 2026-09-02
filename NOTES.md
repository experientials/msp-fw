# msp-fw — working notes (for future sessions)

Last updated 2026-09-01. State + next steps so we can resume after a context compaction.
See also: [TOOLCHAIN.md](TOOLCHAIN.md), [RPI-BUILD-FLASH.md](RPI-BUILD-FLASH.md),
[TESTING.md](TESTING.md), [diag/DESIGN.md](diag/DESIGN.md),
[diag/DIAGNOSTICS.md](diag/DIAGNOSTICS.md), [examples/README.md](examples/README.md),
[pac/README.md](pac/README.md), and the `msp430-macos-dev` skill.

## Where we are

diag is a working power-on self-test on the FR2476 LaunchPad. The I²C sensor bus is healthy and
**6 devices enumerate cleanly**. The firmware now runs a **cooperative scheduler**, samples the
RCWL radar and APDS proximity as tasks, and has a **feature-gated I²C stress mode**. All committed
on `main` (`6962636`), 1 commit ahead of `origin` (unpushed).

**Board state:** **ONE unified firmware** (no more `--features stress`) — 17.8 KB, built clean,
awaiting a flash of the menu build. Everything is a cooperative task switched at runtime via a
**button + OLED menu** (`tasks::Mode`): boots into `Post` (live self-test, unchanged), **S1** opens
the menu / backs out, **S2** cycles `POST LIVE / I2C MARGIN / I2C SOAK`, **S1** selects. Stress is
now `stress::StressTask` (bounded cooperative batches, not a blocking `-> !` runner). The
**ssd1306/embedded-graphics stack was removed** (the trim the design anticipated) — all display is
now the raw driver `ssd1306_raw.rs`; that reclaimed ~16 KB and pushed diag from ~28 KB to 17.8 KB
(now FR2433-viable). Also on board, verified earlier: boot ASCII banner; `[board]` stats (chip/rev/
die-serial from TLV 0x1A04, reads `832A`); **measured DVCC + die temp** (`adc.rs`, A13/A12, ~3.31 V
/ ~31 °C), boot + periodic `board:` line; MC6470 gravity cold-boot fix (wake-poll → Skip).
Gotchas banked: FR2xx **temp sensor needs `TSENSOREN` in PMMCTL2** (separate from `INTREFEN`);
**buttons S1=P1.6 / S2=P2.3** are a bench-silk assumption — if unresponsive, check the LaunchPad
schematic (`buttons.rs`).

### The bus today (shared eUSCI_B0, SDA=P1.2 / SCL=P1.3, one 3.3 V domain)

| Device | Addr | diag coverage |
|---|---|---|
| IS31FL3730 LED matrix | 0x60/0x61 | driver + pass/fail status display |
| SSD1306 OLED | 0x3C | driver (status display) |
| APDS-9960 | 0x39 | ID (0xAB) + proximity task (5 Hz) |
| VL53L0X ToF | 0x29 | ID only (WHO_AM_I 0xEE) |
| MC6470 eCompass | 0x4C accel + 0x0C mag | presence (see gotcha) |
| RCWL-0516 radar | P2.4 (GPIO) | motion-window task |

## Architecture added this session

- **`crates/sched`** — PAC-agnostic cooperative (run-to-completion) scheduler: `Task<C>`, `Slot`,
  `tick`. Generic over a caller context + caller-supplied `now`, so one copy serves diag / product /
  examples on FR2433/FR2476. Shared library crates now live under **`crates/`** (like `pac/`).
- **diag on the scheduler** — `main` builds a `Cx` blackboard + a task table and loops
  `sched::tick`. `clock.rs` = polled **TB0** ms time base (no ISR). Tasks: `RadarTask` (variable
  rate), `ProximityTask` (APDS), `PostTask` (~3 s POST + report). Rule: **never block in a task**;
  the WDT is the backstop (cooperative = no preemption).
- **Button + OLED menu (`tasks::ButtonTask` + `UiTask` + `Mode`)** — the unification milestone: one
  firmware, tasks switched at runtime instead of `--features`. `Post` (live self-test) is the
  default; **S1** = menu/back, **S2** = next item, **S1-in-menu** = select (`POST LIVE / I2C MARGIN
  / I2C SOAK`). Menu/status rendered with the raw `ssd1306_raw.rs` driver (5×7 font). Buttons on
  P1.6/P2.3 (bench assumption, `buttons.rs`).
- **Stress as a cooperative task (`stress::StressTask`)** — the old blocking `--features stress`
  `-> !` runner is gone; it now advances a bounded batch of *verified* reads per `poll` and yields.
  Margin sweep (100 k→1 MHz) + cumulative error-rate soak (@100 kHz operating clock); `usec.rs` =
  **TA0** µs timer (always built now). Verified reads catch bit-corruption, not just NACKs. Publishes
  progress to the blackboard for `UiTask`; still prints the full lines over UART. Spec in
  `diag/DIAGNOSTICS.md`.
- **Scheduler deadline telemetry** — always-on: `sched` tracks
  per-task `max_late` + `overruns` (from ms `now`, no hardware); `main` prints a `sched:` line every
  ~10 s. Quantifies the "task running too long" concern — the ~3 s POST hogs the loop, so radar/prox
  show that as their max-late.

## Design stance (decisions to honour)

- **Retro-diag-ROM ethos: keep diag lean.** Spirit of an Amiga/C64 diag ROM (a few KB). It's a
  bring-up/health tool, not a product.
- **Graphics stack (`ssd1306`/`embedded-graphics`) — currently trimmed, NOT a settled decision.**
  It came out to fit the unified firmware, but the "ROM ceiling" it hit was the **linker map's
  32 KB** (`diag/memory.x` ROM = 0x8000–0xFF7F), **not the chip** — the FR2476 has **64 KB FRAM**;
  the upper 32 KB (0x10000+) is unmapped. The real fix is mapping the full 64 KB (needs the large
  memory model / 20-bit addressing — msp430 Rust support TBC), after which graphics can return
  alongside everything. Graphics reduction is a "when genuinely too big" lever and **is not
  pressing** — do not treat this trim as permanent. Display functionality is intact via the raw
  `ssd1306_raw.rs` driver; new display work still uses raw I²C.
- **HAL abstraction (`hal.rs` `EusciI2c` = `embedded-hal::I2c` seam) — KEPT.** Was briefly removed
  with the graphics stack (its only consumer), but it's ~0 ROM (LTO strips the unused impl) and is
  the portability seam for `crates/devices` + product firmware. **Do not remove robustness or
  abstraction to save graphics footprint** — they're independent. Restored 2026-09-02.
- **Trimming the stack is discretionary — the timing doesn't matter either way.** Not urgent, not
  blocked on budget (we're fine at ~23.5 KB of 64 KB); do it whenever convenient. Removes ~16 KB and
  makes diag near-FR2433-viable (15.5 KB). It's cleanup, not a gate on anything.
- **End-state: one unified diag firmware**, POST + stress-type work as runtime-selectable *tasks*
  (not a build feature). Independent of the trim — a unified image fits 64 KB with or without it.
  The current `--features stress` split is a stepping stone.

## Structure direction (considered, not yet built)

- **Bag of tests.** diag's checks should be a reorderable registry — `Test { name, group, run: fn(&Cx)
  -> Outcome }`, `Outcome { Pass, Fail, Skip, Info }` — that `run()` iterates. Reorder = reorder the
  array; scripts/menu pick subsets. Realizes DESIGN's `TESTS` sketch. **Keep Tests (run-once →
  verdict) distinct from `sched::Task`s (continuous)**; the POST is a Task that runs the Test bag.
  *Do this refactor inside diag next — low risk, unblocks the button menu.*
- **Code layout.** Not everything belongs under `diag/`. Device drivers + the test framework are
  shared with the future product firmware (which needs self-test too). Target: `crates/devices`
  (drivers), `crates/board` (chip BSP: i2c/uart/timer, FR2433/FR2476 by feature), `crates/selftest`
  (Test framework); `diag/` becomes a thin binary (manifest + orchestration + stress + main).
  **Blocker/decision:** sharing drivers across chips + across diag/product needs a **bus trait seam**
  (`embedded-hal::I2c`, already half-present in `hal.rs`) — a deliberate softening of "no HAL,
  PAC-direct," justified only by the second consumer. **Defer the extraction until the product
  firmware exists (YAGNI); keep each driver a self-contained `src/<chip>.rs` now so the lift is a
  move, not a rewrite.**

## Key gotchas learned (don't relearn these)

- **6DOF IMU 13 Click (MIKROE-4228) is an mCube MC6470, NOT an ICM-42605** — DEV_BOARDS had it
  wrong. Accel+mag **eCompass**, **I²C-only (no CS/SPI)**, at TWO sub-addresses: **0x4C accel +
  0x0C mag** (both ACK the scan). If a "6DOF IMU" reads absent at 0x68, look at 0x4C/0x0C.
- **InvenSense ICM-426xx need CS tied HIGH for I²C** (a CS low edge at power-up latches SPI until
  power-cycle). Don't leave CS floating if we ever use one. (Was the wrong first theory for the MC6470
  — the "no CS pin" observation is what corrected it.)
- **RCWL-0516 radar OUT → P2.4**, chosen over P1.6 because P2.4 is port-interrupt/wake-from-LPMx.5
  capable *and* keeps the VSOM ADC pin (P1.6) free. Wired through a **2 kΩ series** resistor — caps
  any fault current (accidental output contention / back-drive through the ESD clamp) under the
  ±2 mA per-pin limit; high level still ~3.0 V after the divider with the internal pulldown.
- **MAX98357A is an I²S amp → host-side**, not the MSP430 supervisor's job (no I²S peripheral; the
  202 Combi routes I²S on the camera side). Never on the sensor bus.
- **ADXL337 rejected** — analog accel would burn 3 ADC pins for what the MC6470/ICM give digitally.
- **Stress is safe to run for days** — I²C reads are non-destructive, nothing writes
  endurance-limited memory, load is milliwatts. Bounded soak duration is for *reportability*.
- **All addresses "ACK" on a scan = SDA stuck low** (electrical), not real devices.
- **UART garbled until the DCO/FLL is set to a precise 1 MHz** (SCG0-off → set CSCTL1/2/3 → wait
  `CSCTL7 & FLLUNLOCK==0` → set CSCTL4 → settle). The "close enough" version garbles the first bytes.
- **SBW attach RESETS the chip** — `md` shows post-reset state; use breakpoints to observe running state.
- **eZ-FET exposes two CDC ports; the higher-numbered is the backchannel UART.** The numbers change
  per re-enumeration (23203 / 23601 / 23603 / …) — `just monitor` auto-picks the highest.
- **P2.0/P2.1 = 32 kHz crystal** — never GPIO. IS31 SDB is on **P2.5**; RCWL on **P2.4**.
- **msp430 inline asm needs** `#![feature(asm_experimental_arch)]` + `core::arch::asm!("bis #0x40, r2")`
  for SCG0 (SR = r2).

## FR2476 pin quick-reference (from datasheet)

- **UCB0 I²C:** SDA=P1.2, SCL=P1.3 (`P1SELx=01`, default). Alt remap: P4.5/P4.6 (SYSCFG2 USCIB0RMP).
- **UCB1 I²C:** SDA=P3.2, SCL=P3.6 (default) or P4.3/P4.4 (remapped).
- **UCA0 UART:** TX=P1.4, RX=P1.5 (`P1SELx=01`) → eZ-FET backchannel.
- **GPIO in use:** **P2.5 = IS31 SDB** (drive high to enable), **P2.4 = RCWL radar OUT** (GPIO in +
  pulldown, 2 kΩ series). Product-intent ADC: **P1.6=VSOM, P1.7=CHARGE** (`connections.toml`).
  LaunchPad (SLAU802): LED1=P1.0, TMP235=P1.1, **S1=P4.0, S2=P2.3** (NOT P1.6 — earlier silk guess
  was wrong), S3=RST, P2.0/2.1=crystal. diag uses only S1 (one-button cycle).
- SFR addresses: WDTCTL 0x01CC, PM5CTL0 0x0130, P1OUT 0x0202/DIR 0x0204/SEL0 0x020A,
  P2OUT 0x0203/DIR 0x0205/SEL0 0x020B, UCB0CTLW0 0x0540/BRW 0x0546/I2CSA 0x0560/IFG 0x056C,
  UCA0CTLW0 0x0500/BRW 0x0506/MCTLW 0x0508/IFG 0x051C, CSCTL1 0x0182/2 0x0184/3 0x0186/4 0x0188/7 0x018E.

## Rust toolchain / PAC / HAL

- One Docker image (`msp430-c-rust:local`) = msp430-gcc + pinned Rust nightly-2025-06-25 + `just`.
  `just bootstrap` builds it. Each `docker run` is a fresh container → cold `core` rebuild (~2–3 min).
- **PACs vendored** in `pac/msp430fr2476` and `pac/msp430fr2433` (svd2rust; regen via `just pac gen`).
  crates.io `msp430fr2476` is YANKED.
- **No off-the-shelf HAL for FR24xx.** Our "HAL" = thin PAC wrappers in `diag/src/{i2c,uart,...}.rs`.
  Shared logic that isn't hardware (the scheduler) lives in **`crates/sched`**. A typed-pin board/BSP
  crate is the planned next layer (see backlog — name will clash with the `board/` config dir).
- **`diag/` (PAC-based) is canonical.** `regs::dump` prints live SFRs at boot (decoded per-pin
  verdict) so we reason from ground truth, not source.

## Open items / backlog

- [ ] **Map the full 64 KB FRAM** (`diag/memory.x` ROM is only the lower 32 KB, 0x8000–0xFF7F).
      Needs the large memory model (20-bit far addressing) + split ROM regions; verify the msp430
      Rust target supports `-mlarge` and that flashing upper FRAM (0x10000+) works. **Unblocks
      restoring the graphics stack without any trim** (chose option B — defer, not rushed). Timebox
      an investigation before committing.
- [ ] **MC6470**: verify chip-ID registers vs datasheet → add WHO_AM_I (gravity-sanity done).
- [ ] **VL53L0X**: range-a-target exercise (mm + status), beyond ID-only.
- [ ] **Rail ADC** (VSOM P1.6 / CHARGE P1.7) + thresholds — the supervisor's core health check;
      also unlocks the thermal/power stress rung.
- [ ] **Prove one wake-on-event source** (RCWL P2.4 port interrupt, or a sensor INT) — the
      supervisor's reason to exist; currently unproven.
- [ ] **Config-drift**: per-product expected-device manifest (`board/devices.toml`) →
      report missing/unexpected (DESIGN objective 2).
- [ ] **Scheduler deadline-miss instrumentation** (the µs timer is already in place).
- [ ] **Thermal/power-under-load stress rung** (needs rail ADC; watch OLED burn-in if driving outputs).
- [ ] Sync `diag/README` coverage/layout for APDS/MC6470 (partially done).
- [ ] **board crate** (typed pins): will want `crates/board`, which clashes with the `board/` config
      dir — reconcile when it lands.
- [ ] APDS INT wiring for interrupt-driven gesture/proximity (polled for now).
- [ ] Wire `examples/*` onto `crates/sched`.
- [ ] `connections.toml` package field `RHB VQFN-40` — confirm the real bob-929 package.
- [ ] Verify **FR2433** I²C pins from *its* datasheet before trusting P1.2/P1.3 there.
- [ ] Push `main` to origin (1 commit ahead) when ready.
