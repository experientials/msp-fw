# `just check <recipe>` — health checks (module).
#
#   just check deps              verify the build toolchain is present
#   just check deps -- --flash   also check hardware-programming tools
#   just check deps -- --deep    authoritative: actually compile a tiny ELF
#
# Extra flags after `--` pass straight through to check-deps.sh.

# verify toolchain dependencies
deps *args:
    ./check-deps.sh {{args}}
