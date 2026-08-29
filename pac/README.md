# MSP430 Peripheral Access Crates (PACs)

Type-safe register access for our MSP430 parts, generated with `svd2rust` and **vendored**
here. The board crate and firmware depend on these by path; nothing pulls a PAC from
crates.io (the published `msp430fr2476` is yanked, and generation is cheap + reproducible).

## Why vendored + generated (not a crates.io dep)

- The only published FR2476 PAC is **yanked/unmaintained**; there is no FR2433 PAC at all.
- Vendoring makes builds reproducible and offline, and lets us patch the SVD when TI's
  device files have gaps (see `overrides/` in `msp430_svd`).
- The generated code is deterministic from `(SVD, svd2rust version)`, so it's safe to
  regenerate and diff.

## Crates

| Crate | Part | Role |
|---|---|---|
| `pac/msp430fr2433` | MSP430FR2433 | production default node (see [MCU_SELECTION](../../bob-929/docs/MCU_SELECTION.md)) |
| `pac/msp430fr2476` | MSP430FR2476 | dev boards on hand + production battery-monitor variant |

Each is a normal `#![no_std]` crate depending on `msp430` / `msp430-rt` 0.4 (matching
[`examples/hello-rust`](../examples/hello-rust/Cargo.toml) and the pinned `nightly-2025-06-25`).

## Regeneration

Generation is scripted and pinned in [`../scripts/gen-pac.sh`](../scripts/gen-pac.sh), run via:

```sh
just pac gen                 # regenerate all vendored PACs
just pac gen msp430fr2433    # one device
```

Pipeline (pinned versions recorded in the script):

1. `msp430_svd` converts TI's bundled device files → `<device>.svd` (+ `.patched` if an
   override exists).
2. `svd2rust --target msp430` → register API.
3. `form` splits the generated `lib.rs` into modules; `rustfmt` formats.
4. Output is copied into `pac/<device>/` and committed.

**Do not hand-edit the generated `src/`** — change the SVD/overrides upstream or the
generator, then regenerate. The only hand-maintained files per crate are `Cargo.toml`
and this pipeline.

## Consuming a PAC

The board crate selects the PAC by cargo feature (chip = compile-time), so one firmware
source builds per-chip binaries:

```toml
# board/Cargo.toml (sketch)
[dependencies]
msp430fr2433 = { path = "../pac/msp430fr2433", optional = true }
msp430fr2476 = { path = "../pac/msp430fr2476", optional = true }

[features]
fr2433 = ["dep:msp430fr2433"]   # default node
fr2476 = ["dep:msp430fr2476"]   # battery-monitor variant
```
