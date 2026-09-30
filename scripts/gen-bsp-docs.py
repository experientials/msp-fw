#!/usr/bin/env python3
"""Generate the MSP430 BSP docs from crates/bsp/connections.toml (the single source of truth).

This mirrors the ziloo pinmux pattern (`Hardware/pinmux/render.py`): one structured source →
generated Markdown views, so the pin map and bus-role docs can NEVER drift from the registry the
firmware is built around. Never hand-maintain parallel pin tables — that is exactly the drift that
left the TEST-FR2476 symbol reversed vs connections.toml.

Outputs (crates/bsp/generated/, do-not-hand-edit):
  - PINMAP.md    — every connection, grouped by status, as a table (mechanical from [[connection]]).
  - BUS-ROLES.md — the I2C buses + their master/slave roles ([[bus]]) with per-bus pin membership.

Usage:
  python3 scripts/gen-bsp-docs.py            # regenerate the files
  python3 scripts/gen-bsp-docs.py --check    # verify on-disk == freshly generated (CI drift gate)
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "crates" / "bsp" / "connections.toml"
OUT_DIR = REPO / "crates" / "bsp" / "generated"
REL_SRC = SRC.relative_to(REPO)
GEN = "scripts/gen-bsp-docs.py"

STATUS_ORDER = ["active", "planned", "reserved"]
ROLE_TEXT = {
    "master": "MSP is the **master**",
    "slave": "MSP is always the **slave**",
    "master-switch": "**master switches** (MSP ↔ SoM)",
}


def banner(title: str) -> str:
    return (
        f"<!-- GENERATED — DO NOT EDIT. Source: {REL_SRC}. Regenerate: `just bsp docs` "
        f"({GEN}). -->\n\n# {title}\n\n"
        f"> Generated from [`{REL_SRC}`](../connections.toml) by `{GEN}`. "
        f"Edit the TOML, then run `just bsp docs` — never edit this file by hand.\n\n"
    )


def cell(v) -> str:
    """Markdown-table-safe cell (escape pipes, collapse newlines, em-dash for missing)."""
    if v is None or v == "":
        return "—"
    return str(v).replace("|", "\\|").replace("\n", " ").strip()


def render_pinmap(data: dict) -> str:
    chip = data.get("chip", {})
    conns = data.get("connection", [])
    out = [banner("MSP430 BSP — pin map (bob-929)")]
    out.append(
        f"**Chip:** `{chip.get('part', '?')}` · **package:** {cell(chip.get('package'))} · "
        f"**datasheet:** {cell(chip.get('datasheet'))}\n\n"
        f"{len(conns)} connections. `status`: active = wired/verified · planned = product-intent, "
        "confirm before activating · reserved = programming/debug.\n"
    )
    cols = ["ID", "Signal", "Pin", "Function", "Module", "Dir", "Net"]
    keys = ["id", "signal", "pin", "function", "module", "dir", "net"]
    for status in STATUS_ORDER:
        rows = [c for c in conns if c.get("status") == status]
        if not rows:
            continue
        out.append(f"\n## {status.capitalize()} ({len(rows)})\n\n")
        out.append("| " + " | ".join(cols) + " |\n")
        out.append("|" + "|".join(["---"] * len(cols)) + "|\n")
        for c in rows:
            out.append("| " + " | ".join(cell(c.get(k)) for k in keys) + " |\n")
    # Any status not in STATUS_ORDER → surface rather than silently drop.
    unknown = sorted({c.get("status") for c in conns} - set(STATUS_ORDER) - {None})
    if unknown:
        out.append(f"\n> ⚠ Unlisted status values present (not rendered above): {unknown}\n")
    return "".join(out)


def render_bus_roles(data: dict) -> str:
    buses = data.get("bus", [])
    conns = data.get("connection", [])
    out = [banner("MSP430 BSP — I2C bus roles (bob-929)")]
    out.append(
        "**The MSP is on exactly these I2C buses and nothing else** — never SYS_I2C or the PMIC "
        "bus (those are SoM-side). Master model below is the canonical fact; see also "
        "`ziloo/Hardware/stem/STEM-EXPANDER.md`.\n"
    )
    for b in buses:
        role = ROLE_TEXT.get(b.get("role"), f"role: {cell(b.get('role'))}")
        out.append(
            f"\n## {cell(b.get('name'))} — `{cell(b.get('module'))}`\n\n"
            f"- **Role:** {role}. {cell(b.get('desc'))}\n\n"
        )
        members = [c for c in conns if c.get("module") == b.get("module")]
        if members:
            out.append("| Pin | Function | Signal | Status |\n|---|---|---|---|\n")
            for c in members:
                out.append(
                    f"| {cell(c.get('pin'))} | {cell(c.get('function'))} | "
                    f"{cell(c.get('signal'))} | {cell(c.get('status'))} |\n"
                )
        else:
            out.append(f"_No connections found with module `{cell(b.get('module'))}`._\n")
    return "".join(out)


def build(data: dict) -> dict[str, str]:
    return {"PINMAP.md": render_pinmap(data), "BUS-ROLES.md": render_bus_roles(data)}


def main() -> int:
    check = "--check" in sys.argv[1:]
    with SRC.open("rb") as f:
        data = tomllib.load(f)
    files = build(data)

    if check:
        stale = []
        for name, content in files.items():
            p = OUT_DIR / name
            if not p.exists() or p.read_text() != content:
                stale.append(name)
        if stale:
            print(f"STALE (run `just bsp docs`): {', '.join(stale)}", file=sys.stderr)
            return 1
        print(f"BSP docs up to date ({', '.join(files)}).")
        return 0

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for name, content in files.items():
        (OUT_DIR / name).write_text(content)
        print(f"wrote crates/bsp/generated/{name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
