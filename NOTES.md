# msp-fw — working notes (for future sessions)

Last updated 2026-08-30. State + next steps so we can resume after a context compaction.
See also: [TOOLCHAIN.md](TOOLCHAIN.md), [examples/README.md](examples/README.md), [pac/README.md](pac/README.md),
and the `msp430-macos-dev` skill (`bob-929/.claude/skills/msp430-macos-dev/`).

## Where we are

Toolchain + both hello-worlds (C & Rust) are proven on the FR2476 LaunchPad. The active work is
**IS31FL3730 8×8 LED matrix over I²C**, which is **not yet talking**. Firmware runs and the UART
console is clean; the bus itself isn't communicating.

## I²C / IS31 bring-up — the live problem

- **Wiring:** IS31FL3730-QFLS2-EB eval board. External control via **TP3** with **JP1 open**
  (jumper OFF — onboard LPC922 MCU high-Z). Its own 4.7 kΩ pull-ups on its 3V3 rail — do NOT add
  external pull-ups. Address **0x61**. SDB is active-low; onboard R3/R4 bias it enabled.
- **MCU side:** eUSCI_B0 I²C master, **SDA=P1.2, SCL=P1.3** (`P1SEL0` bits, `P1SELx=01`) — CONFIRMED
  correct vs datasheet **Table 9-23**. 100 kHz (SMCLK 1 MHz / BRW=10).
- **Confirmed good:** MCU firmware runs; UART readable; IS31 board powered (5 V), TP3 SDA/SCL = 3.0 V
  when disconnected (pull-ups fine).
- **Symptom:** connected + running → every I²C address "ACKs" and SDA/SCL sit at 0 V = **SDA held
  low**. That's not an address/firmware bug (pins verified); it points at **wiring** — the IS31
  SDA/SCL must land on **P1.2/P1.3** (not UCB1's P3.2/P3.6 or P4.3/P4.4), GND shared, no SDA↔SCL or
  SDA↔GND short.
- **Next step:** re-verify the physical SDA→P1.2 / SCL→P1.3 continuity end-to-end; then flash
  `examples/i2c-is31` (now timeout-guarded) + `just monitor` and read `[scan]` / `[is31] init`.
  Expect a single scanned address (0x60–0x63) once the bus is right.

## Key gotchas learned (don't relearn these)

- **All addresses "ACK" on a scan = SDA stuck low** (electrical), not real devices.
- **UART garbled until the DCO/FLL is set to a precise 1 MHz.** Must do the SCG0-off → set
  CSCTL1/2/3 → wait `CSCTL7 & FLLUNLOCK==0` → set CSCTL4 → settle. The "no SCG0, close enough"
  version garbles the first bytes. (Fixed in i2c-is31 and now in diag.)
- **SBW attach RESETS the chip** (PC comes up at reset vector) — `md` shows post-reset state, not
  live. Use breakpoints (`setbreak`/`run`) to observe running state; `run` blocks until a BP/interrupt.
- **App UART = `/dev/cu.usbmodem23203`**, FET/debug = `usbmodem23201`. `just monitor` defaults to 23203.
- **P2.0 is the crystal XOUT** on this chip — never use it as GPIO. SDB moved to **P2.5**.
- **msp430 inline asm needs** `#![feature(asm_experimental_arch)]` + `core::arch::asm!("bis #0x40, r2")`
  for SCG0 (SR = r2).

## FR2476 pin quick-reference (from datasheet, grounded via `nm`/tables)

- **UCB0 I²C:** SDA=P1.2, SCL=P1.3 (`P1SELx=01`, default). Remapped alt: P4.5/P4.6 (SYSCFG2 USCIB0RMP).
- **UCB1 I²C:** SDA=P3.2, SCL=P3.6 (default) or P4.3/P4.4 (remapped).
- **UCA0 UART:** TX=P1.4, RX=P1.5 (`P1SELx=01`) → eZ-FET backchannel.
- **LED1=P1.0**, **P1.1=TMP235 temp sensor**, **P1.6=button S1**, **P2.3=button S2**, **P2.0/P2.1=32k crystal**.
- SFR addresses (nm): WDTCTL 0x01CC, PM5CTL0 0x0130, P1OUT 0x0202, P1DIR 0x0204, P1SEL0 0x020A,
  P2OUT 0x0203/P2DIR 0x0205/P2SEL0 0x020B, UCB0CTLW0 0x0540/BRW 0x0546/TXBUF 0x054E/I2CSA 0x0560/IFG 0x056C,
  UCA0CTLW0 0x0500/BRW 0x0506/MCTLW 0x0508/TXBUF 0x050E/IFG 0x051C, CSCTL1 0x0182/2 0x0184/3 0x0186/4 0x0188/7 0x018E.

## Rust toolchain / PAC / HAL

- One Docker image (`msp430-c-rust:local`) = msp430-gcc + pinned Rust nightly-2025-06-25 + `just`.
  `just bootstrap` builds it. Builds recompile `core` from scratch (~3–4 min under emulation).
- **PACs vendored** in `pac/msp430fr2476` and `pac/msp430fr2433` (svd2rust; regen via `just pac gen`,
  see `scripts/gen-pac.sh` + skill `references/rust-pac.md`). Public equivalent: EnmanuelParache's
  `msp430fr247x` PAC. The crates.io `msp430fr2476` is YANKED.
- **No off-the-shelf HAL for FR24xx** (`msp430fr2x5x-hal` is FR2355-only). Our "HAL" = thin PAC
  wrappers in `diag/src/{i2c,uart,is31}.rs`. A board/BSP crate (typed pins, chip via cargo feature)
  is the planned next layer.
- **`diag/` (PAC-based) is canonical**; `examples/i2c-is31` (raw SFRs) was a bring-up vehicle — retire
  or port it onto the PAC once the bus works.

## Just done this session (fixes 1–4)

1. `diag`: SDB moved P2.0 → **P2.5** (P2.0 = crystal).
2. `diag`: proper SCG0/FLL-lock clock init (was the garbled-UART cause).
3. `diag/i2c.rs`: **timeout-guarded** polls (SPIN=4000) — no more hangs; returns false on stuck bus.
4. `.gitignore`: blanket `target/`; unstaged committed PAC/example target dirs.

## Port-config verification (added after fixes 1–4)

- **Pin function is the 2-bit pair `PxSEL1:PxSEL0`** (00=GPIO, **01=primary module**, 10/11=alt).
  The old code set only `P1SEL0` and *relied on reset default `P1SEL1=0`* — an unverified
  assumption. `diag/main.rs` now clears `P1SEL1` for the eUSCI bits explicitly before setting
  `P1SEL0`, so a stray SEL1 bit can't silently select the wrong function.
- **`diag/src/regs.rs` — one-shot boot register dump** (`regs::dump`, called after init).
  Prints live SFRs so we reason from ground truth, not source: SYSRSTIV (reset cause),
  CSCTL1–7, P1/P2 SEL1/SEL0/DIR/OUT/REN/IN, eUSCI_B0 (CTLW0/BRW/I2CSA/STATW/IFG),
  eUSCI_A0 (CTLW0/BRW/MCTLW), PM5CTL0 — plus a **decoded per-pin verdict** (`P1.2 sel=01
  module dir=I want=UCB0SDA (01)`). This is how we now *know* the mux, vs infer it.
  `hex16` helper added to `uart.rs`. Ports are byte-wide (u8) → widened to u16 for the helpers.
- Independent cross-check without firmware: halt in mspdebug and `md 0x020a`/`md 0x020c`
  (P1SEL0/P1SEL1). Electrical: muxed I2C pins are open-drain (idle-high via IS31 pull-ups).

diag builds clean (3916 B). Verified to compile; **not yet flashed/hardware-tested** — first
flash should read the dump and confirm `P1SEL1=0000`, P1.2/1.3 `sel=01`, CSCTL7 unlock bits clear.

## Open items

- [ ] **Fix the physical I²C wiring** and confirm the IS31 responds (the live blocker).
- [ ] `board/connections.toml` package field says `RHB VQFN-40` — **wrong** (RHB is 32-pin; LaunchPad
      target is TPT/48-LQFP). Confirm the real bob-929 package and correct it.
- [ ] Verify **FR2433** I²C pins from *its* datasheet (different chip) before trusting P1.2/P1.3 there.
- [ ] Flash + hardware-test the updated `diag` (clock, SDB, timeouts).
- [ ] Consider retiring `examples/i2c-is31` in favor of `diag`.
- [ ] Optional: shared `CARGO_TARGET_DIR`/sccache to cut the ~4-min per-example `core` rebuild.
