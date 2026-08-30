//! Emits `DIAG_BUILD` — a short stamp compiled into the firmware and printed in the POST
//! banner. It exists to kill one specific ambiguity: "is the board actually running the build
//! I just flashed, or a stale image?" The running firmware announces its own identity over the
//! UART, so you can *see* the answer instead of guessing.
//!
//! `just diag run` pins this via the `DIAG_BUILD` env var (a fresh, unique value per run) and
//! then verifies that exact string comes back on the wire. A bare `cargo build` has no pin, so
//! we derive one from `git` + build time instead — still unique enough to tell two builds apart.

use std::process::Command;

fn main() {
    // Re-run when the caller pins a new stamp, or when HEAD moves (so a plain build tracks the
    // commit). Missing `.git` just means this rerun-trigger always fires — harmless.
    println!("cargo:rerun-if-env-changed=DIAG_BUILD");
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let stamp = match std::env::var("DIAG_BUILD") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => derive(),
    };
    println!("cargo:rustc-env=DIAG_BUILD={stamp}");
}

/// `<short-hash>[-dirty].<unix-secs>` — commit identity plus a timestamp so repeated builds of
/// the same (possibly dirty) tree still differ.
fn derive() -> String {
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".into());
    let dirty = match git(&["status", "--porcelain"]) {
        Some(s) if !s.trim().is_empty() => "-dirty",
        _ => "",
    };
    // build.rs runs on the host toolchain (std available) — only the *target* is no_std.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{hash}{dirty}.{secs}")
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
