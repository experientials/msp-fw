# diag — diagnostics & stress catalog

What diag can check, how deep, and the pass criteria. Companion to [DESIGN.md](DESIGN.md) (the
*direction*) and the connection registry [../crates/bsp/connections.toml](../crates/bsp/connections.toml)
(the pin source of truth). This file is the *catalog* — what each check verifies and what a failure
means.

## The coverage ladder

Every element climbs the same rungs; "present" is nearly worthless alone — the value is upward.

**presence (ACKs) → identity (WHO_AM_I) → config-match (right part, right place) → function
(front-end actually works) → event (interrupts/wakes fire) → trend (drift across passes)**

| Subsystem | Coverage today | Next rung |
|---|---|---|
| I²C bus | idle levels, stuck-bus recover, full scan, **clock margin + soak** (stress) | — |
| IS31FL3730 | function (visual self-test) | — |
| SSD1306 OLED | function (render) | — |
| APDS-9960 | identity + function (proximity task) | ALS; INT-line event |
| VL53L0X | identity (WHO_AM_I) | function (range a target) |
| MC6470 (eCompass) | presence + **function (gravity ≈ 1 g)** | identity (WHO_AM_I); magnetometer |
| RCWL-0516 radar | function (motion window) | duty/"level" |
| Rails (DVCC internal) | **function — measured DVCC via ADC** | external VSOM/CHARGE rails + thresholds |
| Wake / event | none | prove one wake source |
| Config-drift | scan flags UNEXPECTED addresses | expected-set manifest per product (hard-fail missing) |
| MCU self | reg dump; reset-cause (SYSRSTIV); clock-lock; board id (chip/rev/die serial, TLV); **measured DVCC + die temp (ADC)** | FRAM CRC; per-interval trend/thresholds |

## Reporting model

- Measurements report in **bands**, not booleans: a value plus PASS / WARN / FAIL against a
  documented threshold (e.g. `|accel| ≈ 1 g`, rail `> 3.15 V`).
- Keep a **machine-readable summary line** frozen for CI/production to assert against.
- Every FAIL should carry its likely **cause** (the bus check already distinguishes "recovered" vs
  "short") — that's what makes a red result actionable.

## Stress & margin mode

A distinct build (`--features stress`) that replaces the POST with a bus-hammering runner. It exists
because the gentle 3 s POST proves *presence*, not *margin* or *sustained reliability* — the failures
that actually bite an always-on device (weak pull-ups, a warm intermittent connector, drift after
hours) are invisible to it.

### Run it

```sh
just diag stress        # build (--features stress) + flash
just monitor            # watch results, 9600 8N1
# restore the normal POST afterwards:
just diag run
```

### What it does

1. **Clock-margin sweep** — raises SCL `100 → 200 → 333 → 500 → 1000 kHz` (SMCLK/`UCB0BRW`) and, at
   each step, does 200 **verified** reads per ID-register target (APDS-9960 `0x39`, VL53L0X `0x29`):
   read a known-valued register and compare. "Verified" catches **bit corruption**, not just NACKs.
   Reports per-step `txn / nack / corrupt` and the **highest clean SCL** = the bus margin.
2. **Error-rate soak** — runs at the **fastest clean clock the sweep just found** (1 MHz when the
   bus passes there; falls back to 100 kHz if nothing was clean) and reads continuously,
   accumulating totals across the **whole run** (not reset per window) so a multi-day soak produces
   one running verdict — "*N errors in M transactions over T seconds*" — the actual pass artifact.
   One report line every `REPORT_EVERY_SECS`. Runs **indefinitely** by default
   (`SOAK_LIMIT_SECS = 0`); set it to bound the soak, after which it prints a final PASS/FAIL and holds.
   Soaking at the edge (not the gentle 100 kHz) is the stronger test — sustained max-speed reliability.

Instrumentation: a µs time base on TA0 (`usec.rs`, SMCLK/1) times each transaction; the WDT is fed
throughout so an indefinite soak never trips the backstop. Transaction totals are split
millions+remainder so the counter survives weeks (a single u32 would wrap at ~12 days @ ~4 k txn/s).

**Hardware note:** running for days is safe — I²C reads are non-destructive, nothing writes
endurance-limited memory, and the load is milliwatts (no thermal stress). The bounded duration is
for *reportability*, not to spare the board. (This changes only if the future thermal/power rung
drives outputs continuously — then watch OLED static burn-in and actually measure the rail.)

### Output (example shape)

```
-- I2C clock-margin sweep --
  100 kHz: 400 txn, 0 nack, 0 corrupt  OK
  200 kHz: 400 txn, 0 nack, 0 corrupt  OK
  333 kHz: 400 txn, 0 nack, 0 corrupt  OK
  500 kHz: 400 txn, 0 nack, 0 corrupt  OK
  1000 kHz: 400 txn, 0 nack, 0 corrupt  OK
  margin: highest clean SCL = 1000 kHz
-- I2C error-rate soak @1000 kHz (cumulative) --      <- soaks at the clean ceiling
  [t=10s] txns=520000  err=0+0  rate=52000/s  lat=30-70us  PASS
  [t=20s] txns=1M 40000  err=0+0  rate=52000/s  lat=28-90us  PASS
  ...
  [t=86400s] txns=4492M 800000  err=0+0  rate=52000/s  lat=25-140us  PASS
```

(If the bus fails at high clocks, the margin line reports the lower ceiling and the soak runs there
— e.g. margin 500 kHz → soak @500 kHz.)

### Pass thresholds

| Metric | PASS |
|---|---|
| Margin (highest clean SCL) | **≥ 400 kHz** — the bus has real headroom over the 100 kHz operating clock |
| Soak errors | **0** nack **and** 0 corrupt over the window |
| Latency | within expected band for the clock (~a few hundred µs at 100 kHz) |

### Interpreting failures

| Symptom | Likely cause |
|---|---|
| Fails only at high SCL (500 k/1 M), clean at 100 k | Weak pull-ups / high bus capacitance / long wiring — edges too slow. Expected margin, not a defect, unless it fails at ≤ 200 k. |
| `corrupt` > 0 at low clock | Genuine signal-integrity problem (reflections, noise, marginal device) — a real finding. |
| `nack` climbs during the soak (was clean cold) | Thermal / intermittent connector — the classic warm-failure a one-shot POST misses. |
| Latency much higher than the band | Clock stretching by a slow device, or repeated stuck-bus recovery. |

## Scheduler deadline telemetry

Not a separate build — it's **always-on in the POST**. The `sched` crate tracks, per task, how far
past its scheduled deadline it actually ran (`max_late`, ms) and how often it ran more than a full
period late (`overruns`), derived from the ms `now` (no hardware). The POST prints a line every
~10 s:

```
sched: radar(late84ms ovr7) prox(late84ms ovr3) post(late1ms ovr0)
```

This quantifies the deferred **"task running too long"** concern directly: the ~3 s `PostTask` (I²C
scan + display) hogs the loop for tens of ms, so `radar`/`prox` show that as their `max_late` and
rack up `overruns` during a POST pass, while `post` itself stays near zero. A healthy result: the
short tasks' `max_late` ≈ the POST duration and no *unbounded* growth. If `max_late` climbs toward
the WDT window (~16 s), a task is misbehaving.

### Not covered yet (backlog)

- **Display update speed** (OLED / LED-matrix frames-per-second) — worth adding as a throughput
  rung: time raw I²C frame writes (512 B to the OLED @0x3C, the matrix to the IS31 @0x60) at the
  soak clock; measures the real animation ceiling without pulling in the graphics stack.
- **Power/thermal under load** — waits on the rail ADC.
- **Cross-reset fault counters** — the soak counts in RAM; a reset zeroes it (FRAM-persist later).

Deliberately out of scope: CPU/throughput benchmarks (meaningless for a supervisor) and any
**destructive** over-stress (over-voltage/temp beyond spec).
