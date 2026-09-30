# `just bsp <recipe>` — BSP pin registry docs (module).
#
# Source of truth = crates/bsp/connections.toml. Human docs are GENERATED from it (never hand-edited),
# mirroring the ziloo pinmux pattern — so the pin map / bus roles can't drift from what the firmware
# is built around.
#
#   just bsp docs    regenerate crates/bsp/generated/*.md from connections.toml
#   just bsp check   verify the generated docs are up to date (CI drift gate; non-zero if stale)

# regenerate the generated BSP docs (PINMAP.md, BUS-ROLES.md) from connections.toml
docs:
    python3 scripts/gen-bsp-docs.py

# fail if the generated docs are stale vs connections.toml (run in CI after edits)
check:
    python3 scripts/gen-bsp-docs.py --check
