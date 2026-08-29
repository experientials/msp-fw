# MSP430 C + Rust toolchain: reuse cdelledonne/msp430-gcc (msp430-gcc 8.3, FR2476 device
# files, mspdebug-capable) and layer a pinned Rust nightly + rust-src for msp430-none-elf.
FROM cdelledonne/msp430-gcc:latest

ARG RUST_TOOLCHAIN=nightly-2025-06-25
USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates gcc && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/opt/ti/msp430-gcc/bin:/usr/local/bin:/usr/bin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain ${RUST_TOOLCHAIN} --profile minimal --component rust-src \
 && rustc --version && cargo --version && msp430-elf-gcc --version | head -1

# just — command runner (prebuilt binary). CI and local dev run the same recipes.
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
      | bash -s -- --to /usr/local/bin \
 && just --version
