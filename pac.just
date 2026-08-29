# PAC generation + compile-test.  `just pac gen` | `just pac check <device>`
# See pac/README.md and the msp430-macos-dev skill's references/rust-pac.md.

image := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")

# regenerate vendored PAC crates (default: both devices) from TI device files
gen *devices:
    scripts/gen-pac.sh {{devices}}

# compile-test a vendored PAC in the toolchain container (nightly + build-std)
check device="msp430fr2433":
    docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work/pac/{{device}} {{image}} \
      cargo build --release --target msp430-none-elf -Z build-std=core
