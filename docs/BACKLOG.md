# msp-fw backlog — tracked tasks & bugfixes

Durable list so findings don't get lost in chat. Each item: **severity · area · evidence · fix ·
status**. Check the box when done. Keep newest context at the top of each item.

---

## Bugs — found by exact console-output vs source comparison (FR2355 bench, 2026-09-30, build hd2355)

- [x] **BUG-1 · regs.rs pin-expectation table is FR2476-specific** — *DONE 2026-09-30* — `board::
  console_uart_pin_report` (reads the right port); **hardware-confirmed on FR2355**: dump now prints
  `P4.2 want=UCA1RXD sel=01 module` / `P4.3 want=UCA1TXD sel=01`, not the old P1.4/5 UCA0.
  - `diag/src/regs.rs:166-167` hardcode `P1.4="UCA0TXD"`, `P1.5="UCA0RXD"` (the FR2476 UART pins).
    On FR2355 the console UART is **UCA1 on P4.2/P4.3**, so the SFR dump prints
    `P1.4 want=UCA0TXD sel=00` — misleading a bench operator into thinking the UART is misconfigured.
  - Evidence: FR2355 console dump shows the P1.4/5 "want=UCA0…" lines while the UART actually works
    on UCA1 (the *register* dump `[console UART] UCAx…` — already board-aware — is correct).
  - Fix: make the pin-expectation table board-aware (have `board` expose the UART **port + pins**), so
    the dump reads the port the active board uses. NOTE: not the same pattern as the register-dump fix —
    FR2355's UART pins are on a **different port** (P4 vs P1), so the dump must read P4SEL for fr2355.
  - Note: I fixed the UART *register* dump (`board::console_uart_regs`) but missed this *pin* table.

- [ ] **BUG-2 · regs.rs chip-id recognition lacks FR2355/FR2155/FR2433** — *low (cosmetic)*
  - `diag/src/regs.rs:40-42` `chip_name()` maps only `0x832A`→FR2476, `0x832B`→FR2475. FR2355 (TLV
    id `0x830C` observed on the bench) → prints `MSP430 (unrecognized)`.
  - Fix, split by confidence:
    - **FR2355 `0x830C` — DONE 2026-09-30**, hardware-confirmed: `chip_name` now maps it; the board
      (JTAG id `0x01ff`) prints `chip: MSP430FR2355 id=830C` (was "unrecognized").
    - **FR2433 `0x8240` — DONE 2026-09-30**, empirically read over SBW (`msp read 0x1A04`) on a
      JTAG-confirmed FR2433 (id `0x01c6`), stable across reads, past the leading `0x55` artifact.
      Added to `chip_name`. (Not firmware-confirmed — diag doesn't run on fr24xx, see CONV-4/#3.)
    - **FR2155 id = gated on FUTURE hardware** — *not just "not attached now."* Henrik 2026-09-30:
      *"You can only test fr2155 logic with fr2355 dev board until some future where I might have a
      prod board with 2155."* Reading the FR2355 dev board yields FR2355's TLV (`0x830C`), NOT
      FR2155's — so FR2155's id can't be captured until a real FR2155 board exists. Do not guess.
      Item stays open **only** for FR2155.

- [ ] **BUG-3 · WDT-timeout reset loop on a stuck/no-shield I²C bus** — *medium (functional)*
  - Evidence: `SYSRSTIV=0016` (WDTIFG) + the boot banner **reprints** between POST cycles → the board
    reboots via the ~16 s watchdog backstop. Bus reads `SDA=L SCL=L` (no sensor shield / no pull-ups).
  - Likely cause: the POST's I²C ops on a stuck bus (112-addr scan + device inits + `recover`) hog the
    cooperative loop past the 16 s backstop before `pet_wdt` runs again (the code comments even note
    "the ~3 s POST hogs the loop").
  - Fix: bound the POST's total stuck-bus time (or pet the WDT inside long POST work) so a stuck bus
    degrades gracefully instead of reset-looping. **Confirm** root cause with a sensor shield attached.
  - Caveat: not confirmed as a regression — no `fr2355run1` reset-cause captured to compare.

---

- [x] **BUG-4 · prod `model.rs` FR2355 id was a placeholder → false "WRONG-FAMILY FLASH"** — *FIXED
  2026-09-30*. `DEVICE_ID_FR2355 = 0x830C` (was placeholder `0xF235`); FR2476 `0x832A` / FR2433 `0x8240`
  were already correct. Verified on hardware: FR2355 now boots `(FR2355)`, no wrong-family warning,
  status `0x0B→0x03`. FR2155 stays a placeholder (no board — can't read its own TLV; see [[diag-prod-variants]]).

## Convergence / tech-debt — from the diag/prod → bsp work (STEM-DIRECTION.md)

- [ ] **CONV-1 · prod `clock.rs`/`uart.rs` → `bsp::board`** — *medium · DESIGN-GATED (not short-term)* · [GitHub #5](https://github.com/experientials/msp-fw/issues/5)
  - The hal/i2c fork is eliminated (both use `bsp`), but prod still has its own family-generic
    `clock.rs`/`uart.rs`. Folding them into `bsp::board` is **not just effort** — it embeds a design
    decision: prod uses **runtime model-detect + pin-map** (`model.rs`, one image per family) vs
    `bsp::board`'s **compile-time per-`<chip>_<module>`**; and prod targets the **product PCB** while
    `board` has only LaunchPad modules. Decide the board-layer philosophy first (see #5).
- [ ] **CONV-2 · `bootrec.rs` de-hardcode** — *low (dormant); PAC import DONE, full agnosticism open*
  - DONE 2026-09-30: `diag/src/bootrec.rs` now uses `crate::pac::Peripherals` (via diag's bsp re-export),
    not the hardcoded `msp430fr2355`. Not build-exercised (still `mod`-undeclared/dormant).
  - Still open: making it *truly* chip-agnostic — its snapshot fields assume the fr2355 board
    (UCA1 ctlw0, p4sel0). Finish when it's actually wired in.
- [ ] **CONV-3 · Stem-I²C slave in `bsp::board`** — *feature (prod milestone)* · [GitHub #2](https://github.com/experientials/msp-fw/issues/2)
  - Add the slave/target uplink (`stem_i2c`) role; needs a non-embedded-hal target abstraction.
- [ ] **CONV-5 · fr24xx (FR2433) has NO console/debug path** — *medium (dev ergonomics); real bring-up*
  - Symptom: prod's `uart`/console is gated `#[cfg(all(_dual, console))]`; FR2433 isn't `_dual`, so
    `--features console` is a **no-op** — console and silent builds are byte-identical (110 B). No
    banner / no debug output on an FR2433. NOT a regression: `run_fr24xx` is an explicit TODO stub, and
    console was `fr247x`-only historically → never present for fr24xx.
  - Investigated 2026-09-30 (spike paused before flashing). CORRECTED understanding: the FR2433 UART is
    **NOT** a different register model — `usci_a0_uart_mode` exposes the SAME accessors
    (`uca0ctlw0`/`uca0brw`/`uca0mctlw`/`uca0ifg`/`uca0txbuf`/`uca0rxbuf`) as the dual families' eUSCI.
    The **only** difference is the peripheral **field name**: `usci_a0_uart_mode` (FR2433) vs
    `e_usci_a0` (dual). (My first attempt used the wrong field name `usci_a0`; and I briefly mis-read a
    filtered grep as "different registers" — both wrong.)
  - Tractable fix (the spike): a `uca0!` field-name macro in `prod/uart.rs` (`e_usci_a0` for `_dual`,
    `usci_a0_uart_mode` for fr24xx; same register methods) + un-gate `mod uart`/`mod clock` for
    console + clock factory-trim + bounded-wait for fr24xx (FLL regs present, like FR2355) +
    `run_fr24xx` console (P1.4/5 pins) + flash/verify on the attached FR2433.
  - **SPIKE SUCCEEDED 2026-09-30 (uncommitted).** FR2433 prints a clean banner
    `== prod v0.1.0 fr2433/… fr24xx (FR2433 Passive) — console up ==` at correct 9600 baud → proves the
    macro (usci_a0_uart_mode), the factory-trim clock (clean baud = FLL locked at 1 MHz), and P1.4/5 UCA0
    (the FR2433 LaunchPad backchannel — confirmed) all work. fr24xx+console = ~500 B (was 110 B silent).
    Verified the sanctioned way via `thepia hwd console tail` (once EXT-3 was fixed 2026-09-30). Caveat:
    the spike loops the banner — production is print-once + serve the I2C-slave surface / ADC / GPIO (the
    real fr24xx role, GitHub #3). Next: tidy to print-once + commit.
  - Prereq-ish for CONV-4 (a useful fr2433 diag/dev build wants debug output).

- [ ] **CONV-4 · fr2433 diag role** — *design (deferred)* · [GitHub #3](https://github.com/experientials/msp-fw/issues/3)
  - FR2433 is single-I²C **slave-only** — diag's sensor-scan POST doesn't apply. Design a diag role
    (exercise the slave/expander + GPIO) when prod's fr2433 node duties exist (today it's a 110 B
    idle scaffold).

---

## Verification gaps — build-verified, not yet run on hardware

- [ ] **VER-1 · diag on FR2476 (post-refactor)** — build-verified only; run single-adapter on an
  FR2476 to confirm (same shared code, UCA0 board module).
- [ ] **VER-2 · prod on hardware (any family)** — never run (scaffold: boots/watchdog/idle). Run when
  node duties are implemented. · [GitHub #4](https://github.com/experientials/msp-fw/issues/4)
- [x] **diag on FR2355 (post-refactor)** — DONE 2026-09-30 (build hd2355): CONFIRMED RUNNING + POST;
  register values match source exactly. See `diag/proofs/console-fr2355.log`.

---

## External — thepia `hwd` (report upstream, not fixed here)

- [ ] **EXT-1 · re-file two thepia reports** — `thepia hwd report` was broken (`agent not embedded /
  --features goose`); the *flash-unguarded-under-multi-adapter* and *console-non-UTF-8* reports never
  filed. Retry when the report channel works. (The per-board routing + cache-poison report DID file.)
- [x] **EXT-3 · thepia console session pins a stale device node (console-side C32)** — *reported + FIXED
  2026-09-30*. thepia now opens/reads the FR2433 console session (`console-2FAFB46F29002500.log` exists;
  `console tail --board 2FAFB46F29002500` reads the live looping banner cleanly — em-dash renders, so the
  old non-UTF-8 `tail` crash is handled too). The FR2433 spike is now verified the sanctioned way.

- [ ] **EXT-2 · `hwd.toml [msp] family = "fr247x"` stale for FR2355 work** — *thepia-gated, not trivial* —
  the cross-family guard didn't refuse an FR2355 flash (FR2355 = "unknown-family" to thepia). thepia
  only knows families `fr247x`/`fr24xx` — there is **no `fr235x`/`fr215x`**, so there's no correct value
  to set. Fix = either drop the guard (lose it) or wait for thepia to add the dual-family designations.
