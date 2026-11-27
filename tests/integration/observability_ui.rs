//! Observability through the headless UI (native).
//!
//! These journeys drive real `UiApp` frames, so they prove the UI layer is
//! wired to telemetry — not just that the engine emits it.

use arca_engine::config::ViewMode;
use arca_engine::telemetry::{span, watchdog};

use crate::common::ui::{FIRST_LIST_ROW, Harness};

/// `[INV-UI-04]` at the UI level: double-clicking into a folder and going
/// back with Backspace never reads a directory on the UI thread in List view.
/// This is the class of work that froze the window under rapid clicking.
#[test]
fn interactive_navigation_keeps_blocking_reads_off_the_ui_thread() {
    let mut harness = Harness::new("obs-ui-nav");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);
    harness.enable_async_navigation();
    assert!(span::is_ui_thread(), "building a frame marks the UI thread");
    let before = span::ui_blocking_io_count();

    harness.double_click(FIRST_LIST_ROW.0, FIRST_LIST_ROW.1);
    harness.settle_navigation();
    assert!(harness.app.state.cwd().ends_with("/sub"));
    harness.press(optics_ui::key::BACKSPACE, 0);
    harness.settle_navigation();
    assert_eq!(harness.app.state.cwd(), harness.dir.str());

    assert_eq!(
        span::ui_blocking_io_count(),
        before,
        "a directory was read synchronously inside a frame"
    );
}

/// Every frame the harness builds is a heartbeat the watchdog can see, and a
/// settled window reads as idle rather than stalled.
#[test]
fn settled_frames_leave_the_heartbeat_idle() {
    let mut harness = Harness::new("obs-ui-heartbeat");
    let hb = watchdog::heartbeat();
    let before = hb.frames();
    harness.settle_for(3);
    assert!(hb.frames() >= before + 3);
    assert!(
        hb.in_frame_for().is_none(),
        "between frames the UI thread is idle, never 'stalled'"
    );
}
