# msp-fw backlog — tracked tasks & bugfixes

Durable list so findings don't get lost in chat. Each item: **severity · area · evidence · fix ·
status**. Check the box when done. Keep newest context at the top of each item.

---

## Bugs — found by exact console-output vs source comparison (FR2355 bench, 2026-09-30, build hd2355)

- [ ] **BUG-1 · regs.rs pin-expectation table is FR2476-specific** — *medium (diagnostic accuracy)*
  - `diag/src/regs.rs:166-167` hardcode `P1.4="UCA0TXD"`, `P1.5="UCA0RXD"` (the FR2476 UART pins).
    On FR2355 the console UART is **UCA1 on P4.2/P4.3**, so the SFR dump prints
    `P1.4 want=UCA0TXD sel=00` — misleading a bench operator into thinking the UART is misconfigured.
  - Evidence: FR2355 console dump shows the P1.4/5 "want=UCA0…" lines while the UART actually works
    on UCA1 (the *register* dump `[console UART] UCAx…` — already board-aware — is correct).
  - Fix: make the pin-expectation table board-aware (have `board` expose the UART pins/instance to
    check), so the dump reports the pins the active board actually uses.
  - Note: I fixed the UART *register* dump (`board::console_uart_regs`) but missed this *pin* table.

- [ ] **BUG-2 · regs.rs chip-id recognition lacks FR2355/FR2155/FR2433** — *low (cosmetic)*
  - `diag/src/regs.rs:40-42` `chip_name()` maps only `0x832A`→FR2476, `0x832B`→FR2475. FR2355 (TLV
    id `0x830C` observed on the bench) → prints `MSP430 (unrecognized)`.
  - Fix: add the FR2355/FR2155/FR2433 TLV device-ids **after confirming each from the datasheet**
    (do not add `0x830C` from a guess — cite the source).

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

## Convergence / tech-debt — from the diag/prod → bsp work (STEM-DIRECTION.md)

- [ ] **CONV-1 · prod `clock.rs`/`uart.rs` → `bsp::board`** — *medium (remaining duplication)*
  - The hal/i2c fork is eliminated (both use `bsp`), but prod still has its own family-generic
    `clock.rs`/`uart.rs`. Fold them into the `board` semantic map (`console_uart`, clock) so the board
    layer is the single source. The next convergence layer after the hal/i2c de-fork.
- [ ] **CONV-2 · `bootrec.rs` de-hardcode** — *low (dormant)*
  - `diag/src/bootrec.rs` still `use msp430fr2355::Peripherals` and is unwired. Route it through
    `bsp::pac` (+ `board`) before wiring it in, else it breaks non-fr2355 builds.
- [ ] **CONV-3 · Stem-I²C slave in `bsp::board`** — *feature (prod milestone)* · [GitHub #2](https://github.com/experientials/msp-fw/issues/2)
  - Add the slave/target uplink (`stem_i2c`) role; needs a non-embedded-hal target abstraction.
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
- [ ] **EXT-2 · `hwd.toml [msp] family = "fr247x"` stale for FR2355 work** — the cross-family guard
  didn't refuse an FR2355 flash (FR2355 = "unknown-family" to thepia). Minor; revisit.
