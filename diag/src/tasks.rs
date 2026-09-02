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

use crate::{apds, buttons, diag, rcwl, ssd1306_raw as ssd, uart};
use msp430fr2476::Peripherals;
use sched::Task;

/// What the firmware is currently doing. **One button (S1) cycles through these in order** —
/// `Post → Margin → Soak → Post` — so a single press starts a stress test, the next press ends it
/// and jumps to the following one. A test that finishes on its own returns to `Post`. `Post` is the
/// default (boots into it) and the resting state; the bus tests were formerly `--features stress`.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Post,
    Margin,
    Soak,
}

impl Mode {
    /// The next state in the one-button cycle.
    pub fn next(self) -> Mode {
        match self {
            Mode::Post => Mode::Margin,
            Mode::Margin => Mode::Soak,
            Mode::Soak => Mode::Post,
        }
    }

    /// Short label for the OLED / console.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Post => "POST",
            Mode::Margin => "I2C MARGIN",
            Mode::Soak => "I2C SOAK",
        }
    }
}

/// Context handed to every task each `poll`. `p` is the hardware; the rest is the cross-task
/// blackboard. Keep shared state here small and explicit — task-private state belongs in the task.
pub struct Cx<'a> {
    pub p: &'a Peripherals,
    /// Any motion seen since the last POST report.
    pub radar_seen: bool,
    /// Rising edges (fresh triggers) counted since the last POST report.
    pub radar_edges: u16,
    /// Latest APDS-9960 proximity (0 far .. 255 near); `None` if absent / not yet valid.
    pub apds_prox: Option<u8>,
    /// Current mode (advanced by `ButtonTask`, read by everyone).
    pub mode: Mode,
    // Latest POST verdict, published by `PostTask`, rendered by `UiTask` (single display owner):
    pub post_ok: bool,
    pub post_present: u16,
    pub post_total: u16,
    /// Whether the OLED took its last `init()` (published by `UiTask`, reported by `PostTask`).
    /// Restores the "OLED rendered / init FAILED" health signal after the display moved to UiTask.
    pub oled_ok: bool,
    // Stress status published by `StressTask`, rendered by `UiTask`:
    /// 0 idle, 1 sweeping, 2 sweep done, 3 soaking.
    pub st_phase: u8,
    pub st_khz: u16, // current sweep step
    pub st_max: u16, // best clean clock found
    pub st_secs: u32, // soak elapsed
    pub st_txn_m: u32, // soak transactions (millions + remainder)
    pub st_txn: u32,
    pub st_err: u32, // soak errors (nack + corrupt)
}

impl<'a> Cx<'a> {
    pub fn new(p: &'a Peripherals) -> Self {
        Self {
            p,
            radar_seen: false,
            radar_edges: 0,
            apds_prox: None,
            mode: Mode::Post,
            post_ok: false,
            post_present: 0,
            post_total: 0,
            oled_ok: true,
            st_phase: 0,
            st_khz: 0,
            st_max: 0,
            st_secs: 0,
            st_txn_m: 0,
            st_txn: 0,
            st_err: 0,
        }
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
        // Pause during the bus stress tests so unrelated I²C traffic doesn't skew the soak counts.
        if cx.mode != Mode::Post {
            cx.apds_prox = None;
            return Some(Self::RETRY_MS);
        }
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
        // Only the live POST runs the scan/verdict; stress modes take over. The verdict is published
        // to the blackboard for UiTask to render — PostTask no longer touches the OLED directly.
        if cx.mode != Mode::Post {
            return Some(3000);
        }
        let (ok, present, total) = diag::run(cx.p);
        cx.post_ok = ok;
        cx.post_present = present;
        cx.post_total = total;
        // OLED health (the panel is drawn by UiTask; this reports whether its last init took —
        // restores the "OLED rendered / init FAILED" signal that used to live in diag::run).
        uart::puts(cx.p, "  OLED: ");
        uart::puts(cx.p, if cx.oled_ok { "ok\n" } else { "INIT FAILED\n" });
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

/// The one physical button (S1 / P4.0). Each **press edge** advances the mode one step in the cycle
/// `Post → Margin → Soak → Post` — so from POST a press starts the margin sweep, the next press ends
/// it and starts the soak, the next returns to POST. Only edges act (a held button doesn't repeat).
/// Debounced by the fixed poll cadence. StressTask is told to (re)start via `st_phase = 0`.
pub struct ButtonTask {
    prev: bool,
}

impl ButtonTask {
    pub const fn new() -> Self {
        Self { prev: false }
    }
}

impl<'a> Task<Cx<'a>> for ButtonTask {
    fn poll(&mut self, cx: &mut Cx<'a>, _now: u32) -> Option<u32> {
        let s1 = buttons::s1(cx.p);
        let edge = s1 && !self.prev;
        self.prev = s1;
        if edge {
            cx.mode = cx.mode.next();
            cx.st_phase = 0; // a fresh run is starting (or returning to POST)
            uart::puts(cx.p, "\n[S1] -> ");
            uart::puts(cx.p, cx.mode.label());
            uart::puts(cx.p, "\n");
        }
        Some(25) // ~40 Hz scan = the debounce interval
    }

    fn name(&self) -> &'static str {
        "btn"
    }
}

/// **Sole owner of the OLED.** Renders the current state as a status screen (the user doesn't
/// navigate it — it just reflects what the system is doing). Boots with a self-test: the panel is
/// filled fully ON for ~1.5 s so we can *see* the raw driver works before trusting any layout.
/// Then it shows POST / MARGIN / SOAK, reading verdict + stress status from the blackboard.
pub struct UiTask {
    screen: Option<Mode>, // last-rendered mode (None until first draw)
    started: bool,
    boot_end: u32,
}

impl UiTask {
    pub const fn new() -> Self {
        Self { screen: None, started: false, boot_end: 0 }
    }
}

impl<'a> Task<Cx<'a>> for UiTask {
    fn poll(&mut self, cx: &mut Cx<'a>, now: u32) -> Option<u32> {
        let p = cx.p;

        // Boot self-test: init once, fill the whole panel ON. A fully-lit display is the
        // unambiguous "the raw driver works" signal; hold it briefly before the first real screen.
        if !self.started {
            self.started = true;
            self.boot_end = now.wrapping_add(1500);
            cx.oled_ok = ssd::init(p);
            ssd::fill(p, 0xFF);
            return Some(200);
        }
        if now < self.boot_end {
            return Some(100); // keep the self-test pattern up
        }

        let mode = cx.mode;
        let entering = self.screen != Some(mode);
        if entering {
            self.screen = Some(mode);
            cx.oled_ok = ssd::init(p);
            ssd::clear(p);
        }

        match mode {
            Mode::Post => {
                ssd::text(p, 0, 0, "POST");
                let c = ssd::text(p, 1, 0, "FOUND ");
                let c = ssd::num(p, 1, c, cx.post_present as u32);
                let c = ssd::text(p, 1, c, "/");
                let c = ssd::num(p, 1, c, cx.post_total as u32);
                ssd::clear_eol(p, 1, c);
                ssd::text(p, 3, 0, if cx.post_ok { "OK" } else { "FAULT" });
            }
            Mode::Margin => {
                ssd::text(p, 0, 0, "I2C MARGIN");
                if cx.st_phase >= 2 {
                    let c = ssd::text(p, 2, 0, "MAX:");
                    let c = ssd::num(p, 2, c, cx.st_max as u32);
                    let c = ssd::text(p, 2, c, "KHZ");
                    ssd::clear_eol(p, 2, c);
                } else {
                    let c = ssd::text(p, 2, 0, "SWEEP ");
                    let c = ssd::num(p, 2, c, cx.st_khz as u32);
                    let c = ssd::text(p, 2, c, "K");
                    ssd::clear_eol(p, 2, c);
                }
            }
            Mode::Soak => {
                ssd::text(p, 0, 0, "I2C SOAK");
                let c = ssd::text(p, 1, 0, "T=");
                let c = ssd::num(p, 1, c, cx.st_secs);
                let c = ssd::text(p, 1, c, "S");
                ssd::clear_eol(p, 1, c);
                let mut c = ssd::text(p, 2, 0, "N=");
                if cx.st_txn_m > 0 {
                    c = ssd::num(p, 2, c, cx.st_txn_m);
                    c = ssd::text(p, 2, c, "M");
                }
                let c = ssd::num(p, 2, c, cx.st_txn);
                ssd::clear_eol(p, 2, c);
                let c = ssd::text(p, 3, 0, "ERR=");
                let c = ssd::num(p, 3, c, cx.st_err);
                let c = ssd::text(p, 3, c, if cx.st_err == 0 { " PASS" } else { " FAIL" });
                ssd::clear_eol(p, 3, c);
            }
        }
        Some(200) // 5 Hz refresh
    }

    fn name(&self) -> &'static str {
        "ui"
    }
}
