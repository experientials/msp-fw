# MSP430 C + Rust toolchain image.
#
# Base: debian:bookworm (glibc 2.36). The old cdelledonne/msp430-gcc base had glibc < 2.28, which
# GitHub Actions' Node 20/24 runtime can't run — every JS action (checkout, upload-artifact) failed
# inside the container with "GLIBC_2.28 not found". A modern base fixes that at the root.
#
# msp430-elf-gcc: OUR reproducible 9.3.1 build, fetched from the GitHub Release that
# toolchain-msp430-gcc.yml publishes — so CI, the release, and local dev all run the SAME toolchain.
FROM debian:bookworm-slim

ARG RUST_TOOLCHAIN=nightly-2025-06-25          # keep in sync with diag/rust-toolchain.toml
ARG MSP430_GCC_REPO=experientials/msp-fw
ARG MSP430_GCC_TAG=msp430-gcc-9.3.1
ARG MSP430_GCC_ASSET=msp430-elf-gcc-9.3.1-linux-x86_64.tar.xz

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates xz-utils git make gcc \
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
