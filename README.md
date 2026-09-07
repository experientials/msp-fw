# msp-fw

MSP430 firmware for the Thepia **bob-929 / ziloo** hardware — a low-power **supervisor + I/O
extender** that monitors rails/signals while the main board sleeps and exposes its GPIO to a host
over I²C.

The current focus is [`diag/`](diag/) — a Rust power-on self-test (POST) that scans the I²C sensor
bus and exercises each known device. Production part is the **FR2433**; the **FR2476** is the dev
board and the battery/rail-monitoring variant.

> New here? Read [TOOLCHAIN.md](TOOLCHAIN.md) (tooling) and [diag/DESIGN.md](diag/DESIGN.md) (the
> firmware model). Pin assignments live in [crates/bsp/connections.toml](crates/bsp/connections.toml).

## Prerequisites

- **Docker** — builds run inside a pinned Linux toolchain image (msp430-gcc + Rust nightly). On
  Apple Silicon that image is amd64 under emulation.
- **`just`** — the command runner.
- **Flashing (macOS host):** a one-time `mspdebug` setup (x86_64 mspdebug + signed
  `libmsp430.dylib`) from the **msp430-macos-dev** skill. USB never goes through Docker.

## Build

```sh
just bootstrap        # once: build the toolchain image locally (only needed for the Docker path)
just diag build       # build diag/ — native if set up, else the Docker image
just check deps       # verify the toolchain
```

`just diag build` auto-selects the fastest path: the **native** toolchain if it's installed (no
emulation), otherwise the **Docker** image; inside CI it builds in-container. Override with
`just diag build docker|native|fast|docker-fast`. This only sets the LOCAL default — shipped
artifacts are always built on CI.

**Faster local loop (macOS/Linux):** one-time `bash scripts/setup-native-macos.sh` installs the
native toolchain (segregated — never your system `rustc`; check with `just diag doctor`). After that
`just diag build` is native, and **`just diag dev`** is the sub-minute build+flash inner loop.
Detail in [TOOLCHAIN.md](TOOLCHAIN.md#native-macos-build-optional-segregated).

## Flash

```sh
just diag run         # build + flash + VERIFY the running firmware reports the stamp  ← everyday command
just diag flash       # flash the last build (no rebuild); 'just diag flash fast' for the fast profile
just monitor          # watch the 9600 8N1 backchannel UART (auto-detects the port)
just usb status       # diagnose the eZ-FET USB if it drops off (hub/latch/short)
```

`just diag run` closes the "did my flash actually take?" gap — it stamps the build, flashes, then
reads the UART and refuses to succeed until it sees that exact stamp come back.

## Versioning

**There is no release system yet — every build is a dev build.** Firmware identity is a stamp
(`DIAG_BUILD`) baked in by [diag/build.rs](diag/build.rs) and printed on every POST banner, so the
board announces exactly what it's running:

```
[<target>/]<year>.<release>-<short-hash>[-dirty][.<secs>]
```

- **Today** (single FR2476 dev build): `2026.dev-59ad941-dirty.1725540000`.
- **`<target>`** is the real axis of variation — the chip/config variant (FR2433 vs FR2476,
  role/placement), kept to a few targets and FRAM-size-constrained, with the firmware autodetecting
  finer placement at boot. Set via `DIAG_TARGET` once the board crate builds per chip.
- **`<release>`** is `dev` until a release process sets a number (`DIAG_RELEASE=1` → `2026.1-59ad941`).
- Not semver — the crate `version` in Cargo.toml is unused by the firmware.

CI ([.github/workflows/hil.yml](.github/workflows/hil.yml)) builds the `diag` ELF in the toolchain
image (the canonical build; native is iteration-only), then a self-hosted runner with a board runs
`just diag hil`: it flashes and asserts over UART that the POST reports the stamp, all expected
devices are present, and nothing is FAULTY. Reproduce locally:

```sh
just diag run                                     # build + flash + confirm the stamp
EXPECT_STAMP=$(git rev-parse HEAD) just diag hil  # full HIL assertion against the board
```

The toolchain image is published to GHCR by
[.github/workflows/toolchain-image.yml](.github/workflows/toolchain-image.yml); set
`MSP430_IMAGE=ghcr.io/experientials/msp430-toolchain:latest` to skip `just bootstrap`.

## Layout

| Path | What |
|---|---|
| [`diag/`](diag/) | Rust POST / diagnostic firmware (current focus) |
| [`pac/`](pac/) | Vendored svd2rust PACs (`msp430fr2433`, `msp430fr2476`) |
| [`crates/bsp/connections.toml`](crates/bsp/connections.toml) | Single source of truth for pins/signals |
| [`examples/`](examples/) | `hello-c`, `hello-rust` — toolchain smoke tests |
| [`justfile`](justfile) + `*.just` | Command runner (`diag`, `pac`, `usb`, `example`, `check`) |
| [`docker/Dockerfile`](docker/Dockerfile) | The one amd64 toolchain image |
| [`scripts/`](scripts/) | Build/setup/PAC logic (recipes stay thin) |
