# `just example <verb> <name>` — per-example operations (module).
# Auto-detects the build system: Cargo.toml → Rust, Makefile → C.
#
#   just example list
#   just example build hello-c
#   just example run   hello-c      (build + flash)
#   just example flash hello-rust

image    := env_var_or_default("MSP430_IMAGE", "msp430-c-rust:local")
mspdebug := env_var_or_default("MSPDEBUG_BIN", "/usr/local/bin/mspdebug")
libdir   := env_var_or_default("MSP430_LIBDIR", "/usr/local/lib")

# list available examples
list:
    @find examples -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort

# build <name> (C=make, Rust=cargo). Native inside the image; Docker-wrapped on a host.
build name:
    #!/usr/bin/env bash
    set -euo pipefail
    d="examples/{{name}}"
    [ -d "$d" ] || { echo "no such example: {{name}} (try 'just example list')" >&2; exit 1; }
    if [ -f /.dockerenv ]; then
        # inside the toolchain image (local docker run, or CI) — build natively
        if [ -f "$d/Cargo.toml" ]; then
            ( cd "$d" && cargo build --release && msp430-elf-size target/msp430-none-elf/release/{{name}} )
        elif [ -f "$d/Makefile" ]; then
            make -C "$d"
        else echo "no build system in $d" >&2; exit 1; fi
    else
        # on the host — run this same recipe inside the container
        docker run --rm --platform linux/amd64 -v "$PWD":/work -w /work {{image}} just example build {{name}}
    fi

# flash <name>'s built ELF to the connected board (host; needs msp430-macos-dev setup)
flash name:
    #!/usr/bin/env bash
    set -euo pipefail
    d="examples/{{name}}"
    if [ -f "$d/Cargo.toml" ]; then elf="$d/target/msp430-none-elf/release/{{name}}"
    elif [ -f "$d/Makefile" ]; then elf=$(ls "$d"/build/*.elf 2>/dev/null | head -1)
    else echo "no such example: {{name}}" >&2; exit 1; fi
    [ -n "${elf:-}" ] && [ -f "$elf" ] || { echo "no built ELF for {{name}} — run 'just example build {{name}}' first" >&2; exit 1; }
    echo "flashing $elf"
    # program then detach: on release the tilib driver runs the target (MSP430_Run),
    # so this returns immediately and the board executes the new firmware. No blocking
    # 'run' (that would hold the debug session open until Ctrl-C).
    DYLD_FALLBACK_LIBRARY_PATH={{libdir}} {{mspdebug}} tilib "prog $elf" exit

# build then flash <name>
run name: (build name) (flash name)
