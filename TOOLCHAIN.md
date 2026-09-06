# msp-fw toolchain

One **Docker-based toolchain** for all firmware — C (`msp430-gcc`) and Rust
(`msp430-none-elf`) in a single image. PlatformIO is intentionally not used.
Local flashing on macOS is handled by the [`msp430-macos-dev` skill](../bob-929/.claude/skills/msp430-macos-dev/SKILL.md).

## Layout

```
docker/Dockerfile              combined image: cdelledonne/msp430-gcc + pinned Rust nightly + rust-src + just
justfile                       command runner — build (in container) + flash/probe (on host)
pac/msp430fr2433/              vendored PAC (svd2rust) — production default node   [pac/README.md]
pac/msp430fr2476/              vendored PAC (svd2rust) — dev boards + battery-monitor variant
diag/                          diagnostic ROM (Rust, no_std) on the FR2476 PAC    [diag/README.md]
examples/hello-c/              minimal FR2476 blink (C, make)         — toolchain smoke test
examples/hello-rust/           minimal FR2476 no_std app (Rust)       — toolchain smoke test
scripts/                       real logic scripts (gen-pac.sh, release stamping, size diffing)
.github/workflows/
  toolchain-image.yml          builds + pushes the image to GHCR
  firmware.yml                 build-only CI: runs `just build`, uploads artifacts
```

## Commands (just)

```bash
just              # list recipes
just build        # build both examples (runs inside the container)
just probe        # read the connected device id (no flash)

# subcommand groups (just modules):
just check deps            # verify the toolchain is present (check-deps.sh)
just check deps -- --flash # pass flags through to check-deps.sh
just example list          # list examples
just example build hello-c # build one example (auto-detects C make vs Rust cargo)
just example run   hello-c # build + flash one example
```

`build` runs `docker run … just build` on a host and natively in CI (it detects
`/.dockerenv`), so CI runs the exact same recipe. Flash/`probe` recipes run **natively on the
host** (Docker on macOS has no USB passthrough) and need the msp430-macos-dev skill's one-time
setup (x86_64 mspdebug + signed `libmsp430.dylib`).

First-time setup — build the toolchain image once:

```bash
just bootstrap        # docker build -t msp430-c-rust:local docker/
```

That local tag is the default `MSP430_IMAGE`. Once `toolchain-image.yml` pushes to GHCR you can
skip `bootstrap` and set `MSP430_IMAGE=ghcr.io/experientials/msp430-toolchain:latest` instead.

> The image is **amd64** (TI's Linux `msp430-gcc` is x86_64-only), so on Apple Silicon it
> runs under Rosetta/QEMU emulation — fine for building, just not native speed. GitHub's
> runners are x86_64, so CI is native.

## CI

- **toolchain-image.yml** builds `docker/Dockerfile` and pushes to GHCR. Run it once
  (`workflow_dispatch`) before the first `firmware.yml` run, since firmware pulls that image.
- **firmware.yml** compiles both examples in the image and uploads the `.elf`s. Build-only —
  no hardware in CI (flashing is local via the skill).

## Native macOS build (optional, segregated)

The default build runs the Linux toolchain in Docker — on Apple Silicon that's **amd64 under
emulation**, which is slow (a one-crate rebuild can take minutes, mostly the LTO link). For a fast
local loop you can build **natively** instead. It's an opt-in alternative; Docker stays canonical
for release/CI.

- **One-time:** `bash scripts/setup-native-macos.sh` — installs the pinned nightly + `rust-src` into
  a **dedicated toolchain home** (`~/.local/share/msp430-rust`, override with `MSP430_RUST_HOME`) and
  a native `msp430-elf-gcc` (Homebrew, tgtakaoka tap). **Segregated by design:** it never writes
  `~/.rustup`, never runs `rustup default`, and never shadows your system `rustc` — the nightly is
  reached only through the recipes below.
- **Build:** `just diag build-native` (add `fast` for the no-LTO profile → `just diag build-native fast`).
- **Verify segregation:** `just diag doctor` — asserts the pinned nightly is reachable *and* is not
  your system default.
- **Flash:** unchanged (`just diag flash`) — still the host-side Rosetta `mspdebug` from the
  `msp430-macos-dev` skill (TI ships `libmsp430.dylib` x86-only, so Rosetta is mandatory there).

Reproducibility note: native `msp430-elf-gcc` (Homebrew tap, **9.3.x**) differs from the Docker
image's msp430-gcc **8.3**, so native and container `.elf`s won't be byte-identical. Treat the
**Docker/CI build as canonical** for released artifacts; native is for iteration. `just diag
build-fast` remains the in-Docker no-LTO option when you want container parity without the LTO cost.

## Notes on the toolchains

- **C:** `msp430-elf-gcc` (msp430-gcc 8.3 in the base image) with TI device support for
  `msp430fr2476` (`-mmcu=msp430fr2476`, headers + `.ld` at `$MSP430_TOOLCHAIN_PATH/include`).
- **Rust:** Tier-3 `msp430-none-elf`, pinned `nightly-2025-06-25` + `rust-src`, `build-std=["core"]`,
  linked through `msp430-elf-gcc`. Based on `rust-embedded/msp430-quickstart`.
  - **PACs are vendored** under `pac/` (svd2rust, generated + regenerable via `just pac gen`;
    see [pac/README.md](pac/README.md) and the skill's `references/rust-pac.md`). `diag/`
    consumes `msp430fr2476` for typed register access; `examples/hello-rust` predates them and
    still pokes raw SFRs.
  - `memory.x` gives `VECTORS` the legacy 16-word layout (`0xFFE0–0xFFFF`) with the reset
    vector at `0xFFFE` — fine while no ISRs are used. For interrupts, enable the PAC's `rt`
    feature (emits the full vector table via `device.x`) instead of the hand-rolled layout.
  - **No HAL yet** — firmware uses the PAC directly. A board/BSP crate (typed pins on the PAC,
    chip selected by cargo feature) is the planned next layer; there is no off-the-shelf HAL
    for the FR24xx family (`msp430fr2x5x-hal` is FR2355-only).
  - Keep `rust-toolchain.toml`'s nightly in sync with the Dockerfile's `RUST_TOOLCHAIN`.
