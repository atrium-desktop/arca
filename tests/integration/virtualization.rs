//! Viewport virtualization invariants against the public UI surface.
//!
//! optics-ui has a 1 MiB per-frame arena: draw calls past the limit are dropped
//! silently, which blanks icons and labels. Every listing view must therefore
//! build only the rows intersecting the viewport. This used to be an internal
//! unit test; keeping it in the root suite exercises the same guarantee through
//! the public `UiApp` constructor an embedder uses, and lets CI name it as an
//! architectural invariant.

use arca_engine::config::{Config, ViewMode};
use arca_engine::AppState;
use test_support::fs::TestDir;

#[test]
fn large_directory_does_not_overflow_frame_arena_in_any_view() {
    let dir = TestDir::new("virtualization");
    for index in 0..2_500 {
        dir.write(format!("file-{index:04}.txt"), b"x");
    }
    let config = dir.path().with_extension("conf");
    let state = AppState::with_config(
        config.to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, config.with_extension("cache")).sync_navigation();
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let input = optics_ui::Input::new((1280.0, 760.0), 1.0 / 60.0);

    for mode in [ViewMode::Grid, ViewMode::List, ViewMode::Miller] {
        app.state.set_view_mode(mode);
        for frame in 0..3 {
            ui.frame(&input, |frame_ref| app.build(frame_ref, &input));
            assert!(
                !ui.overflowed(),
                "{mode:?} overflowed the optics-ui arena on frame {frame}"
            );
        }
    }
    let _ = std::fs::remove_file(&config);
    let _ = std::fs::remove_dir_all(config.with_extension("cache"));
}

#[test]
fn scrolled_large_listing_builds_only_visible_rows() {
    let dir = TestDir::new("virtualization-scroll");
    for index in 0..1_000 {
        dir.write(format!("entry-{index:04}.txt"), b"row");
    }
    let config = dir.path().with_extension("conf");
    let state = AppState::with_config(
        config.to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, config.with_extension("cache")).sync_navigation();
    app.state.set_view_mode(ViewMode::List);
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
    for _ in 0..3 {
        ui.frame(&settle, |frame| app.build(frame, &settle));
    }

    // Sixty wheel notches travel far past the first viewport. A non-virtualized
    // implementation would build 1000 rows and overflow; the virtualized one
    // restacks a bounded window and stays under the arena.
    for _ in 0..60 {
        let mut wheel = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        wheel.set_cursor(600.0, 420.0);
        wheel.set_scroll(0.0, -3.0);
        ui.frame(&wheel, |frame| app.build(frame, &wheel));
    }
    assert!(
        !ui.overflowed(),
        "scrolling a 1000-entry list must remain virtualized"
    );
    let _ = std::fs::remove_file(&config);
    let _ = std::fs::remove_dir_all(config.with_extension("cache"));
}
