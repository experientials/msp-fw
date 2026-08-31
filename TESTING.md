# msp-fw — test scenarios to automate

The catalog of what we *should* cover with automated tests, and at which layer each belongs. This
is the target, not a status report: almost nothing here is wired up yet (there are no `#[test]`s in
the tree today). It exists so we add tests deliberately — starting with the cheap, high-signal
pure-logic ones — rather than relying on eyeballing the `just monitor` stream.

Companion to [diag/DIAGNOSTICS.md](diag/DIAGNOSTICS.md) (what each *check* verifies) and
[diag/DESIGN.md](diag/DESIGN.md). Pin/device truth: [crates/bsp/connections.toml](crates/bsp/connections.toml).

## Four layers (fastest/cheapest first)

| Layer | Runs where | Needs hardware | What it can prove |
|---|---|---|---|
| **1. Unit** (`cargo test`, host std) | CI + laptop, seconds | no | pure logic: scheduler timing, decoders, math, classification |
| **2. HIL** (flash + parse UART) | bench / Pi with a board attached | yes | the POST actually enumerates and verdicts correctly on real silicon |
| **3. Soak** (`--features stress`) | bench, nightly, minutes–hours | yes | sustained I²C margin/reliability; no WDT reset-loop |
| **4. Build gates** (`just check`) | CI, every push | no | warning-free, ROM budget, all variants + PACs compile |

Layer 1 is where most *logic* bugs are cheapest to catch and is almost entirely unbuilt — prioritize
it. Layers 2–3 are the only place electrical/timing truth lives, but they need a board, so they run
on the bench/Pi, not on cloud CI.

---

## Layer 1 — Unit tests (host, no hardware)

These target logic with **no `Peripherals` dependency**. Some functions are already pure and
testable as-is; others have the pure core tangled with UART I/O and need a small **seam refactor**
first (extract the decision into a pure fn, keep the printing in the caller). Both are listed;
"needs seam" flags the refactor.

### `crates/sched` — the scheduler (fully pure, highest priority)

`Task<C>` / `Slot` / `tick` are hardware-agnostic by design (the crate doc even calls out host unit
tests). Use a mock `Task` that records calls + returns a scripted next-period, and a hand-advanced
`now`. Scenarios:

- **Fires when due, not before** — `Slot::every(50, t)` runs on the first `tick` (due starts at 0),
  then not again until `now` reaches `due`.
- **Reschedules on returned period** — `poll` returning `Some(5)` moves the next due to `now+5`;
  returning `None` keeps the existing period.
- **Non-accumulating skew** — after a late run, next due is `now+period` (from *now*, not old due):
  a slow pass causes a one-off skew, never a catch-up burst.
- **Multiple slots run in array order** each pass; `tick` returns the count that ran (0 when idle).
- **Deadline telemetry** — `runs` increments each run; `max_late` tracks the largest lateness;
  `overruns` increments only when lateness `> period`. Feed a scripted `now` and assert all three.
- **u32 wrap-safety (the subtle one)** — the `now.wrapping_sub(due) < 0x8000_0000` guard must read
  "due" correctly across the millisecond counter wrapping at `u32::MAX`. Cases: `due` just below
  wrap and `now` just above (should fire); `now` far *before* `due` (future, should not fire). This
  is pure arithmetic — trivial to test, easy to get wrong, and a silent bug in production if wrong.

### `regs::reset_cause(v: u16) -> &str` — SYSRSTIV decode (pure, do this)

Table-driven: every documented code maps to its datasheet string (`0x02`→SVS, `0x16`→WDT timeout,
`0x14`→software POR, `0x24`→FLL unlock, …) and an undocumented code falls to the default. Guards the
decode table against a copy-paste/off-by-one error we'd otherwise only notice by misreading a live
reset.

### `mc6470::isqrt(u32) -> u16` — integer sqrt (pure, do this)

Compare against a reference (floor of f64 sqrt) across: 0, 1, perfect squares, values just
above/below a perfect square, and `u32::MAX`. Backs the milli-g report.

### MC6470 gravity classification (needs seam)

Extract the decision from `gravity_check` into a pure `classify(x, y, z) -> GravityVerdict`
(`{ mg, verdict: Pass | OutOfRange | NotReady }`), leaving the UART prints in the caller. Then test:

- at-rest 1 g on each single axis → Pass, `mg ≈ 1000`
- 1 g split across axes (the real `x≈970 y≈100 z≈300 → 997 mg`) → Pass
- `(0,0,0)` → **NotReady** (the cold-boot wake-latency sentinel we just fixed — this is the
  regression guard for that bug, at the logic level)
- 2 g / 0.3 g magnitude → OutOfRange
- band edges: just inside/outside 0.7 g and 1.3 g (`G_LO2`/`G_HI2`)

### `diag::is_expected(addr)` — config-drift membership (needs tiny seam)

Move `DEVICES`/`is_expected` somewhere host-testable (they don't touch hardware). Assert every
registry address is "expected" and a non-registry address (e.g. `0x68`, the ICM we *don't* have)
is not — this is the core of the config-drift `?` flagging.

### UART formatters `uart::dec` / `hex8` and `mc6470::dec_i16` (needs seam)

Today they push bytes straight to the eUSCI register. Give them a `&mut impl core::fmt::Write` (or a
byte sink) target and the hardware path becomes a thin adapter. Then test: `dec(0)`, max `u16`,
`dec_i16` negative/zero/positive, `hex8` zero-padding. Low glamour, but these format every number a
technician reads.

### Stress BRW ladder `stress::LADDER` (pure)

Assert the `kHz → UCB0BRW` mapping (SMCLK/brw) is monotonic and matches the documented steps
(100/200/333/500/1000 kHz ↔ 10/5/3/2/1), so a divider typo can't silently mislabel a sweep line.

---

## Layer 2 — Hardware-in-the-loop (flash a build, parse the backchannel)

The POST emits **deterministic text** at 9600 8N1. A harness (pytest / shell over `just monitor`)
flashes a build, waits for a full cycle, and asserts on the stream. **Precondition:** freeze a
**machine-readable summary line** for the harness to key on (DIAGNOSTICS.md already calls for this)
— the human-formatted lines are fine for eyeballing but brittle to parse. The build stamp
(`env!("DIAG_BUILD")` in every banner) lets the harness confirm it's asserting against the firmware
it just flashed, not a stale image.

### Golden POST — fully populated board

- `reset: none` on a clean power-up; `FLL: LOCKED`; bus idle `SDA=H SCL=H`.
- I²C scan lists **exactly** the six expected addresses (`0x0C 0x29 0x39 0x3C 0x4C 0x60`), **no `?`**,
  **no `UNEXPECTED`**.
- inventory `6/6 present`; WHO_AM_I `VL53L0X id=EE OK`, `APDS-9960 id=AB OK`.
- gravity within band (`|a|` in 700–1300 mg), `summary: 3 passed`.
- `OLED rendered`, `IS31 init OK show OK`, bus `H/H` after each display write.

### Fault-injection scenarios (each asserts the *right* failure, not just "not green")

- **Missing device** — unplug one → inventory `5/6 present, 1 missing`; gravity → **skip** if the
  MC6470 is the one pulled; summary still counts correctly. (Missing ≠ fault today — becomes a fault
  only once the per-product manifest lands; add that assertion when it does.)
- **Config drift** — wire an unexpected part (or any extra ACK) → scan prints `0xNN?` and
  `(1 UNEXPECTED)`, `t_scan` → **FAIL**, summary shows `1 FAILED`.
- **Stuck bus** — hold SDA low → bus idle `SDA=L`, `recover()` runs, prints `(recovered)` or
  `(recover FAILED - short?)`. Assert it never hangs (bounded-spin invariant).
- **WHO_AM_I mismatch** — (harder to inject physically) a wrong-ID part → `MISMATCH want=EE`,
  device inventory → FAIL.
- **Cold-boot gravity latency** — power-cycle and capture the **first** POST cycle: it must **not**
  print `OUT-OF-RANGE`/`FAILED` from an all-zero read. Either a valid `|a|` or a `skip`. Regression
  guard for the wake-poll fix at the HIL level.

### Live-behavior scenarios

- **Radar motion window** — `radar window (P2.4)` flips `idle → motion` with a trigger. Fully
  automating needs an actuator to wave; realistically a **semi-manual** or fixture test. Document it
  as manual until a bench actuator exists.
- **Reset-cause decode** — force a WDT timeout (block a task past ~16 s in a special build) or a
  software POR; assert the **next** boot prints the matching `reset:` string. Automatable via a
  deliberately-misbehaving test build.
- **Scheduler telemetry sanity** — over several minutes: the `sched:` line appears ~every 10 s;
  `post` stays `late0ms`/`ovr0`; the short tasks' `max_late` is bounded (≈ one POST-pass duration,
  the observed ~1.8 s) and does **not** grow unbounded toward the WDT window. Growth = a task
  misbehaving.

---

## Layer 3 — Soak / stress (`--features stress`, nightly)

- **Margin sweep completes without reset** — the sweep prints a line for **every** rung including
  `1000 kHz` (PASS *or* FAIL) and reaches the `margin:` summary. Direct regression guard for the
  WDT-overrun reset-loop bug (slow-failing 1 MHz reads used to reset the chip before printing).
- **Margin threshold** — highest clean SCL **≥ 400 kHz** = real headroom over the 100 kHz operating
  clock.
- **Soak clean** — at the swept ceiling, **0 nack + 0 corrupt** over the window; one report line per
  `REPORT_EVERY_SECS`; latency within band.
- **Counter longevity** — the millions-split totals (`txn_m`/`txn`) increment correctly and don't
  wrap on a multi-hour run (a bare u32 wraps at ~12 days @ ~4 k txn/s).
- **Verified-read catches corruption, not just NACK** — the whole point of comparing a known
  register value; if feasible, inject a corruption path in a test build and assert `corrupt` counts.

---

## Layer 4 — Build / CI gates (no hardware, every push)

Wire into `just check` / CI:

- **Warning-free** — `RUSTFLAGS="-D warnings"` on the diag build (would have caught the `Test.name`
  unused-field warning automatically).
- **ROM budget** — assert `text` size under a ceiling; warn as it approaches 64 KB, and track the
  ~15.5 KB FR2433-viable target for the trimmed (raw-I²C, no graphics-stack) build.
- **All variants compile** — default POST **and** `--features stress`.
- **Host unit tests pass** — `cargo test` for `crates/sched` (and the extracted pure cores once the
  seams exist).
- **PAC regen is reproducible** — `just pac check` for both FR2433 and FR2476.
- **Examples build** — `just example build …` for the smoke-test binaries.

---

## Priority order (what to build first)

1. **`crates/sched` unit tests** — pure, deterministic, and the wrap-safe `tick` is exactly the kind
   of arithmetic that fails silently. Cheapest high-value win.
2. **`reset_cause` + `isqrt` unit tests** — trivial, guard two decode/math tables.
3. **CI warning-free + ROM-budget gate** — mechanical, prevents regressions we've already hit.
4. **Gravity `classify` seam + unit tests** — locks in the cold-boot fix at the logic level.
5. **HIL golden-POST + fault-injection harness** — the highest-value hardware coverage, but needs a
   frozen machine-readable summary line and a board on the bench/Pi first.
6. **Soak assertions** — fold into the nightly stress run.

## Deliberately manual / out of scope

- Physical actuation (waving at the radar, moving the board to tilt gravity onto a chosen axis) —
  semi-manual until a bench fixture exists.
- Destructive over-stress (over-voltage/temp beyond spec) — never.
- CPU/throughput micro-benchmarks — meaningless for a supervisor MCU.
