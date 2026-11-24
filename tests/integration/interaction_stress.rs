//! Repetitive-interaction stress guards.
//!
//! The product symptom this suite targets is "clicking repeatedly makes the
//! window stop responding". A UI-thread stall or an unbounded per-click
//! allocation shows up as a wall-clock blow-up or an arena overflow here,
//! without needing a compositor or GPU. The thresholds are deliberately
//! generous: they catch pathological behaviour, not scheduler noise.

use std::time::{Duration, Instant};

use arca_engine::config::ViewMode;

use crate::common::ui::{Harness, FIRST_LIST_ROW, LIST_ROW_PITCH, TOOLBAR_GEAR};

#[test]
fn hundreds_of_row_clicks_are_bounded_and_keep_one_selection() {
    let mut harness = Harness::at("click-stress");
    for index in 0..20 {
        harness.dir.write(format!("item-{index:02}.txt"), b"x");
    }
    harness.app.state.refresh();
    harness.set_view(ViewMode::List);
    harness.settle_for(2);

    let start = Instant::now();
    // Alternate rows so consecutive clicks are not a double-click: clicking
    // the same row twice is a legitimate open action that spawns an
    // application, not the selection path under test here.
    for step in 0..400 {
        let row = step % 10;
        let y = FIRST_LIST_ROW.1 + row as f32 * LIST_ROW_PITCH;
        harness.click(FIRST_LIST_ROW.0, y);
    }
    harness.settle_for(5);

    assert!(
        start.elapsed() < Duration::from_secs(20),
        "400 clicks took {:?}; a per-click stall is the freeze symptom",
        start.elapsed()
    );
    assert!(
        !harness.ui.overflowed(),
        "repeated clicks must not exhaust the 1 MiB frame arena"
    );
    assert_eq!(
        harness.app.state.selected_count(),
        1,
        "plain clicks leave exactly one selected row"
    );
    assert_eq!(harness.app.state.entries().len(), 20);
}

#[test]
fn repeated_settings_and_view_changes_return_to_quiescence() {
    let mut harness = Harness::new("chrome-stress");
    harness.settle_for(2);

    let start = Instant::now();
    for step in 0..40 {
        harness.click(TOOLBAR_GEAR.0, TOOLBAR_GEAR.1);
        harness.settle();
        let mode = [ViewMode::Grid, ViewMode::List, ViewMode::Miller][step % 3];
        harness.set_view(mode);
        harness.settle();
    }
    // Let any segmented-control / popup easing finish.
    harness.settle_for(60);

    assert!(
        start.elapsed() < Duration::from_secs(20),
        "repeated chrome interaction took {:?}",
        start.elapsed()
    );
    assert!(!harness.ui.overflowed());
    assert!(
        !harness.ui.anim_pending(),
        "the chrome must settle after the last interaction"
    );
}

#[test]
fn rapid_directory_double_clicks_are_bounded() {
    let mut harness = Harness::new("navigate-stress");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);

    let start = Instant::now();
    for _ in 0..30 {
        harness.double_click(FIRST_LIST_ROW.0, FIRST_LIST_ROW.1);
        assert!(
            harness.app.state.cwd().ends_with("/sub"),
            "double-clicking a directory navigates into it"
        );
        harness.press(optics_ui::key::BACKSPACE, 0);
        assert_eq!(harness.app.state.cwd(), harness.dir.str());
    }
    harness.settle_for(5);

    assert!(
        start.elapsed() < Duration::from_secs(20),
        "30 navigate round-trips took {:?}",
        start.elapsed()
    );
    assert!(!harness.ui.overflowed());
}
