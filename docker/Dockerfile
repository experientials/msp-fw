# MSP430 C + Rust toolchain image.
#
# Base: ubuntu:24.04 (glibc 2.39). Two glibc constraints must both be met:
#   • GitHub Actions' Node 20/24 runtime needs glibc >= 2.28 (the old cdelledonne base failed this
#     with "GLIBC_2.28 not found", breaking checkout/upload-artifact inside the container);
#   • our msp430-elf-gcc is built on ubuntu-24.04 (toolchain-msp430-gcc.yml), so its binary needs
#     glibc >= 2.38. Matching the base to the build runner (ubuntu:24.04) satisfies both.
#
# msp430-elf-gcc: OUR reproducible 9.3.1 build, fetched from the GitHub Release that
# toolchain-msp430-gcc.yml publishes — so CI, the release, and local dev all run the SAME toolchain.
FROM ubuntu:24.04

ARG RUST_TOOLCHAIN=nightly-2025-06-25          # keep in sync with diag/rust-toolchain.toml
ARG MSP430_GCC_REPO=experientials/msp-fw
ARG MSP430_GCC_TAG=msp430-gcc-9.3.1
ARG MSP430_GCC_ASSET=msp430-elf-gcc-9.3.1-linux-x86_64.tar.xz

# build-essential (not just gcc) — Rust proc-macro/build-script deps (quote/syn) link HOST binaries
# and need the C runtime (Scrt1.o/crti.o/libc), which lives in libc6-dev. `make` builds the C example.
RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates xz-utils git make build-essential \
 && rm -rf /var/lib/apt/lists/*

# Our msp430-elf-gcc 9.3.1 (public repo → release asset needs no auth). Extracts to /opt/msp430-gcc,
# with device headers + linker scripts under /opt/msp430-gcc/include (see build-msp430-gcc.sh).
RUN curl --proto '=https' --tlsv1.2 -fsSL \
      "https://github.com/${MSP430_GCC_REPO}/releases/download/${MSP430_GCC_TAG}/${MSP430_GCC_ASSET}" \
      | tar -C /opt -xJ \
 && /opt/msp430-gcc/bin/msp430-elf-gcc --version | head -1

ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    MSP430_TOOLCHAIN_PATH=/opt/msp430-gcc \
    PATH=/opt/cargo/bin:/opt/msp430-gcc/bin:/usr/local/bin:/usr/bin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain ${RUST_TOOLCHAIN} --profile minimal --component rust-src \
 && rustc --version && cargo --version

# just — command runner (prebuilt binary). CI and local dev run the same recipes.
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
      | bash -s -- --to /usr/local/bin \
 && just --version
