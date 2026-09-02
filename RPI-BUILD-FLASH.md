# Raspberry Pi as an MSP430 build + flash node — research & plan

Goal: a Raspberry Pi that can **build and flash** msp-fw — over USB to the LaunchPad (eZ-FET) now,
and over the target's **SBW debug pins** for production later — driven remotely from VS Code.

Companion to [TOOLCHAIN.md](TOOLCHAIN.md) (the build/flash model) and the
[msp430-macos-dev skill](https://github.com/experientials/bob-929/tree/main/.claude/skills/msp430-macos-dev)
(the host flashing setup this mirrors on Linux). This is a **plan**, not yet built.

## The one constraint that shapes everything: architecture

Two pieces of the pipeline are **x86_64-only** as shipped by TI, and a Pi is **aarch64**:

| Piece | Role | TI ships | On aarch64 Pi |
|---|---|---|---|
| `msp430-gcc` / `msp430-elf-gcc` | **build** (compile + link) | x86_64 Linux only (per [TOOLCHAIN.md](TOOLCHAIN.md)) | emulate, or build from source |
| `libmsp430.so` (MSPDebugStack / `tilib`) | **flash** (eZ-FET/MSP-FET over USB) | x86_64 Linux only | build from source (SLAC460) |

The winning move is the one the repo's CI **already makes**: **decouple build from flash.**
`firmware.yml`/`hil.yml` build in an x86_64 cloud container and flash on a separate self-hosted
runner. The Pi can be the flash/runner end and never needs the build toolchain at all — or it can
build too, with extra setup. Pick per role below.

## Three roles the Pi can play (independent — do any subset)

1. **Remote dev host** — you SSH in from VS Code, edit/build/flash on the Pi as if local.
2. **Self-hosted HIL runner** — the Pi replaces the Mac bench runner in [hil.yml](.github/workflows/hil.yml)
   (runner label `msp430`): cloud builds, Pi flashes + asserts the POST over UART.
3. **Production programmer** — Pi drives **SBW** straight into the target's TEST/RST pins (no FET).

## Building on the Pi — three options, cheapest first

- **A. Don't. Build in the cloud/CI, Pi only flashes.** Matches `hil.yml` exactly; zero toolchain on
  the Pi. Best for the HIL-runner and production-programmer roles. **Recommended default.**
- **B. Emulated amd64 Docker on the Pi.** Install `qemu-user-static` + binfmt, run the existing
  `ghcr.io/.../msp430-toolchain` image under emulation. Same `just build` recipe, just slow
  (minutes). Good enough for occasional local iteration; zero new toolchain work.
- **C. Native aarch64 toolchain.** Fast local builds, most effort:
  - `msp430-elf-gcc`: build from source for aarch64 (it's GCC — portable; TI's *binaries* aren't
    ARM but the source is). Or try Debian's `gcc-msp430` (older mspgcc) as the linker — **verify it
    satisfies the Rust `msp430-none-elf` target's linker/intrinsic expectations before trusting it.**
  - Rust: `nightly-2025-06-25` has an aarch64-linux host; the `msp430-none-elf` target is
    host-agnostic. `rustup` + our pinned nightly builds `core` natively.
  - Payoff: near-instant edit/build/flash on the Pi. Do this only once local build speed matters.

**Recommendation:** start with **A or B**; graduate to **C** only if you iterate builds *on the Pi*
often enough to care.

## Flashing over USB (eZ-FET) — works today, needs an ARM `libmsp430`

The repo flashes with `mspdebug tilib "prog <elf>" exit` (see [diag.just](diag.just)). `tilib` needs
`libmsp430`. On the Pi:

1. **Build `libmsp430.so` for aarch64** from TI's **MSPDebugStack source (SLAC460)** — the Linux/ARM
   path the macOS skill's signed `libmsp430.dylib` is the Mac analog of. (Community RPi builds of
   this exist; it compiles with `make` + HID/libusb deps.)
2. **Install `mspdebug`** (`apt install mspdebug`, or build for the newest `tilib` support).
3. **udev rules** so a non-root user reaches the eZ-FET: match the TI FET USB VID (confirm with
   `lsusb`; TI tools are typically `2047:xxxx`), grant the `plugdev`/`dialout` group, no sudo.
4. **Backchannel UART** enumerates as **`/dev/ttyACM*`** (not macOS `cu.usbmodem*`) — the higher
   interface is the console, same as on the Mac.

### Repo changes this implies (recipes are macOS-flavored today)

The `just` recipes assume macOS; a Linux/Pi branch is needed:

- `diag.just`/`hil`: `DYLD_FALLBACK_LIBRARY_PATH` → **`LD_LIBRARY_PATH`**; `mspdebug` path via the
  existing `MSPDEBUG_BIN` env (already overridable) → point at the Linux binary.
- `justfile` `monitor`: `/dev/cu.usbmodem*` glob → **`/dev/ttyACM*`** on Linux. Gate on `uname`.
- `just usb` recovery (hub/latch) is macOS `ioreg`-based → Linux `lsusb`/`usbreset` equivalent.
- Add a Linux branch to `just check deps`.

None are hard; they're `if [ "$(uname)" = Linux ]` splits. This is the bulk of the "make it run on
the Pi" work and is worth doing regardless (it also unblocks a Linux CI HIL runner).

## Production programming: BSL-over-UART (the chosen path)

**Decision:** production flashing of bare boards goes through the MSP430 ROM **bootloader (BSL)**
over **UART**, not a FET and not SBW bit-bang. It's arch-independent (pure Python on the Pi, so the
x86-only `libmsp430` problem never applies), needs no debug probe, and is scriptable for a jig.
JTAG/SBW stays available for *bring-up debug*, but is not the production programmer.

### What the FR2476 ROM BSL gives us (datasheet Table 9-4, slau550)

| Signal | BSL function | Note |
|---|---|---|
| **P1.4** | **Data transmit** (MCU → host) | **same pin as our UCA0 TXD console** |
| **P1.5** | **Data receive** (host → MCU) | **same pin as our UCA0 RXD console** |
| RST/NMI/SBWTDIO | entry-sequence signal | driven by host DTR (or a Pi GPIO) |
| TEST/SBWTCK | entry-sequence signal | driven by host RTS (or a Pi GPIO) |
| VCC / VSS | power / ground | 3.3 V, Pi-compatible |

Two facts make this clean:
- **The BSL UART is P1.4/P1.5 — the exact pins as the diag console UART.** The programming link and
  the debug console are the same two wires; on the LaunchPad they're already on the eZ-FET
  backchannel.
- **A blank FR24xx auto-invokes the BSL** (empty reset vector → BSL at power-up). So a **fresh
  production chip needs no entry sequence at all** — power it up and it's listening on P1.4/P1.5.
  The TEST/RST entry sequence is only needed to *re-enter* BSL on an already-programmed board.

### UART parameters (get these exact or it won't sync)

- **9600 baud to start**, **8 data bits, EVEN parity, 1 stop bit (8-E-1)** — note: *even parity*,
  **not** the 8-N-1 our console runs. Baud can be negotiated up after init on FR24xx.
- Password-protected memory access; a **mass-erase** resets the password (the standard production
  first-write flow: mass-erase → program → verify).

### The Pi-side jig

- **Data:** the Pi's own UART (`/dev/ttyAMA0`/`ttyS0`) **or** a USB-serial adapter → target P1.5
  (MCU RX) / P1.4 (MCU TX), GND, and 3.3 V to power the target from the Pi.
- **Entry sequence (only for non-blank boards):** either a USB-serial adapter whose **DTR→RST** and
  **RTS→TEST** (what TI's tools drive), or **two Pi GPIOs** bit-banging the sequence. For pure
  first-time production of blank chips, skip it — rely on blank-device auto-BSL.
- **Tooling on the Pi (aarch64-native Python):** the FRAM/5xx-style UART BSL protocol — via
  `python-msp430-tools` (BSL5 UART) or an equivalent open BSL host. TI's *BSL Scripter* is x86-only,
  so prefer the Python host so it runs natively on the Pi. Wrap it in a `just` recipe
  (`just prod flash <elf>`), TI-TXT/hex output from the build.

### Why not SBW bit-bang (kept as a note, not the plan)

`mspdebug rpi` can bit-bang Spy-Bi-Wire over Pi GPIO (no FET, 3.3 V both sides). Rejected as the
*production* path because: the classic `rpi` driver poke GPIO via `/dev/mem` at **old-SoC base
addresses** (Pi 1–3), so **Pi 4/5 needs a patched/libgpiod mspdebug or a Pi 3**; and it still leans
on the x86-only `libmsp430` for the higher-level ops. It remains useful for **full JTAG debug** of a
bare board when BSL's program-only nature isn't enough — a bring-up tool, not the line programmer.

## VS Code Remote — how you'd actually drive it

- **Remote-SSH (recommended).** VS Code on your laptop connects over SSH; the workspace lives on the
  Pi; the integrated terminal runs `just` against the Pi's real USB/GPIO; **rust-analyzer runs on the
  Pi natively (aarch64)** — full IntelliSense with no local toolchain. Flashing is just a terminal
  command hitting local hardware. This `claude-code` CLI also runs great in that SSH terminal.
- **Remote Tunnels (`code tunnel`).** For a **headless Pi behind NAT** with no inbound SSH: run
  `code tunnel` on the Pi, authenticate via GitHub, connect from `vscode.dev` or desktop. Good for a
  Pi that lives on the bench and you reach from anywhere.
- **Dev Containers.** Attach VS Code to the **emulated amd64 toolchain container** on the Pi for
  exact CI build parity — but rust-analyzer then runs emulated (slower). Prefer Remote-SSH + native
  rust-analyzer, and keep *builds* via `just` (cloud or emulated).

**Extensions:** Remote-SSH (or Remote-Tunnels), rust-analyzer, a serial-monitor extension (or just
`just monitor`). Nothing about the target build depends on the editor.

## Suggested phasing (priority: the BSL production programmer)

- **Phase 0 — Remote dev.** Flash Raspberry Pi OS (64-bit), enable SSH, VS Code Remote-SSH in, clone
  the repo, install `just` + Rust nightly. Builds via **emulated Docker** (option B) or cloud CI.
  Confirm rust-analyzer. *Proves the remote workflow; no hardware yet.*
- **Phase 1 — BSL-over-UART programmer (the chosen production path).**
  1. Emit a **TI-TXT/hex** artifact from the build (BSL host input), alongside the ELF.
  2. Install an **aarch64-native Python BSL host** (`python-msp430-tools` BSL5 UART or equivalent).
  3. Wire the jig: Pi UART **or** USB-serial → target **P1.5 (RX) / P1.4 (TX)**, GND, 3.3 V; at
     **9600 8-E-1**. For blank chips, rely on **auto-BSL** (no entry sequence). For re-flash, add
     **DTR→RST / RTS→TEST** (USB-serial) or two Pi GPIOs.
  4. Wrap as `just prod flash <elf>`: mass-erase → program → verify.
  *Validate first on the LaunchPad (P1.4/P1.5 are already on the eZ-FET backchannel), then on a bare
  board.*
- **Phase 2 — HIL runner on the Pi.** Register the Pi as the `msp430` self-hosted runner; move the
  bench role off the Mac in [hil.yml](.github/workflows/hil.yml) (its notes assume a Mac). Cloud
  builds, Pi flashes (via BSL) + asserts the POST over UART.
- **Phase 3 (optional) — USB FET + SBW debug.** If interactive **debug** of a bare board is needed
  beyond BSL's program-only scope: build aarch64 `libmsp430.so` (MSPDebugStack/SLAC460) for
  `mspdebug tilib` over an eZ-FET/MSP-FET, or bring up `mspdebug rpi` SBW bit-bang (mind the Pi-4/5
  GPIO caveat). Also add the **Linux branch to the `just` recipes** (`LD_LIBRARY_PATH`,
  `/dev/ttyACM*`) if flashing the LaunchPad from the Pi via USB.

## Decisions locked / still open

- **Production flash = BSL-over-UART.** ✔ (SBW/JTAG demoted to optional debug.)
- **Near-term focus = the production programmer** (Phase 1). ✔
- Still open: **which Pi model** (any 64-bit Pi works for BSL — no SBW bit-bang dependency, so Pi
  4/5 is fine); **Pi UART vs USB-serial** for the BSL link (USB-serial gives DTR/RTS entry lines for
  free); whether to bother with a **native aarch64 build toolchain** (option C) or stay on
  cloud/emulated builds; and confirming the exact **Python BSL host** that speaks the FR24xx UART
  BSL protocol on aarch64.
