//! UI health monitoring: the frame heartbeat and the watchdog that reads it.
//!
//! The UI calls [`frame_begin`]/[`frame_end`] around every build. A watchdog
//! thread samples that heartbeat and feeds a pure [`Monitor`], which turns
//! samples into alerts:
//!
//! | Rule | Fires when | Severity |
//! |------|------------|----------|
//! | `ui_stall` | one frame has been building longer than `stall` | Critical |
//! | `ui_stall_recovered` | that frame finally completed | Warning |
//! | `frame_storm` | ≥ `storm_fps` frames/s sustained for `storm_window` | Critical |
//! | `error_burst` | ≥ `error_burst` errors logged within `error_window` | Warning |
//!
//! A stall is the "window froze" symptom: the event loop is stuck inside a
//! frame on synchronous work. A storm is the opposite failure: frames build
//! back-to-back with nothing reaching the screen, which is what a lost
//! compositor connection looks like from inside the client (the display fd
//! reports hang-up on every poll, so the loop never sleeps).
//!
//! An idle window is *not* a stall: between frames the heartbeat reads idle,
//! however long the user leaves the window alone.
//!
//! The frame heartbeat only brackets Arca's own build; the toolkit's layout,
//! store maintenance, and render run after it returns. A loop there leaves
//! the heartbeat idle, so on Linux the watchdog also reads the UI thread's
//! CPU time (`/proc/thread-self/schedstat`): a thread pinned near 100 % for
//! the stall budget without completing a frame is the same freeze, raised as
//! `ui_stall` "outside the frame build". Idle time costs no CPU, so an idle
//! window still never trips it.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::alerts::{self, Alert, AlertKind, Severity};
use super::metrics;

/// Frame heartbeat of one UI thread.
///
/// Each thread that builds frames owns one (created on first use and
/// registered weakly), so concurrent headless UIs in one test process — or a
/// chooser beside a window — never reset each other's frame state.
pub struct Heartbeat {
    epoch: Instant,
    /// Nanoseconds since `epoch` + 1 when the current frame began; 0 = idle.
    frame_started: AtomicU64,
    frames: AtomicU64,
    /// Innermost span open on the owning thread (see [`super::span`]).
    operation: Mutex<Option<&'static str>>,
    /// The owning thread's `schedstat` (on-CPU nanoseconds first), if the
    /// platform exposes one.
    cpu_stat: Option<std::path::PathBuf>,
}

impl Heartbeat {
    fn new() -> Heartbeat {
        Heartbeat {
            epoch: Instant::now(),
            frame_started: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            operation: Mutex::new(None),
            cpu_stat: thread_schedstat(),
        }
    }

    /// Total CPU time the owning thread has consumed, where measurable.
    #[must_use]
    pub fn cpu_time(&self) -> Option<Duration> {
        let text = std::fs::read_to_string(self.cpu_stat.as_ref()?).ok()?;
        let ns = text.split_whitespace().next()?.parse::<u64>().ok()?;
        Some(Duration::from_nanos(ns))
    }

    /// The innermost span open on the owning thread.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        *self
            .operation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn set_operation(&self, operation: Option<&'static str>) {
        *self
            .operation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = operation;
    }

    fn now_ns(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    /// Completed frames so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// How long the in-progress frame has been running, if one is.
    #[must_use]
    pub fn in_frame_for(&self) -> Option<Duration> {
        match self.frame_started.load(Ordering::Acquire) {
            0 => None,
            started => Some(Duration::from_nanos(
                self.now_ns().saturating_sub(started - 1),
            )),
        }
    }
}

/// Stable path of the calling thread's schedstat: `/proc/thread-self` is a
/// link to `<pid>/task/<tid>`, resolved here so other threads can read it.
fn thread_schedstat() -> Option<std::path::PathBuf> {
    let task = std::fs::read_link("/proc/thread-self").ok()?;
    let path = std::path::Path::new("/proc").join(task).join("schedstat");
    path.exists().then_some(path)
}

static REGISTRY: Mutex<Vec<Weak<Heartbeat>>> = Mutex::new(Vec::new());

thread_local! {
    static CURRENT: Arc<Heartbeat> = {
        let heartbeat = Arc::new(Heartbeat::new());
        let mut registry = REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(&heartbeat));
        heartbeat
    };
}

/// The calling thread's heartbeat (created and registered on first use).
#[must_use]
pub fn heartbeat() -> Arc<Heartbeat> {
    CURRENT.with(Arc::clone)
}

/// Every live UI heartbeat.
fn live_heartbeats() -> Vec<Arc<Heartbeat>> {
    REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

/// Mark the start of a UI frame. Pair with [`frame_end`] (see [`FrameGuard`]).
pub fn frame_begin() {
    let hb = heartbeat();
    hb.frame_started.store(hb.now_ns() + 1, Ordering::Release);
}

/// Mark the end of a UI frame, recording its duration into `ui.frame`.
pub fn frame_end() {
    let hb = heartbeat();
    let started = hb.frame_started.swap(0, Ordering::AcqRel);
    hb.frames.fetch_add(1, Ordering::Relaxed);
    if started != 0 {
        let elapsed = Duration::from_nanos(hb.now_ns().saturating_sub(started - 1));
        metrics::observe("ui.frame", elapsed);
    }
}

/// RAII pairing of [`frame_begin`]/[`frame_end`]; ends the frame even if the
/// build panics.
#[must_use = "the frame ends when the guard drops"]
pub struct FrameGuard(());

impl FrameGuard {
    pub fn begin() -> FrameGuard {
        frame_begin();
        FrameGuard(())
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        frame_end();
    }
}

/// Watchdog thresholds.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// A frame running longer than this is a stall (`ARCA_STALL_MS`).
    pub stall: Duration,
    /// Sampling period of the watchdog thread.
    pub poll: Duration,
    /// Frame rate that no display refreshes at; sustained, it is a spin.
    pub storm_fps: u64,
    pub storm_window: Duration,
    pub error_burst: u64,
    pub error_window: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            stall: Duration::from_millis(750),
            poll: Duration::from_millis(100),
            storm_fps: 400,
            storm_window: Duration::from_secs(3),
            error_burst: 20,
            error_window: Duration::from_secs(60),
        }
    }
}

/// One watchdog observation.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Monotonic time of the sample.
    pub at: Duration,
    pub in_frame_for: Option<Duration>,
    pub frames: u64,
    pub errors: u64,
    pub operation: Option<&'static str>,
    /// Cumulative CPU time of the UI thread; `None` where unmeasurable.
    pub cpu: Option<Duration>,
}

/// Share of one core a thread must hold, with no frame completing, to count
/// as busy outside the frame build.
const BUSY_CPU_SHARE: f64 = 0.9;

/// Pure alerting state machine over [`Sample`]s.
pub struct Monitor {
    config: Config,
    /// Frame counter value of the frame currently reported as stalled.
    stalled_frame: Option<(u64, Option<&'static str>)>,
    longest_stall: Duration,
    frame_history: VecDeque<(Duration, u64)>,
    storming: bool,
    error_history: VecDeque<(Duration, u64)>,
    bursting: bool,
    /// `(at, frames, cpu)` over the last `stall`, for the busy-outside-frame
    /// rule.
    cpu_history: VecDeque<(Duration, u64, Duration)>,
    /// When the reported busy-outside-frame episode began.
    busy_since: Option<Duration>,
}

fn op_suffix(operation: Option<&'static str>) -> String {
    operation.map_or_else(String::new, |op| format!(" in {op}"))
}

/// Drop history older than `window` but keep one sample at or beyond it, so
/// the oldest retained sample spans the full window.
fn trim(history: &mut VecDeque<(Duration, u64)>, now: Duration, window: Duration) {
    while history.len() > 1 && now.saturating_sub(history[1].0) >= window {
        history.pop_front();
    }
}

impl Monitor {
    #[must_use]
    pub fn new(config: Config) -> Monitor {
        Monitor {
            config,
            stalled_frame: None,
            longest_stall: Duration::ZERO,
            frame_history: VecDeque::new(),
            storming: false,
            error_history: VecDeque::new(),
            bursting: false,
            cpu_history: VecDeque::new(),
            busy_since: None,
        }
    }

    /// Feed one sample; returns the alerts it triggers.
    pub fn observe(&mut self, sample: &Sample) -> Vec<Alert> {
        let mut raised = Vec::new();
        self.check_stall(sample, &mut raised);
        self.check_busy(sample, &mut raised);
        self.check_storm(sample, &mut raised);
        self.check_errors(sample, &mut raised);
        raised
    }

    /// The monitored thread is gone. A stall still open was ended by the
    /// frame completing (or the thread exiting); report its recovery rather
    /// than losing it with the heartbeat.
    pub fn finish(&mut self) -> Vec<Alert> {
        if let Some((_, operation)) = self.stalled_frame.take() {
            metrics::observe("ui.stall", self.longest_stall);
            return vec![Self::recovered(self.longest_stall, operation)];
        }
        // A busy episode's end time is unknown here; nothing to report.
        self.busy_since = None;
        Vec::new()
    }

    fn recovered(longest: Duration, operation: Option<&'static str>) -> Alert {
        Alert::new(
            AlertKind::UiStallRecovered,
            Severity::Warning,
            format!(
                "UI was unresponsive for {} ms{}",
                longest.as_millis(),
                op_suffix(operation)
            ),
        )
    }

    fn check_stall(&mut self, sample: &Sample, raised: &mut Vec<Alert>) {
        // The frame that stalled has completed once the counter moved on or
        // the heartbeat went idle.
        if let Some((frame, operation)) = self.stalled_frame {
            let same_frame = sample.frames == frame && sample.in_frame_for.is_some();
            if same_frame {
                self.longest_stall = self
                    .longest_stall
                    .max(sample.in_frame_for.unwrap_or_default());
                return;
            }
            self.stalled_frame = None;
            metrics::observe("ui.stall", self.longest_stall);
            raised.push(Self::recovered(self.longest_stall, operation));
        }
        if let Some(running) = sample.in_frame_for
            && running >= self.config.stall
        {
            self.stalled_frame = Some((sample.frames, sample.operation));
            self.longest_stall = running;
            raised.push(Alert::new(
                AlertKind::UiStall,
                Severity::Critical,
                format!(
                    "UI unresponsive for {} ms{}",
                    running.as_millis(),
                    op_suffix(sample.operation)
                ),
            ));
        }
    }

    /// The UI thread burns CPU between frames without finishing one: a loop
    /// in the toolkit after the build returned, invisible to `check_stall`.
    fn check_busy(&mut self, sample: &Sample, raised: &mut Vec<Alert>) {
        let Some(cpu) = sample.cpu else {
            return;
        };
        if let Some(since) = self.busy_since {
            let stuck_frames = self.cpu_history.back().map_or(sample.frames, |e| e.1);
            if sample.frames == stuck_frames && sample.in_frame_for.is_none() {
                return;
            }
            self.busy_since = None;
            let longest = sample.at.saturating_sub(since);
            metrics::observe("ui.stall", longest);
            raised.push(Alert::new(
                AlertKind::UiStallRecovered,
                Severity::Warning,
                format!(
                    "UI was unresponsive for {} ms outside the frame build",
                    longest.as_millis()
                ),
            ));
        }
        // A frame in progress belongs to `check_stall`; a completed frame
        // restarts the window.
        if sample.in_frame_for.is_some()
            || self
                .cpu_history
                .back()
                .is_some_and(|e| e.1 != sample.frames)
        {
            self.cpu_history.clear();
        }
        if sample.in_frame_for.is_some() {
            return;
        }
        self.cpu_history.push_back((sample.at, sample.frames, cpu));
        while self.cpu_history.len() > 1
            && sample.at.saturating_sub(self.cpu_history[1].0) >= self.config.stall
        {
            self.cpu_history.pop_front();
        }
        let (start, _, start_cpu) = self.cpu_history[0];
        let wall = sample.at.saturating_sub(start);
        if wall < self.config.stall {
            return;
        }
        let busy = cpu.saturating_sub(start_cpu);
        if busy.as_secs_f64() >= wall.as_secs_f64() * BUSY_CPU_SHARE {
            self.busy_since = Some(start);
            raised.push(Alert::new(
                AlertKind::UiStall,
                Severity::Critical,
                format!(
                    "UI unresponsive for {} ms outside the frame build (UI thread at {:.0}% CPU, no frame completed)",
                    wall.as_millis(),
                    busy.as_secs_f64() / wall.as_secs_f64() * 100.0
                ),
            ));
        }
    }

    fn check_storm(&mut self, sample: &Sample, raised: &mut Vec<Alert>) {
        self.frame_history.push_back((sample.at, sample.frames));
        trim(&mut self.frame_history, sample.at, self.config.storm_window);
        let (start, start_frames) = self.frame_history[0];
        let span = sample.at.saturating_sub(start);
        if span < self.config.storm_window {
            return;
        }
        let fps = (sample.frames - start_frames) as f64 / span.as_secs_f64();
        metrics::gauge("ui.fps", fps as i64);
        if !self.storming && fps >= self.config.storm_fps as f64 {
            self.storming = true;
            raised.push(Alert::new(
                AlertKind::FrameStorm,
                Severity::Critical,
                format!(
                    "render loop spinning at {fps:.0} frames/s for {} s (lost compositor connection?)",
                    span.as_secs()
                ),
            ));
        } else if self.storming && fps < self.config.storm_fps as f64 / 2.0 {
            self.storming = false;
        }
    }

    fn check_errors(&mut self, sample: &Sample, raised: &mut Vec<Alert>) {
        self.error_history.push_back((sample.at, sample.errors));
        trim(&mut self.error_history, sample.at, self.config.error_window);
        let new_errors = sample.errors - self.error_history[0].1;
        if !self.bursting && new_errors >= self.config.error_burst {
            self.bursting = true;
            raised.push(Alert::new(
                AlertKind::ErrorBurst,
                Severity::Warning,
                format!(
                    "{new_errors} errors logged in the last {} s",
                    self.config.error_window.as_secs()
                ),
            ));
        } else if self.bursting && new_errors == 0 {
            self.bursting = false;
        }
    }
}

/// Observer of watchdog alerts (see [`Watchdog::start_with_hook`]).
pub type AlertHook = dyn Fn(&Alert) + Send + Sync;

fn raise(hook: Option<&AlertHook>, alert: Alert) {
    if let Some(hook) = hook {
        hook(&alert);
    }
    alerts::global().raise(alert);
}

/// The running watchdog thread; stops on drop.
pub struct Watchdog {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Watchdog {
    /// Sample the process heartbeat every `config.poll` and raise alerts on
    /// the global [`alerts::AlertLog`].
    #[must_use]
    pub fn start(config: Config) -> Watchdog {
        Watchdog::start_with_hook(config, None)
    }

    /// [`Watchdog::start`], also handing each raised alert to `hook` (on the
    /// watchdog thread, before it is logged). The hook must not block: the
    /// watchdog keeps sampling to observe the recovery.
    #[must_use]
    pub fn start_with_hook(config: Config, hook: Option<Arc<AlertHook>>) -> Watchdog {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("arca-watchdog".into())
            .spawn(move || {
                let poll = config.poll;
                // One monitor per UI thread, keyed by heartbeat identity.
                let mut monitors: HashMap<usize, (Weak<Heartbeat>, Monitor)> = HashMap::new();
                let started = Instant::now();
                while !thread_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(poll);
                    monitors.retain(|_, (weak, monitor)| {
                        let alive = weak.strong_count() > 0;
                        if !alive {
                            for alert in monitor.finish() {
                                raise(hook.as_deref(), alert);
                            }
                        }
                        alive
                    });
                    let errors = metrics::global().counter_value("log.errors");
                    for hb in live_heartbeats() {
                        let key = Arc::as_ptr(&hb) as usize;
                        let (_, monitor) = monitors
                            .entry(key)
                            .or_insert_with(|| (Arc::downgrade(&hb), Monitor::new(config.clone())));
                        let sample = Sample {
                            at: started.elapsed(),
                            in_frame_for: hb.in_frame_for(),
                            frames: hb.frames(),
                            errors,
                            operation: hb.operation(),
                            cpu: hb.cpu_time(),
                        };
                        for alert in monitor.observe(&sample) {
                            raise(hook.as_deref(), alert);
                        }
                    }
                }
            })
            .map_err(|error| log::warn!(target: "arca::watchdog", "watchdog disabled: {error}"))
            .ok();
        Watchdog { stop, handle }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            stall: Duration::from_millis(500),
            storm_fps: 400,
            storm_window: Duration::from_secs(2),
            error_burst: 5,
            error_window: Duration::from_secs(10),
            ..Config::default()
        }
    }

    fn sample(at_ms: u64, in_frame_ms: Option<u64>, frames: u64) -> Sample {
        Sample {
            at: Duration::from_millis(at_ms),
            in_frame_for: in_frame_ms.map(Duration::from_millis),
            frames,
            errors: 0,
            operation: Some("fs.read_dir"),
            cpu: None,
        }
    }

    fn kinds(alerts: &[Alert]) -> Vec<AlertKind> {
        alerts.iter().map(|a| a.kind).collect()
    }

    #[test]
    fn an_idle_window_never_stalls() {
        let mut monitor = Monitor::new(config());
        for second in 0..120 {
            assert!(monitor.observe(&sample(second * 1000, None, 10)).is_empty());
        }
    }

    #[test]
    fn a_long_frame_alerts_once_then_reports_recovery() {
        let mut monitor = Monitor::new(config());
        assert!(monitor.observe(&sample(0, Some(100), 7)).is_empty());
        let fired = monitor.observe(&sample(500, Some(600), 7));
        assert_eq!(kinds(&fired), [AlertKind::UiStall]);
        assert_eq!(fired[0].severity, Severity::Critical);
        assert!(fired[0].summary.contains("in fs.read_dir"), "{fired:?}");
        // Still stuck in the same frame: no repeat.
        assert!(monitor.observe(&sample(2000, Some(2100), 7)).is_empty());
        // The frame completed.
        let recovered = monitor.observe(&sample(2100, None, 8));
        assert_eq!(kinds(&recovered), [AlertKind::UiStallRecovered]);
        assert!(
            recovered[0].summary.contains("2100 ms"),
            "longest observed stall is reported: {recovered:?}"
        );
    }

    #[test]
    fn a_stall_open_when_the_thread_exits_still_reports_recovery() {
        let mut monitor = Monitor::new(config());
        assert!(
            monitor.finish().is_empty(),
            "nothing open, nothing to report"
        );
        monitor.observe(&sample(0, Some(700), 3));
        let finished = monitor.finish();
        assert_eq!(kinds(&finished), [AlertKind::UiStallRecovered]);
        assert!(finished[0].summary.contains("700 ms in fs.read_dir"));
        assert!(monitor.finish().is_empty(), "reported once");
    }

    #[test]
    fn back_to_back_stalled_frames_alert_separately() {
        let mut monitor = Monitor::new(config());
        assert_eq!(
            kinds(&monitor.observe(&sample(0, Some(900), 1))),
            [AlertKind::UiStall]
        );
        // Frame 1 finished and frame 2 is already stuck.
        assert_eq!(
            kinds(&monitor.observe(&sample(1000, Some(800), 2))),
            [AlertKind::UiStallRecovered, AlertKind::UiStall]
        );
    }

    #[test]
    fn a_sustained_frame_storm_alerts_and_rearms_after_calming() {
        let mut monitor = Monitor::new(config());
        let mut frames = 0;
        let mut storms = 0;
        // 1000 fps for 3 s.
        for tick in 0..=30 {
            frames += 100;
            storms += kinds(&monitor.observe(&sample(tick * 100, None, frames)))
                .iter()
                .filter(|k| **k == AlertKind::FrameStorm)
                .count();
        }
        assert_eq!(storms, 1, "one alert per storm");
        // Calm (60 fps) for 3 s, then storm again.
        for tick in 31..=60 {
            frames += 6;
            monitor.observe(&sample(tick * 100, None, frames));
        }
        let mut again = 0;
        for tick in 61..=90 {
            frames += 100;
            again += monitor.observe(&sample(tick * 100, None, frames)).len();
        }
        assert_eq!(again, 1, "the rule re-arms once the loop calms down");
    }

    #[test]
    fn normal_vsync_rates_are_not_a_storm() {
        let mut monitor = Monitor::new(config());
        let mut frames = 0;
        for tick in 0..100 {
            frames += 14; // 140 fps, a fast panel
            assert!(
                monitor
                    .observe(&sample(tick * 100, None, frames))
                    .is_empty()
            );
        }
    }

    #[test]
    fn an_error_burst_alerts_once_per_burst() {
        let mut monitor = Monitor::new(config());
        let at = |ms: u64, errors: u64| Sample {
            errors,
            ..sample(ms, None, 0)
        };
        assert!(monitor.observe(&at(0, 0)).is_empty());
        assert!(monitor.observe(&at(1000, 3)).is_empty());
        assert_eq!(
            kinds(&monitor.observe(&at(2000, 6))),
            [AlertKind::ErrorBurst]
        );
        assert!(monitor.observe(&at(3000, 9)).is_empty(), "already bursting");
    }

    fn cpu_sample(at_ms: u64, frames: u64, cpu_ms: u64) -> Sample {
        Sample {
            cpu: Some(Duration::from_millis(cpu_ms)),
            ..sample(at_ms, None, frames)
        }
    }

    #[test]
    fn a_thread_spinning_between_frames_stalls_then_recovers() {
        let mut monitor = Monitor::new(config());
        let mut fired = Vec::new();
        // Frames stop at 41; the thread keeps one core pinned for 1 s.
        for tick in 0..=10 {
            fired.extend(monitor.observe(&cpu_sample(tick * 100, 41, tick * 100)));
        }
        assert_eq!(kinds(&fired), [AlertKind::UiStall], "{fired:?}");
        assert!(
            fired[0].summary.contains("outside the frame build"),
            "{fired:?}"
        );
        // Still spinning: no repeat.
        assert!(monitor.observe(&cpu_sample(1500, 41, 1500)).is_empty());
        // A frame finally completes.
        let recovered = monitor.observe(&cpu_sample(1600, 42, 1550));
        assert_eq!(kinds(&recovered), [AlertKind::UiStallRecovered]);
        assert!(recovered[0].summary.contains("1600 ms"), "{recovered:?}");
    }

    #[test]
    fn an_idle_or_lightly_busy_thread_between_frames_is_not_a_stall() {
        let mut monitor = Monitor::new(config());
        // Idle: no CPU, no frames, for two minutes.
        for tick in 0..1200 {
            assert!(monitor.observe(&cpu_sample(tick * 100, 5, 20)).is_empty());
        }
        // Background-ish load at 50% of a core between frames.
        for tick in 0..100 {
            assert!(
                monitor
                    .observe(&cpu_sample(200_000 + tick * 100, 5, 20 + tick * 50))
                    .is_empty()
            );
        }
    }

    #[test]
    fn a_busy_thread_that_keeps_completing_frames_is_not_a_stall() {
        let mut monitor = Monitor::new(config());
        for tick in 0..100 {
            assert!(
                monitor
                    .observe(&cpu_sample(tick * 100, tick * 6, tick * 100))
                    .is_empty()
            );
        }
    }

    #[test]
    fn the_heartbeat_measures_its_threads_cpu_time() {
        if !std::path::Path::new("/proc/thread-self").exists() {
            return;
        }
        let hb = heartbeat();
        let before = hb.cpu_time().expect("schedstat readable");
        let spin = Instant::now();
        while spin.elapsed() < Duration::from_millis(30) {
            std::hint::black_box(0u64);
        }
        assert!(hb.cpu_time().unwrap() > before);
    }

    #[test]
    fn the_heartbeat_reports_frames_and_frame_duration() {
        let before = heartbeat().frames();
        {
            let _frame = FrameGuard::begin();
            assert!(heartbeat().in_frame_for().is_some());
        }
        assert!(heartbeat().frames() > before);
        assert!(metrics::global().histogram("ui.frame").is_some());
    }

    /// A busy UI thread completing frames must not mask a stall on another
    /// (the heartbeat used to be process-global, so any `frame_end` cleared
    /// every thread's in-progress frame).
    #[test]
    fn heartbeats_are_per_thread() {
        let busy = std::thread::spawn(|| {
            for _ in 0..100 {
                let _frame = FrameGuard::begin();
            }
            heartbeat().frames()
        });
        let stalled = std::thread::spawn(|| {
            frame_begin();
            super::super::span::mark_ui_thread();
            let _span = super::super::span("test.watchdog.isolated");
            std::thread::sleep(Duration::from_millis(30));
            let hb = heartbeat();
            (hb.in_frame_for(), hb.operation())
        });
        assert_eq!(busy.join().unwrap(), 100);
        let (in_frame, operation) = stalled.join().unwrap();
        assert!(in_frame.is_some_and(|d| d >= Duration::from_millis(30)));
        assert_eq!(operation, Some("test.watchdog.isolated"));
    }

    #[test]
    fn the_watchdog_thread_raises_a_stall_on_the_global_log() {
        let config = Config {
            stall: Duration::from_millis(50),
            poll: Duration::from_millis(10),
            ..Config::default()
        };
        let watchdog = Watchdog::start(config);
        frame_begin();
        let stalled = test_support::poll::wait_for(Duration::from_secs(5), || {
            alerts::global()
                .recent()
                .iter()
                .any(|a| a.kind == AlertKind::UiStall)
        });
        frame_end();
        drop(watchdog);
        assert!(stalled, "alerts: {:?}", alerts::global().recent());
    }
}
