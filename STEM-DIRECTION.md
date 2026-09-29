# Stem — architecture & direction

> **Captured verbatim from Henrik (2026-09-25).** This is the intended **leading description for a
> future version of the README**, plus the fuller Stem architecture spec. Forward-looking: the roles
> are being filled **slowly / incrementally**. This file is canonical for the Stem direction — when
> the README lead is rewritten, draw from the verbatim blocks below (polish grammar, keep the intent).
> Ties to project memory `stem-nervous-system-vision`, `prod-fw-multimodel-dualbus`,
> `msp-fw-ota-host-root-of-trust`.

## Proposed leading README description (verbatim)

The stem is composed of mcus that maintains the system cohesion even when inactive. They take on
specific roles that make up the overall functionality. A SoM/SoC is the default master on the stem
bus and the work horse for the active system.

Defined roles for mcus are:
- **Sleep supervisor**
- **Power supply**
- **Inner vision** (nfc and super low res ir ml vision)
- **Radio receiver** (ble/thread)

The msp-fw will become the **stem repo** with firmware for all stem mcus. We need proper **HAL** to
manage the exact mcu models.

## Role → MCU (current fills)

| Role | Production chip | Core | Dev board |
|---|---|---|---|
| **Sleep supervisor** (a.k.a. **Detector**) | MSP430 (FR2355 canonical; FR2433 baseline) | MSP430 | FR2476 / FR2355 LaunchPad |
| **Power supply** | **CYPM1111** | Cortex-**M0** | **CY7111** |
| **Inner vision** (NFC + super-low-res IR ML vision) | **RP2350** | — | — |
| **Radio receiver** (BLE / Thread) | **nRF52 family** | — | nRF52 dongle |

*Sleep supervisor ≡ **Detector** (Henrik, 2026-09-25): it uses sensors to determine what/how the device
needs to respond. "Detector" names the function (sense → decide response); "sleep supervisor" names the
when (active while the system sleeps).*

Each role has a **production chip** and (where applicable) a **dev board** used to develop it — the
same split as MSP430's FR2476 LaunchPad (dev) → FR2433/FR2355 (production). For the power role that's
**CY7111 (dev board) → CYPM1111 (the actual chip we would use)**.

Not starting the non-MSP430 parts yet — these are the current assignments. "Proper HAL to manage the
exact mcu models" is what lets one Stem repo hold firmware per role across these different cores/
vendors; it is a **later, incremental** step (extract it as the MCUs land, not ahead of them — the
near-term MSP430 step is FR2355 via a feature-gated PAC alias, see
[diag/FR2355-SCOPE.md](diag/FR2355-SCOPE.md)).

### MSP430 roles — FR2355 sleep-supervisor, FR2433 general I/O expander

FR2355 fills the **sleep-supervisor** role. A second MSP430 role — **"some sort of general I/O expander
on a daughterboard"** (Henrik, 2026-09-25, **not yet clearly defined**) — is an **even better fit for
the FR2433**: single-I²C, slave-only, already the **Stembus Expander** (a pure I/O-extender the SoM
drives), whereas FR2355's **2× I²C** (sensor master + SoM slave) is what suits the *autonomous*
sleep-supervisor. These map onto the prod firmware's existing behaviours — **Sensing** ≈ autonomous
supervisor (FR2355) and **Passive** ≈ pure I/O-extender/slave (FR2433 expander); see
[prod/DESIGN.md](prod/DESIGN.md). The daughterboard I/O-expander would use the semantic (topic,
topic-pin) model above. **To define:** what the daughterboard I/O-expander role adds beyond the
existing FR2433 Stembus Expander.

## Field-updatable firmware & integrity — FUTURE MILESTONE (verbatim)

> **Deferred.** Henrik (2026-09-25): *"I will return to this topic in a future milestone."* Not now —
> captured so it's not lost.

The firmware build includes **checksums** and may be **aligned to row boundaries** to allow partial
updates. When booting the firmware calculates the checksums and compare with expected. If it fails
the loading is **restricted to known working parts**.

Goal: **Stem firmware field-updatable with check-summed incremental updates.** The **CYPM1111 has row
logic** (row-based flash) that we would use for the **incremental field updates** — this is why images
align to **row boundaries** (a row is the update granule). This applies across the Stem MCUs, not just
CYPM1111.

**Decision (Henrik, 2026-09-25): A/B is NOT the approach** — common, but concluded against it. *"We
will make a custom scheme if need be, but attempt to ground any logic in what has been done before."*
So: a **custom scheme is acceptable** where warranted; ground it in **proven OSS solutions and
principles** as much as possible (checksum/signature verification, verify-then-boot, known-good
fallback) **without adopting A/B**. Note the model here is **row-granular incremental** updates (patch
rows, via the CYPM1111 row logic) — inherently different from A/B's whole-image swap.

## The full stembus (verbatim)

The full stembus for the som to communicate with mcus, is **i2c, uart, reset, bootsel, wakeup,
sleepy**.

It allows the som to connect to a specific mcu and **program or monitor it over uart or swd**. Some
lines are guarded by a **directional level shifter that can be switched on/off via the i2c**.

## Stem & Leaf I²C buses (verbatim · Henrik 2026-09-29)

There are **two I²C buses**, named by role, not by silicon instance:

- **Stem I²C** — the SoM↔MCU interconnect (part of the stembus above). The MCU is a **slave/node**
  on it; the SoM is master.
- **Leaf I²C** — the bus carrying **leaf nodes**. Henrik: *"Leaf is a better name for the bus with
  sensors as it can also have LCD and other non-MCU nodes."* So it is **not** just "sensor I²C":
  sensors, LCDs, and any other **non-MCU** peripheral hang off it. The MCU is **master** here.

**The allocation of Stem vs Leaf to a physical eUSCI instance + pins is chip/PAC-specific.** So code
must not reference a raw peripheral like `e_usci_b0`; the **semantic bus (Stem/Leaf) is defined per
chip in the `board` layer** and exposed by role (e.g. `board::leaf_i2c()` / `board::stem_i2c()`),
exactly like the backchannel UART instance. The generated svd2rust PAC stays pure silicon
(`e_usci_b0`/`b1`); the *role→instance* mapping lives in `board`. On a 2-eUSCI-B part (e.g. FR2355:
UCB0 + UCB1) one becomes Leaf-master, one Stem-slave; on a slave-only part (FR2433) there is only the
Stem side. See [[per-sensor-module-convention]], [[prod-fw-multimodel-dualbus]].

### Granularity of `board` (decided · Henrik 2026-09-29)

**`board` = the module / PCB** — not the MCU (below it) and not the assembled product (above it).
Rationale, verbatim: *"We generally don't have more than one MCU per module. If we were to have two,
it would most likely be very different ones."* So a module carries **one MCU**, and `board` = that
module. Vocabulary: *"Modules are boards of sorts, or built around one. A sub-module can also be
thought of as a daughter board."* — module ≈ board; a **sub-module = a daughterboard**; all one layer.

Three layers: **chip/MCU** (silicon: PAC, clock trim, timer, RAM, eUSCI existence + pin options) →
**board/module** (which pins are wired; the semantic peripheral map: `leaf_i2c`/`stem_i2c`/
`console_uart`/`spi`) → **product/assembly** (which modules + MCU roles compose the device — the stem
topology). Because a module is ~1:1 with its MCU, the chip and board facts are **fused in one
`board::<chip>_<module>` file** (e.g. `fr2355_launchpad`) rather than split into separate `chip`/
`board` layers — the split is only forced if one MCU ever lands on ≥2 modules. The cargo feature names
the **chip** (the PAC axis); the module is named in the file. See [[stem-nervous-system-vision]].

**This generalizes to the whole node (Henrik 2026-09-29): `board` is the semantic peripheral map —
2× I²C, UART, SPI are all semantic.** Every bus is named by role and bound per-chip to its instance +
pins + mode: Stem-I²C (slave), Leaf-I²C (master), the backchannel/Stem UART, and SPI (e.g. an LCD or
non-MCU node). On MSP430 the allocation is constrained by silicon: **eUSCI_A = UART or SPI**,
**eUSCI_B = SPI or I²C** (SLAU445/SLAU144), and A/B counts + pinouts differ per part — which is exactly
why the role→instance map must be per-chip in `board`, and the layers above (`hal`/drivers) stay
role-agnostic.

**Every MCU node carries the same triplet (Henrik 2026-09-29):** `stem_i2c` (uplink to the SoM — the
MCU as **slave**), `leaf_i2c` (its **own** sensor/LCD/non-MCU subtree — the MCU as **master**), and
`console_uart` (debug/programming link). `stem_i2c` is needed **in addition to** `leaf_i2c`, not
instead of it.

**Multi-MCU boards need the node role as a disambiguator.** If a module has >1 MCU, `console_uart`
and `stem_i2c` exist **once per MCU** (both connect to the SoM), so the bus role alone is not unique —
you must qualify by **which MCU**, i.e. its **system role** (Detector, Expander, …). So the full
identity is `<node-role>::<bus-role>` (e.g. `detector.stem_i2c`, `expander.console_uart`), and the
**`board` granularity refines to (chip, module, node-role)** — the role is implicit on a single-MCU
module, explicit when >1. This is the same node identity the stembus uses (source chip / uid, above).
Physical consequence: **Stem-I²C is one shared bus with a distinct slave address per node**;
**console_uart is a separate link per node**; **leaf_i2c is a separate master subtree per node** (never
two masters on one bus). From a node's own firmware the accessors (`board::stem_i2c()` etc.) always
mean *this* node — the role qualifier lives in the system/topology layer and the SoM's addressing.

## Expanders & semantic pin identity (verbatim)

Expanders keep a **current state** on connected pins. An expander also keeps **topical identity** of
each connected pin. This allows a cluster of expanders to provide **semantic control of pins
regardless of how they connect**. Using stembus it becomes easier to iterate through hardware designs,
changing the location and connection of chips on the way.

Connected pins are identified by multiple IDs:
- **uid** — global known pin ID
- **Source chip**
- **Source chip pin**
- **Topic**
- **Topic pin**

### Topics
- T-USB 3.0 alt modes
- T-USB 2.0 alt modes
- M.2 Key B SSD
- M.2 Key E WiFi
- SoM NPU wakeful module
- Left Cam Module
- Right Cam Module
- PD Controller
- Battery charger

For each pin on the expander it defines the **topic no** and **topic pin index** that it connects to.
This allows other MCUs to **request state change without knowing the pin number** — the semantic
(topic, topic-pin) addressing rather than a physical pin number. This is the direction the pin
registry ([crates/bsp/connections.toml](crates/bsp/connections.toml)) evolves toward.
