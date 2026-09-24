# prod — design & requirements (Stembus node firmware)

This is the authoritative design doc for the `prod/` firmware. The block below is the product
owner's requirements captured **verbatim** — it is the anchor; everything else on this page is a
*derivation* of it. If a derived section disagrees with the canonical block, the canonical block wins.

---

## CANONICAL (verbatim) — requirements

> The prod firmware needs to have some basic boot logic for enumerating the sensors on I2C and other
> detection of what other devices can be sensed. It must use one I2C bus for sensors and another for
> being a slave on the I2C MCU bus. On FR2433 the I2C would just be on the MCU bus. The firmware must
> detect what exact model it is loaded on and correctly map the pins. We would support multiple
> MSP430 models with the same binary firmware image

> If we get to the limit of firmware size, we can use feature macros to leave out blocks of
> functionality from the built firmware

> I want the least number of firmware permutations in releases. I don't yet know the exact MSP430
> models used on different destinations. Each product potentially has multiple chips. Different
> products potentially has different models. I want to limit permutations and try to determine at
> runtime if possible. Worst case the UCM SoM determines what firmware to load out of a firmware bundle

_(captured 2026-09-21)_

---

## Derived design (subordinate to the canonical block)

### Roles & buses
- The firmware is a **Stembus node** with up to **two I²C buses**:
  1. **MCU-bus SLAVE** — the control surface the SoM masters; register map in [`src/regmap.rs`](src/regmap.rs)
     (PCA9698 emulation + Thepia extensions, per [../I2C-API.md](../I2C-API.md)).
  2. **Sensor-bus MASTER** — enumerates/monitors attached I²C sensors; result proxied to the SoM.
- **On the FR2433 (1× I²C): only the MCU-bus slave exists — no sensor master.** A part with a 2nd
  I²C (e.g. FR2476, 2× eUSCI_B) runs both roles. This matches the tracked split
  ([[msp-fw-fit-fr2433]]): one I²C can be slave *or* master, not both.

### Boot logic (the "basic boot logic" required)
1. **Model detection** — read the device ID from the MSP430 **TLV device descriptor** / SYS device-ID
   registers; pick the model record at runtime.
2. **Pin mapping** — apply that model's map: which port pins are GPIO banks (regmap), which are the
   MCU-bus I²C, the sensor-bus I²C, ADC (VSOM/CHARGE), and the STEM INT/MSG line.
3. **Sensor enumeration** — if a sensor-master bus exists, probe it and record what's present /
   sensable; expose to the SoM.

### Single binary across multiple models
Requirement: **one binary image serves multiple MSP430 models**, detecting the exact model and
mapping pins at runtime. Feasible **within a register-/memory-map-compatible family** (same peripheral
base addresses + FRAM/RAM origins; models differ mainly in package/pin-count). See the OPEN QUESTION
below — this does **not** trivially extend across families whose peripheral bases or memory maps differ.

### Size management & the per-image FRAM ceiling
If the image approaches its FRAM budget (gated by [../scripts/size-check.sh](../scripts/size-check.sh)),
**feature-gate** optional blocks (Cargo `features` + `#[cfg(feature = …)]`) so functionality can be
compiled out per build. Candidate gates: sensor-master + enumeration, voltage thresholds/messaging,
extended/custom registers, OTA/flash-lock.

**Per-image budget = the part's FULL program FRAM.** Update is **host-driven BSL reflash** (the i.MX8
SoM is root of trust — it enters the ROM BSL over RST/TEST and rewrites; a half-write is recoverable by
re-entering BSL, so no on-MCU bank pair is needed). See MCU_SELECTION.md "Update model" +
[[msp-fw-ota-host-root-of-trust]]. **On-MCU A/B self-update is a distant priority / non-driver**
(Henrik, 2026-09-21) — so we do NOT reserve a second bank; a node runs one full image.
_(Prior note, now deprioritized: Henrik earlier floated_ "We have discussed 2 bank update model, for
that 64K FRAM = 32K ceiling" _— that halving only applied if A/B were adopted, which it isn't for now.)_
Usable budget per part: **FR2433 = 15 KB**, **FR2155 = 32 KB** (the dual-I²C baseline), FR2476 = 32 KB
only via the toolchain lower-window if ever used.

---

## Permutation strategy (RESOLVED 2026-09-21 — derives from the 3rd canonical statement)

Goal from the canonical block: **the least number of firmware permutations in releases**, models not
yet known, multiple chips per product, different models per product, **runtime-detect where possible**,
**worst case the SoM selects the right image from a bundle**. Derived architecture:

1. **Widest runtime coverage per image.** Each built image runtime-detects the model (TLV device
   descriptor / SYS device-ID) and applies a pin-map table, covering **every model in its
   register-/memory-map-compatible family** with ONE binary. This minimizes images within a family.
2. **One image per incompatible family, not per model.** A single linked ELF can't span differing
   FRAM origins / peripheral bases (FR2433 `0xC400` + its PAC vs FR2476 `0x8000` + `e_usci_*` PAC), so
   the permutation count = **number of incompatible families in the product BOM**, not number of
   models. Same source; family chosen at build time via Cargo `--features` (+ PAC selection).
3. **SoM bundle-selection is the fallback, not the primary mechanism.** The release ships the small
   set of family images as a bundle; the UCM SoM, which knows the board it's flashing, picks the
   right one. Runtime detect handles within-family variation so the bundle stays tiny.

Net: permutations = **#incompatible-families** (aim: 1–2), each covering its whole family at runtime.

### Consequences for crate structure — IMPLEMENTED (2026-09-21)
- A `model` module: `detect() -> Model` from the device descriptor; a `PinMap` the rest of the code
  consumes. Model table is **runtime data within a family**, not `#[cfg]` per model. ✅ `src/model.rs`
  (+ `matches_build_family()` catches a wrong-family flash).
- The **family/PAC** is the only compile-time axis (a Cargo feature), kept to as few as the BOM forces.
  ✅ **live:** `fr247x` (default, FR2476/FR2475 dual-I²C) and `fr24xx` (FR2433 single-I²C) both build —
  the feature selects the optional PAC dep, the `#[cfg]` peripheral access in `main.rs`, the
  `memory-<family>.x` (via `build.rs`), and the gate budget (`prod.just`). Both verified gating (fr247x
  110 B/32 KB, fr24xx 110 B/15 KB). FR2155 joins as a future `fr215x` family. **FR2433 is NOT dropped —
  it is a first-class family; fr247x is only the default (the on-hand dev boards).**
- `#[cfg(feature)]` blocks (sensor-master, thresholds, extended regs, OTA) trim size per the 2nd
  canonical statement — orthogonal to the family axis (esp. to fit the smaller `fr24xx` budget).

**Still needed to implement (not blocking the seam):** the candidate MSP430 model list per product/
destination, to fill the pin-map tables. Until then, build the `model`/`PinMap` seam generically.
