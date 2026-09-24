#!/usr/bin/env bash
# Firmware footprint gate. Runs `msp430-elf-size` on a built ELF, computes the FRAM and SRAM
# footprints, and FAILS (exit 1) if either exceeds the target part's budget. This is the machinery
# that keeps the product firmware inside the FR2433's 15 KB program FRAM — the linker only catches
# an overflow *at link time* for whatever region memory.x declares; this gates against the real
# part budget explicitly, reports headroom, and can be pointed at any ELF (diag or prod).
#
# Usage: size-check.sh <elf> <fram_budget_bytes> <ram_budget_bytes> [label] [stack_floor_bytes]
#
# Footprint model (MSP430, FRAM part):
#   FRAM (nonvolatile) = text + data   — code, rodata, vectors, and the init image of .data
#   SRAM (volatile)    = data + bss     — .data lives in RAM at runtime, plus .bss
# `msp430-elf-size` (Berkeley format) reports text/data/bss; we derive both footprints from it.
#
# STACK BUDGET (stack_floor_bytes, optional): the MSP430 is MMU-less — the stack grows DOWN into the
# same SRAM as .data/.bss, and an overflow silently corrupts them (no fault). Static RAM usage is
# known exactly here, but PEAK stack is not (that needs whole-program analysis or runtime painting).
# So this gate enforces a conservative *available-stack floor*: it fails if the RAM the build leaves
# free (ram_budget - static RAM) is less than stack_floor_bytes — i.e. the build must reserve at
# least that much headroom for the stack. This is a necessary (not sufficient) check: it guarantees
# room, not that peak usage fits. Verify actual peak with runtime stack-painting on hardware (HIL).
set -euo pipefail

elf="${1:?usage: size-check.sh <elf> <fram_budget_bytes> <ram_budget_bytes> [label] [stack_floor_bytes]}"
fram_budget="${2:?missing fram_budget_bytes}"
ram_budget="${3:?missing ram_budget_bytes}"
label="${4:-$(basename "$elf")}"
stack_floor="${5:-0}"

[ -f "$elf" ] || { echo "size-check: no ELF at $elf" >&2; exit 1; }
command -v msp430-elf-size >/dev/null || { echo "size-check: msp430-elf-size not on PATH (run inside the toolchain container / native env)" >&2; exit 1; }

# Berkeley format: header line then "  text   data    bss    dec    hex filename"
read -r text data bss _ < <(msp430-elf-size "$elf" | awk 'NR==2 {print $1, $2, $3}')

fram=$((text + data))
ram=$((data + bss))
ram_free=$((ram_budget - ram))
fram_pct=$((fram * 100 / fram_budget))
ram_pct=$((ram * 100 / ram_budget))

printf '── size gate: %s ──\n' "$label"
printf '   FRAM   %6d / %6d B  (%3d%%, %d free)\n' "$fram" "$fram_budget" "$fram_pct" "$((fram_budget - fram))"
printf '   SRAM   %6d / %6d B  (%3d%%, %d free)\n' "$ram"  "$ram_budget"  "$ram_pct"  "$ram_free"
if [ "$stack_floor" -gt 0 ]; then
    printf '   stack  %6d free vs %6d floor  (static RAM leaves this for stack+heap)\n' "$ram_free" "$stack_floor"
fi

fail=0
if [ "$fram" -gt "$fram_budget" ]; then
    echo "   ✗ FRAM OVER budget by $((fram - fram_budget)) B" >&2
    fail=1
fi
if [ "$ram" -gt "$ram_budget" ]; then
    echo "   ✗ SRAM OVER budget by $((ram - ram_budget)) B" >&2
    fail=1
fi
if [ "$stack_floor" -gt 0 ] && [ "$ram_free" -lt "$stack_floor" ]; then
    echo "   ✗ STACK FLOOR: only $ram_free B free for stack, need >= $stack_floor B (raise RAM headroom or the floor)" >&2
    fail=1
fi
[ "$fail" -eq 0 ] || exit 1
echo "   ✓ within budget"
