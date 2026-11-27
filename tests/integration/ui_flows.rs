//! External headless interaction flows.
//!
//! The crate-local unit tests constrain individual widgets and app methods.
//! These tests drive complete user journeys through the public `UiApp` surface
//! (no private fields, no crate-internal harness) so a regression in the
//! composition of chrome, engine state, and optics-ui input handling shows up
//! before a release.

use arca_engine::SortKey;
use arca_engine::config::ViewMode;

use crate::common::ui::{FIRST_LIST_ROW, Harness, LIST_ROW_PITCH};

#[test]
fn every_view_builds_and_settles() {
    let mut harness = Harness::new("views");
    harness.settle_for(2);
    for mode in [ViewMode::Grid, ViewMode::List, ViewMode::Miller] {
        harness.set_view(mode);
        harness.settle_for(3);
        assert!(
            !harness.ui.overflowed(),
            "{mode:?} must build within the frame arena"
        );
        assert_eq!(harness.app.state.entries().len(), 2);
    }
}

#[test]
fn hidden_toggle_filter_and_sort_are_driven_by_the_public_surface() {
    let mut harness = Harness::new("keys");
    harness.settle_for(2);
    assert_eq!(harness.app.state.entries().len(), 2);

    harness.press('h' as i32, optics_ui::mods::CTRL);
    harness.settle();
    assert!(harness.app.state.show_hidden);
    assert_eq!(harness.app.state.entries().len(), 3);

    harness.app.state.set_filter("a.txt".into());
    harness.settle();
    let visible = harness.app.state.visible();
    assert_eq!(visible.len(), 1);
    assert_eq!(
        harness.app.state.entries()[visible[0]].name,
        "a.txt",
        "the text filter narrows the listing"
    );

    harness.app.state.set_filter(String::new());
    harness.app.state.toggle_sort(SortKey::Size);
    harness.settle();
    assert!(!harness.ui.overflowed());
}

#[test]
fn enter_and_backspace_traverse_directories() {
    let mut harness = Harness::new("traverse");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);
    harness.app.state.select_visible(0);
    assert_eq!(
        harness
            .app
            .state
            .selected_entry()
            .map(|entry| entry.name.as_str()),
        Some("sub")
    );

    harness.press(optics_ui::key::RETURN, 0);
    assert!(harness.app.state.cwd().ends_with("/sub"));
    harness.settle();

    harness.press(optics_ui::key::BACKSPACE, 0);
    assert_eq!(harness.app.state.cwd(), harness.dir.str());
    assert!(!harness.ui.overflowed());
}

#[test]
fn double_click_opens_a_directory() {
    let mut harness = Harness::new("double-click");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);

    harness.double_click(FIRST_LIST_ROW.0, FIRST_LIST_ROW.1);
    assert!(
        harness.app.state.cwd().ends_with("/sub"),
        "double-clicking the first row (a directory) navigates into it"
    );
    harness.settle_for(2);
    assert!(!harness.ui.overflowed());
}

#[test]
fn asynchronous_navigation_never_blocks_the_frame_and_publishes() {
    let mut harness = Harness::new("async-nav");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);
    harness.enable_async_navigation();

    harness.double_click(FIRST_LIST_ROW.0, FIRST_LIST_ROW.1);
    harness.settle_navigation();
    assert!(
        harness.app.state.cwd().ends_with("/sub"),
        "the worker publishes the directory listing"
    );
    assert!(!harness.app.state.is_navigating());

    harness.press(optics_ui::key::BACKSPACE, 0);
    harness.settle_navigation();
    assert_eq!(harness.app.state.cwd(), harness.dir.str());
    assert!(!harness.ui.overflowed());
}

#[test]
fn right_click_selects_and_opens_a_stable_menu() {
    let mut harness = Harness::new("context-menu");
    harness.set_view(ViewMode::List);
    harness.settle_for(2);

    // Row 1 is `a.txt` (directories sort first).
    let y = FIRST_LIST_ROW.1 + LIST_ROW_PITCH;
    harness.right_click(FIRST_LIST_ROW.0, y);
    assert_eq!(
        harness
            .app
            .state
            .selected_entry()
            .map(|entry| entry.name.as_str()),
        Some("a.txt"),
        "right-click moves the selection to the row under the pointer"
    );

    // Repeated right-clicks and quiet frames must not accumulate overlays or
    // leak arena memory. A stale "open with" snapshot used to reorder items
    // between frames.
    for _ in 0..5 {
        harness.settle();
    }
    harness.right_click(FIRST_LIST_ROW.0, y);
    harness.settle_for(5);
    assert!(!harness.ui.overflowed());
}

#[test]
fn settings_popup_toggles_without_desynchronising_frames() {
    let mut harness = Harness::new("settings");
    harness.settle_for(2);

    // The gear is the right-most toolbar icon (toolbar starts below the 38 px
    // tab strip; see `common::ui::TOOLBAR_GEAR`).
    for _ in 0..4 {
        harness.click(
            crate::common::ui::TOOLBAR_GEAR.0,
            crate::common::ui::TOOLBAR_GEAR.1,
        );
        harness.settle();
    }
    assert!(
        !harness.ui.overflowed(),
        "repeated open/close cycles must not leak transient nodes"
    );
}
