# `just usb <verb>` — diagnose / recover the eZ-FET USB connection (macOS host).
#
#   just usb status     is the LaunchPad on the bus? which /dev node? any stale holder?
#   just usb recover    kill stale FET/port holders, then wait for re-enumeration
#   just usb wait       poll until /dev/cu.usbmodem* appears
#
# NOTE: macOS cannot re-enumerate a device that has physically dropped off the bus. If
# `status` says "NOT on the bus", the fix is physical — unplug OLED/shorts, power-cycle USB
# ~20 s, use a direct port + data cable, or a full Shut Down (Apple Silicon). This automates
# only the softwarey parts: clearing stale holders and confirming when the board returns.

# grep pattern that matches the TI eZ-FET on `system_profiler SPUSBDataType`
ti := "0x2047|0x0451|Texas Instr|eZ.?FET|MSP.?FET|Application UART"

# report bus presence, /dev node, and any stale holder
status:
    #!/usr/bin/env bash
    set -uo pipefail
    echo "== eZ-FET USB status =="
    if system_profiler SPUSBDataType 2>/dev/null | grep -qiE '{{ti}}'; then
        echo "  bus : TI eZ-FET PRESENT on USB"; bus=1
    else
        echo "  bus : NOT on the USB bus"; bus=0
    fi
    node=$(ls /dev/cu.usbmodem* 2>/dev/null || true)
    [ -n "$node" ] && echo "  dev : $node" || echo "  dev : no /dev/cu.usbmodem*"
    h=$(pgrep -fl mspdebug || true)
    [ -n "$h" ] && echo "  hold: $h" || echo "  hold: no stale mspdebug"
    if [ "$bus" = 1 ] && [ -z "$node" ]; then
        echo "  -> on the bus but no /dev node = macOS quirk; log out / reboot to create it."
    fi

# clear stale FET/port holders, then wait for the board to re-enumerate
recover:
    #!/usr/bin/env bash
    set -uo pipefail
    echo "== recover eZ-FET =="
    # 1. clear software holders (these cause 'No unused FET found' when the board IS present)
    if pgrep -x mspdebug >/dev/null 2>&1; then pkill -x mspdebug && echo "  killed stale mspdebug"; fi
    for pid in $(lsof -t /dev/cu.usbmodem* 2>/dev/null || true); do
        echo "  freeing port held by PID $pid ($(ps -p "$pid" -o comm= 2>/dev/null))"
        kill "$pid" 2>/dev/null || true
    done
    # 2. already back?
    if ls /dev/cu.usbmodem* >/dev/null 2>&1; then
        echo "  ✓ device present: $(ls /dev/cu.usbmodem*)"; exit 0
    fi
    # 3. guide + wait — macOS can't bring back an absent device from software
    echo "  device is off the bus — physically:"
    echo "    1) unplug the OLED (clear any short)"
    echo "    2) power-cycle the HUB too: unplug the LaunchPad AND the hub ~20 s"
    echo "       (a latched hub port won't clear from re-plugging just the board)"
    echo "    3) prefer a POWERED hub + a data cable"
    echo "  waiting up to 45 s for it to return..."
    for i in $(seq 1 45); do
        if ls /dev/cu.usbmodem* >/dev/null 2>&1; then
            echo "  ✓ back after ${i}s: $(ls /dev/cu.usbmodem*) — now: just diag run"; exit 0
        fi
        sleep 1
    done
    echo "  ✗ still absent after 45 s — try another cable/port, or full Shut Down (Apple Silicon)."
    exit 1

# block until the serial node appears
wait:
    #!/usr/bin/env bash
    set -uo pipefail
    echo "waiting for /dev/cu.usbmodem* (Ctrl-C to stop)..."
    until ls /dev/cu.usbmodem* >/dev/null 2>&1; do sleep 1; done
    echo "✓ $(ls /dev/cu.usbmodem*)"
