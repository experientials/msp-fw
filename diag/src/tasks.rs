//! diag's cooperative tasks and the context they share.
//!
//! The [`sched`] crate is the generic engine; this module is diag's concrete use of it. `Cx` is
//! the context every task is handed: the peripherals handle plus a small **blackboard** for
//! cross-task data. Task-private state stays in the task struct (e.g. [`RadarTask::prev`]).
//!
//! This is where the radar became a first-class *task* rather than a once-per-pass snapshot:
//! [`RadarTask`] samples P2.4 continuously (fast while motion is asserted, idling slower) and
//! accumulates a motion window that [`PostTask`] reports and clears every ~3 s — so a trigger
//! that falls between POST passes is no longer missed.

use crate::{apds, diag, rcwl, uart};
use msp430fr2476::Peripherals;
use sched::Task;

/// Context handed to every task each `poll`. `p` is the hardware; the rest is the cross-task
/// blackboard (each field produced by one task, consumed by [`PostTask`]). Keep shared state here
/// small and explicit — task-private state belongs in the task itself.
pub struct Cx<'a> {
    pub p: &'a Peripherals,
    /// Any motion seen since the last POST report.
    pub radar_seen: bool,
    /// Rising edges (fresh triggers) counted since the last POST report.
    pub radar_edges: u16,
    /// Latest APDS-9960 proximity (0 far .. 255 near); `None` if absent / not yet valid.
    pub apds_prox: Option<u8>,
}

impl<'a> Cx<'a> {
    pub fn new(p: &'a Peripherals) -> Self {
        Self { p, radar_seen: false, radar_edges: 0, apds_prox: None }
    }
}

/// Samples the RCWL-0516 OUT line (P2.4) and accumulates a motion window into the blackboard.
///
/// Variable cadence (the [`sched`] "fixed/variable frequency" case): it samples tight while OUT is
/// asserted — to time the pulse and not miss a short one — and backs off when idle, where a 20 Hz
/// glance is plenty to catch the rising edge of the next ~2 s trigger. Private state is just the
/// previous level, for edge detection.
pub struct RadarTask {
    prev: bool,
}

impl RadarTask {
    const FAST_MS: u32 = 5; // while OUT is high
    const IDLE_MS: u32 = 50; // while OUT is idle

    pub const fn new() -> Self {
        Self { prev: false }
    }
}

impl<'a> Task<Cx<'a>> for RadarTask {
    fn poll(&mut self, cx: &mut Cx<'a>, _now: u32) -> Option<u32> {
        let hi = rcwl::motion(cx.p);
        if hi && !self.prev {
            cx.radar_edges = cx.radar_edges.saturating_add(1); // a fresh trigger
        }
        cx.radar_seen |= hi;
        self.prev = hi;
        Some(if hi { Self::FAST_MS } else { Self::IDLE_MS })
    }

    fn name(&self) -> &'static str {
        "radar"
    }
}

/// Exercises the APDS-9960 proximity engine (beyond the WHO_AM_I check in the registry): enables
/// it once — retrying while absent, so a sensor plugged in after boot still comes up — then
/// samples PDATA into the blackboard. Non-blocking: the sensor free-runs and we read the latest
/// byte. Hot-unplug is handled (a read failure + missing device re-arms `enable`), matching diag's
/// "watch it re-test live on the bench" ethos.
pub struct ProximityTask {
    enabled: bool,
}

impl ProximityTask {
    const RATE_MS: u32 = 200; // 5 Hz once running
    const RETRY_MS: u32 = 1000; // slower poll while the device is absent

    pub const fn new() -> Self {
        Self { enabled: false }
    }
}

impl<'a> Task<Cx<'a>> for ProximityTask {
    fn poll(&mut self, cx: &mut Cx<'a>, _now: u32) -> Option<u32> {
        if !self.enabled {
            if apds::enable(cx.p) {
                self.enabled = true;
            } else {
                cx.apds_prox = None; // still absent — back off
                return Some(Self::RETRY_MS);
            }
        }
        match apds::proximity(cx.p) {
            Some(v) => {
                cx.apds_prox = Some(v);
                Some(Self::RATE_MS)
            }
            None => {
                // None is either "sample not ready yet" or "gone" — probe to tell them apart.
                cx.apds_prox = None;
                if apds::present(cx.p) {
                    Some(Self::RATE_MS) // present, just not valid this instant
                } else {
                    self.enabled = false; // unplugged — re-init when it returns
                    Some(Self::RETRY_MS)
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "prox"
    }
}

/// The periodic power-on self-test: runs the I²C scan + device checks + display verdict, then
/// reports the blackboard values (radar window, APDS proximity) accumulated since the last pass
/// and clears the ones that latch. Fixed ~3 s cadence (the period set in `main`; returns `None`).
pub struct PostTask;

impl<'a> Task<Cx<'a>> for PostTask {
    fn poll(&mut self, cx: &mut Cx<'a>, _now: u32) -> Option<u32> {
        diag::run(cx.p);
        uart::puts(cx.p, "  radar window (P2.4): ");
        uart::puts(cx.p, if cx.radar_seen { "motion, " } else { "idle, " });
        uart::dec(cx.p, cx.radar_edges);
        uart::puts(cx.p, " trigger(s)\n");
        cx.radar_seen = false;
        cx.radar_edges = 0;

        uart::puts(cx.p, "  APDS-9960 prox (0x39): ");
        match cx.apds_prox {
            Some(v) => uart::dec(cx.p, v as u16),
            None => uart::puts(cx.p, "n/a"),
        }
        uart::puts(cx.p, "\n");
        None
    }

    fn name(&self) -> &'static str {
        "post"
    }
}
