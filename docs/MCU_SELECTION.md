# 202 Combi — Supervisor MCU Selection

Tracking for the MCU planned on the [202 Combi Camera Module](../Hardware/202/202-MODULE.md).
The v0.2 board reserves space for an MCU but does not populate one.

## Selection dimensions (CANONICAL — verbatim, 2026-09-21)

> price, one or two I2C, ADC for reading sensors, number of free GPIO, RAM, FRAM, SPI for reading
> sensors. Those dimensions are obvious to me, could be others

Firmware-fit note (Henrik): **FRAM is the gate** — the model must hold the firmware image; filter the
candidate list against it, and drop any part flagged do-not-use. Derived "could-be-others" to weigh
(from this doc + STEM-EXPANDER.md, not overriding the verbatim list): always-on/standby current (the
real decider for the always-on role), wake source (ADC-window comparator vs eCOMP), LCSC/JLCPCB
availability, package/footprint, temp range. See the filtered recommendation section below.

## Full FRAM lineup + JLCPCB availability (research 2026-09-21)

Datasheet-verified specs (SLASE78E/SLAS879/SLASEC4D/SLASEO7C/SLAS865F + TI product pages; all parts
**Active**, no NRND/obsolete) merged with LCSC/JLCPCB availability (point-in-time Sep 2026 — stock &
Basic/Extended flags fluctuate daily, **re-check before a BOM commit**). Prices are ballpark @1k.
Sorted by program FRAM. "gate" = passes the ≥15 KB FRAM fit-floor for the full expander image.

| Part | FRAM prog | SRAM | I²C | ADC | GPIO / pkg | Analog | LCD | ~$1k | JLCPCB (LCSC) | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| FR2000 | 0.5 KB | 0.5 | **0** | none | 12 / TSSOP-16 | eCOMP | No | ~$0.30 | not carried | ✗ no I²C, no ADC, FRAM |
| FR2100 | 1 KB | 0.5 | **0** | 10b/8 | 12 / TSSOP-16 | eCOMP | No | ~$0.37 | ~3 (risk) | ✗ no I²C, FRAM |
| FR2110 | 2 KB | 1 | **0** | 10b/8 | 12 / TSSOP-16 | eCOMP | No | ~$0.45 | ~350 thin | ✗ no I²C, FRAM |
| FR2310 | 2 KB | 1 | 1 | 10b/8 | 16 / VQFN-16 | eCOMP+opamp+TIA | No | ~$0.55 | unverified | ✗ FRAM |
| FR2111 | 3.75 KB | 1 | **0** | 10b/8 | 12 / TSSOP-16 | eCOMP | No | ~$0.55 | 0 / OOS | ✗ no I²C, FRAM |
| **FR2311** | 3.75 KB | 1 | 1 | 10b/8 | 16 / VQFN-16 | eCOMP+opamp+**TIA** | No | ~$0.68 | **Basic ~15k ✓✓** | ✗ FRAM (⚠ minimal-build candidate — see rules) |
| FR4131 | 4 KB | 0.5 | 1 | 10b | 60 / TSSOP-48 | none | **Yes** | ~$1.1 | unverified | ✗ LCD, FRAM |
| FR2422 | 7.25 KB | 2 | 1 | 10b/8 | 15 / TSSOP-16 | none | No | ~$1.1 | ~3.9k ✓ | ✗ FRAM (+prior do-not-use) |
| FR2032 | 8 KB | 1 | 1 | 10b/10 | 60 / TSSOP-48 | none | No | ~$0.85 | unverified | ✗ FRAM |
| FR4132 | 8 KB | 1 | 1 | 10b | 60 / TSSOP-48 | none | **Yes** | ~$1.4 | unverified | ✗ LCD, FRAM |
| FR2033 | 15 KB | 2 | 1 | 10b/10 | 60 / TSSOP-48 | none | No | ~$1.1 | not on LCSC (consign) | ✓ 1×I²C, 60 GPIO, no analog |
| **FR2433** | 15 KB (+0.5 info) | 4 | 1 | 10b/8 | 19 / VQFN-24 | none | No | ~$1.3 | **~5.5k ✓** | **PRIMARY** |
| FR4133 | 15 KB (+0.5 info) | 2 | 1 | 10b | 60 / TSSOP-48 | none | **Yes** | ~$1.7 | ~65 thin | ✗ LCD |
| FR2153 | 16 KB | 2 | **2** | 12b/12 | 44 / VQFN-32 | eCOMP×2 | No | ~$0.95 | not carried (consign) | ✓ 2×I²C, cheap |
| FR2353 | 16 KB | 2 | **2** | 12b/12 | 44 / VQFN-32 | **SAC×4**+eCOMP×2 | No | ~$2.2 | not carried (consign) | ✓ 2×I²C+SAC |
| FR2155 | 32 KB | 4 | **2** | 12b/12 | 44 / VQFN-32 | eCOMP×2 | No | ~$1.2 | ~95 thin | ✓ 2×I²C (best-stocked 2×) |
| FR2355 | 32 KB | 4 | **2** | 12b/12 | 44 / VQFN-32 | **SAC×4**+eCOMP×2 | No | ~$2.7 | ~4 (consign) | ✓ 2×I²C+SAC |
| FR2475 | 32 KB | 6 | **2** | 12b/12 | 43 / VQFN-32 | eCOMP | No | ~$1.5 | not carried (consign) | ✓ 2×I²C |
| FR2476 | 64 KB | 8 | **2** | 12b/12 | 43 / VQFN-32 | eCOMP | No | ~$1.9 | pre-order (consign) | ✓ 2×I²C; over-spec (A/B gone), scarcest |

## Exclusion rules (DRAFT — for Henrik's review)

**FRAM is the gate.** Only a few rules actually *exclude* a part; everything else ranks the survivors.
JLCPCB availability is an **indicator, not a gate** — JLCPCB can populate boards with customer-supplied
(consigned) inventory, so a part that's thin/absent on LCSC is a *sourcing cost/lead-time* signal, not
a disqualifier.

**Hard gates (rule a part OUT):**
- **G1 — program FRAM < image size.** Full-expander baseline ≈ 15 KB → **< 15 KB = OUT** (everything
  below FR2033/FR2433). _Adjustable:_ a feature-gated **minimal** build (regmap + I²C-slave only) could
  lower the floor and re-open a small part — the attractive sub-15 KB target is **FR2311** (3.75 KB,
  I²C+ADC+TIA, fee-free Basic, ~$0.68).
- **G2 — no hardware I²C (no eUSCI_B).** Kills FR2000/2100/2110/2111 (bit-bang only; not a reliable
  slave control surface).
- **G3 — no ADC.** Kills FR2000 (can't voltage-sense VSOM/CHARGE).

**Ranking indicators (raise cost/risk, do NOT exclude):**
- **JLCPCB/LCSC availability** — favors FR2433 (well-stocked, cheapest to source); others (FR2153/
  FR2353/FR2475/FR2033 not carried, FR2476 pre-order, FR2155/FR2355 thin) just need **consigned
  inventory** (extra cost/lead-time), not ruled out.
- **Segment-LCD (FR41xx)** — usable but pays pins+cost for an unused LCD driver; avoid unless a display
  is actually wanted.
- Price @1k; package/pin over-spec; analog (SAC/eCOMP) over-spec; each extra distinct part = one more
  firmware permutation. `1× I²C` = slave-only expander; `2× I²C` only when a local sensor-master is needed.

**Net picture (after the gates; JLCPCB as a tiebreaker, not a filter):** the recommended band is every
**≥15 KB, ≥1 I²C, ADC-equipped, non-LCD** part — FR2033, FR2433, FR2153, FR2353, FR2155, FR2355, FR2475,
FR2476.

### Update model (settles the A/B question)

**Update = host-driven BSL reflash; the i.MX8 SoM is root of trust.** The MSP430 has no network, so
every image arrives *through* the SoM anyway; the SoM enters the ROM BSL over RST/TEST and rewrites
(password-gated after first write — STEM-EXPANDER.md), and a half-written image is recoverable by
re-entering BSL (BSL isn't in the application FRAM → not a brick). **On-MCU A/B self-update is a
distant priority / effectively irrelevant** (Henrik, 2026-09-21): its only real benefit is
zero-downtime supervisor update, which doesn't apply here (the SoM is awake pushing the update, so the
"supervise while host sleeps" duty is idle). **Consequence: A/B does NOT drive part choice.** Each part
runs **one full image = its full program FRAM** (no 2-bank halving), and the 64 KB FR2476 is no longer
specially favored.

## Two node roles → two tracked ranges

The product has two distinct node roles; **each is its own firmware family (a separate build)**, and
both are tracked here — we do not drop the single-I²C parts.

1. **Dual-I²C node** — Stem slave **and** local sensor-bus master. **PREFERRED** (Henrik: "I strongly
   favor dual I2C capability").
2. **Single-I²C node** — Stem **slave-only** expander (the one I²C is the control surface; local
   sensing on ADC/GPIO/SPI). Cheaper, smaller, best-stocked; still needed for GPIO/ADC-only domains.

### Dual-I²C range (2× eUSCI_B)

2× eUSCI_B + 2× eUSCI_A, 12-bit 12-ch ADC, 44 GPIO (VQFN-32/LQFP-48). Differ on FRAM, RAM, analog, sourcing:

| Part | FRAM (full image) | RAM | Analog | ~$1k | JLCPCB | Niche |
|---|---|---|---|---|---|---|
| FR2153 | 16 KB | 2 KB | 2× eCOMP | ~$0.95 | consign | cheapest dual-I²C |
| **FR2155** | **32 KB** | 4 KB | 2× eCOMP | ~$1.2 | **~95 (best-stocked 2×)** | **← baseline pick** |
| FR2353 | 16 KB | 2 KB | **4× SAC** + 2× eCOMP | ~$2.2 | consign | analog front-end, small |
| FR2355 | 32 KB | 4 KB | **4× SAC** + 2× eCOMP | ~$2.7 | consign | analog front-end |
| FR2475 | 32 KB | 6 KB | 1× eCOMP | ~$1.5 | consign | more RAM |
| FR2476 | 64 KB | 8 KB | 1× eCOMP | ~$1.9 | pre-order/consign | over-spec now (A/B gone) + scarcest |

- **Baseline: FR2155** — 32 KB full image, best LCSC stock of the six, 2× eCOMP (dual-rail VSOM+CHARGE wake).
- **Cheaper/smaller:** FR2153 (16 KB) if the image fits and stock isn't a concern.
- **Analog conditioning (opamp/PGA/DAC):** FR2353/FR2355 (SAC) only if a domain needs in-chip front-ends.
- **FR2476:** with A/B deprioritized, 64 KB is over-spec and it's the worst-sourced — reconsider only
  if a domain genuinely needs >32 KB code or 8 KB RAM.

### Single-I²C range (1× eUSCI_B) — slave-only expander

| Part | FRAM | RAM | ADC | GPIO / pkg | Analog | ~$1k | JLCPCB | Niche |
|---|---|---|---|---|---|---|---|---|
| **FR2433** | 15 KB | 4 KB | 10-bit 8ch | 19 / VQFN-24 | none | ~$0.54–0.88 | **well-stocked** | **← baseline slave-only** |
| FR2033 | 15 KB | 2 KB | 10-bit 10ch | 60 / TSSOP-48 | none | ~$1.1 | not on LCSC (consign) | big I/O fan-out, large pkg |
| FR2311 | 3.75 KB | 1 KB | 10-bit 8ch | 16 / VQFN-16 | eCOMP+opamp+TIA | ~$0.68 | **Basic (fee-free)** | ultra-cheap minimal *iff* image ≤3.75 KB |

- **Baseline: FR2433** — cheapest, smallest (VQFN-24), best-stocked; the default 1× I²C node.
- **FR2033** — 60 GPIO for a big I/O fan-out, but TSSOP-48 and not LCSC-stocked.
- **FR2311** — only if the image feature-gates under 3.75 KB; then fee-free Basic + analog (TIA) at ~$0.68.

### FR2155 vs FR2433 — the two baselines head-to-head

| Dimension | **FR2433** (1× I²C) | **FR2155** (2× I²C) | Edge |
|---|---|---|---|
| Role | slave-only expander | slave + sensor-master | 2155 (preferred) |
| I²C / total eUSCI | 1 / 3 | 2 / 4 | 2155 |
| UART/SPI (eUSCI_A) | 2 | 2 | tie |
| Program FRAM | 15 KB (+0.5 info) | **32 KB** (+0.5 info) | 2155 (2.1×) |
| SRAM | 4 KB | 4 KB | tie |
| ADC | 10-bit, 8 ch | **12-bit, 12 ch** | 2155 |
| Analog wake | none (ADC-window comp only) | **2× eCOMP** + 6-bit ref DAC | 2155 (dual-rail) |
| Free GPIO | 19 / VQFN-24 (~3 banks) | **44 / LQFP-48** (~5 banks) | 2155 |
| Max CPU | 16 MHz | 24 MHz | 2155 |
| Standby LPM3.5 | ~710 nA | ~620 nA | ~tie |
| Active | ~126 µA/MHz | 142 µA/MHz | 2433 |
| Temp | −40…85 °C | −40…**105 °C** | 2155 |
| Smallest pkg | **VQFN-24** (~4×4 mm) | VQFN-32 → LQFP-48 | 2433 |
| Price @1k | **~$0.54–0.88** | ~$1.2 | 2433 (~½) |
| JLCPCB | **well-stocked (~5.5k)** | thin (~95 → consign) | 2433 |
| Family / PAC | FR24xx (dev ID 0x8240) | FR215x | separate build each |

FR2155 wins the capability axes (2× I²C, 2× FRAM, 12-bit ADC, 2× eCOMP, more GPIO, faster, wider temp);
FR2433 wins the practical axes (~½ price, well-stocked, small VQFN-24). Different families ⇒ one build
each — which maps cleanly onto the two roles.

### Dev boards
- **FR2155 target → develop on the FR2355 LaunchPad (MSP-EXP430FR2355)** — a **strict superset** (same
  FR21x/23x family + register map, adds SAC, same 2× eCOMP / 32 KB / 44 I/O). The on-hand **FR2476
  boards are a different family and one eCOMP short (1 vs 2) + one GPIO fewer** — fine for early logic
  bring-up, **not** for FR2155 sign-off.
- **FR2433 target → FR2433 LaunchPad** (the FR2476 boards can host early logic only; different family).

### Net recommendation (draft — for review)
Two **production** firmware families, one per role: **FR2155** = dual-I²C primary; **FR2433** =
single-I²C slave-only. Minimum-permutation set that honors the dual-I²C preference and keeps the cheap,
well-stocked slave-only node. Consign FR2155 for JLCPCB runs (or dual-source), or accept FR2153 (16 KB)
if its stock/price beats FR2155 at buy time.

**Development sequencing (Henrik, 2026-09-21): primary dev target = FR2476 until the FR2355 board arrives.**
The dual-I²C firmware is developed *now* on the on-hand **FR2476** boards (FR247x family — 2× eUSCI_B,
same dual-I²C role), then moves to the **FR2155** production part once an **FR2355 LaunchPad** (its
strict superset) is available to sign it off. FR2476 and FR2155 are different families (FR247x vs
FR215x) ⇒ the port is a PAC/memory.x swap; the dual-I²C *logic* carries. The one thing FR2476 can't
exercise is the 2nd eCOMP (1 vs 2) — validate that on the FR2355 board. The `prod/` crate has a
**compile-time family axis**: `fr247x` (default, FR2476/FR2475) and `fr24xx` (FR2433 single-I²C) **both
build today** — `just prod build [fr247x|fr24xx]` selects the PAC + memory map + gate budget. **FR2433 is
NOT dropped**; FR247x is only the *default*. The FR2155 production dual-I²C part joins as a future
`fr215x` family (see prod/DESIGN.md).

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
