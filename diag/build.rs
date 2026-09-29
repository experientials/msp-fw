//! Emits `DIAG_BUILD` — the firmware's identity stamp, compiled in and printed in the POST banner.
//! It exists to kill one specific ambiguity: "is the board running the build I just flashed, or a
//! stale image?" The firmware announces its own identity over the UART, so you can *see* the answer.
//!
//! Format: `[<target>/]<year>.<release>-<short-hash>[-dirty][.<secs>]`
//! (e.g. `2026.dev-59ad941-dirty.1725540000`, or `fr2433/2026.1-59ad941` once we build per chip)
//!   <target>  build target — the chip/config variant (FR2433 vs FR2476, role/placement), from
//!             `DIAG_TARGET`. **Omitted today**: one FR2476 dev build. This is the real axis of
//!             variation (kept few, FRAM-size-constrained); the firmware autodetects finer
//!             placement at boot. The board crate will set it when we build a config per chip.
//!   <year>    build year (UTC).
//!   <release> `DIAG_RELEASE` when set (a real release), else `dev`. There is **no release system
//!             yet** — every build is a dev build until a release process sets this to a number.
//!   -dirty    the working tree had uncommitted changes.
//!   .<secs>   dev builds only, so rebuilding the same (dirty) tree still yields a distinct stamp —
//!             the stale-firmware check in `just diag run` needs every build to differ. A real
//!             release is immutable (target + release + commit already make it unique), so it's omitted.
//!
//! `just diag run` / CI may pin a unique suffix via the `DIAG_BUILD` env var; we prefix the
//! target+version onto it. `DIAG_VERSION` (`<year>.<release>`) is also emitted.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DIAG_BUILD");
    println!("cargo:rerun-if-env-changed=DIAG_RELEASE");
    println!("cargo:rerun-if-env-changed=DIAG_TARGET");
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let version = version(); // <year>.<release>
    let is_dev = version.ends_with(".dev");
    let prefix = match target() {
        Some(t) => format!("{t}/{version}"), // [<target>/]<year>.<release>
        None => version.clone(),
    };
    let stamp = match std::env::var("DIAG_BUILD") {
        // A pinned suffix (just diag run, CI): keep it verbatim but lead with target+version.
        Ok(s) if !s.trim().is_empty() => format!("{prefix}-{s}"),
        _ => derive(&prefix, is_dev),
    };
    println!("cargo:rustc-env=DIAG_VERSION={version}");
    println!("cargo:rustc-env=DIAG_BUILD={stamp}");

    emit_memory_x();
}

/// Emit the linker memory map for the selected chip into OUT_DIR (found via link.x's
/// `INCLUDE memory.x` + the link-search below). The ONLY delta between the two targets is RAM size
/// (FR2476 = 8 KB, FR2355 = 4 KB); ROM window and the minimal no-ISR vector region are shared.
/// Generating it here (not a static file) is what makes both chips first-class build targets from
/// one tree — no file swapping.
fn emit_memory_x() {
    use std::io::Write;
    let ram_len = if std::env::var("CARGO_FEATURE_FR2355").is_ok() {
        "0x1000" // FR2355: 4 KB SRAM (0x2000-0x2FFF)
    } else {
        "0x2000" // FR2476: 8 KB SRAM (0x2000-0x3FFF)
    };
    let memx = format!(
        "MEMORY\n\
         {{\n\
        \x20 RAM     : ORIGIN = 0x2000, LENGTH = {ram_len}\n\
        \x20 ROM     : ORIGIN = 0x8000, LENGTH = 0x7F80\n\
        \x20 /* diag uses no interrupts: only the reset vector (0xFFFE) matters, so the vector\n\
        \x20    region is the minimal 0xFFE0-0xFFFF. Revisit with the PAC `rt` feature if ISRs land. */\n\
        \x20 VECTORS : ORIGIN = 0xFFE0, LENGTH = 0x0020\n\
         }}\n"
    );
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let path = std::path::Path::new(&out).join("memory.x");
    std::fs::File::create(&path)
        .expect("create memory.x")
        .write_all(memx.as_bytes())
        .expect("write memory.x");
    println!("cargo:rustc-link-search={out}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_FR2355");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_FR2476");
}

/// Build target (chip/config variant), e.g. `fr2433`, `fr2476`. `None` until we build per-chip —
/// see the board-crate plan (chip = cargo feature, role = FRAM config).
fn target() -> Option<String> {
    // Explicit override wins; otherwise derive from the active chip feature so the stamp always
    // carries which target it was built for (fr2355/… vs fr2476/…) with no manual env.
    if let Ok(t) = std::env::var("DIAG_TARGET") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    if std::env::var("CARGO_FEATURE_FR2355").is_ok() {
        return Some("fr2355".into());
    }
    if std::env::var("CARGO_FEATURE_FR2476").is_ok() {
        return Some("fr2476".into());
    }
    None
}

/// `<year>.<release>` — CalVer. Year from the build clock; release from `DIAG_RELEASE` (a release
/// build) or `dev` (no release system yet).
fn version() -> String {
    let year = run("date", &["-u", "+%Y"]).unwrap_or_else(|| "0000".into());
    let release = match std::env::var("DIAG_RELEASE") {
        Ok(r) if !r.trim().is_empty() => r,
        _ => "dev".into(),
    };
    format!("{year}.{release}")
}

/// Full derived stamp when no `DIAG_BUILD` suffix is pinned: prefix + commit + dirty (+ time on dev).
fn derive(prefix: &str, is_dev: bool) -> String {
    let hash = run("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".into());
    let dirty = match run("git", &["status", "--porcelain"]) {
        Some(s) if !s.trim().is_empty() => "-dirty",
        _ => "",
    };
    let mut stamp = format!("{prefix}-{hash}{dirty}");
    if is_dev {
        // build.rs runs on the host toolchain (std available) — only the *target* is no_std.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        stamp = format!("{stamp}.{secs}");
    }
    stamp
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
