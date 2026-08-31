#!/usr/bin/env python3
"""HIL assertion for the diag POST over the eZ-FET backchannel UART.

Captures a few POST cycles and checks that (1) the running firmware is the exact build we
flashed, (2) the expected devices are present, (3) nothing is reported FAULTY, and optionally
(4) the bus is left clean after the display writes. Exits non-zero with a clear message on any
failure so CI fails loudly. The full capture is written to hil-capture.log for the failure
artifact. Requires pyserial (`pip3 install pyserial`).

Used by `just diag hil` on the self-hosted runner; also runnable by hand at the bench.
"""
import argparse
import os
import sys
import time

try:
    import serial  # pyserial
except ImportError:
    print("hil_assert: pyserial not installed (pip3 install pyserial)", file=sys.stderr)
    sys.exit(2)

CAPTURE = os.path.join(os.path.dirname(__file__), "hil-capture.log")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", required=True, help="serial device, e.g. /dev/cu.usbmodemXXXX")
    ap.add_argument("--baud", type=int, default=9600)
    ap.add_argument("--stamp", default="", help="build stamp that must appear in a banner")
    ap.add_argument(
        "--expect",
        default="SSD1306 OLED,IS31FL3730 LED",
        help="comma-separated device-name substrings that must be reported 'present'",
    )
    ap.add_argument("--seconds", type=float, default=12.0, help="capture window")
    ap.add_argument(
        "--require-clean-bus",
        action="store_true",
        help="fail if any 'bus after' line is not H/H (once the eUSCI resync fix is confirmed)",
    )
    args = ap.parse_args()

    lines = []
    deadline = time.monotonic() + args.seconds
    try:
        with serial.Serial(args.port, args.baud, timeout=1) as ser:
            while time.monotonic() < deadline:
                raw = ser.readline()
                if not raw:
                    continue
                line = raw.decode("ascii", "replace").rstrip("\r\n")
                if line:
                    lines.append(line)
    except serial.SerialException as e:
        print(f"hil_assert: cannot open {args.port}: {e}", file=sys.stderr)
        return 2

    text = "\n".join(lines)
    try:
        with open(CAPTURE, "w") as f:
            f.write(text + "\n")
    except OSError:
        pass

    if not text.strip():
        print("hil_assert: no UART output captured — board silent or wrong port?", file=sys.stderr)
        return 1

    fails = []

    # 1. Build identity — proves we're running THIS flash, not a stale image.
    if args.stamp and args.stamp not in text:
        fails.append(f"build stamp {args.stamp!r} never seen (stale or failed flash?)")

    # 2. Expected devices present. A device line looks like "  SSD1306 OLED   @0x3C: present".
    for name in (d.strip() for d in args.expect.split(",") if d.strip()):
        if not any(name in ln and ln.rstrip().endswith("present") for ln in lines):
            fails.append(f"expected device {name!r} not reported present")

    # 3. No faults (present-but-wrong / id-read failed -> summary says FAULTY).
    if any("FAULTY" in ln for ln in lines):
        fails.append("summary reported FAULTY device(s)")

    # 4. Bus teardown — warn by default (the resync fix may not be confirmed yet), hard-fail
    #    once --require-clean-bus is enabled.
    dirty = [ln for ln in lines if "bus after" in ln and "H/H" not in ln]
    if dirty:
        msg = "bus not clean after writes: " + "; ".join(dirty)
        if args.require_clean_bus:
            fails.append(msg)
        else:
            print("hil_assert: WARNING " + msg, file=sys.stderr)

    if fails:
        print("hil_assert: FAIL", file=sys.stderr)
        for f in fails:
            print("  - " + f, file=sys.stderr)
        print(f"---- captured {len(lines)} lines (also in {CAPTURE}) ----", file=sys.stderr)
        print(text, file=sys.stderr)
        return 1

    print(f"hil_assert: PASS ({len(lines)} lines; stamp ok, devices present, no faults)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
