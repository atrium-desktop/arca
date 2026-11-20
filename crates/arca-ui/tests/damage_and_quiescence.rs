//! Integration tests verifying Optics regional damage localization,
//! OPTICS_UI_NODE_LEAVING departure damage, and animation quiescence.

use arca_engine::config::{Config, ViewMode};
use arca_engine::AppState;
use optics_ui::{Align, Frame, Input, LayoutOpts, Rect, SegmentedControl, SegmentedItem};

fn run_frame(app: &mut arca_ui::UiApp, ui: &mut optics_ui::Ui, input: &Input) {
    ui.frame(input, |f: &mut Frame| app.build(f, input));
    if let Ok(s) = ui.snapshot() {
        let _ = ui.activate(&s);
    }
}

fn query_damage(ui: &mut optics_ui::Ui) -> (usize, f32) {
    let mut buf = [Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }; 64];
    let n = ui.damage_rects(&mut buf);
    let total_area: f32 = buf[..n].iter().map(|r| r.w * r.h).sum();
    (n, total_area)
}

#[test]
fn test_trailing_row_removal_emits_regional_damage_and_settles() {
    let dir = std::env::temp_dir().join(format!("arca-test-dmg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    for name in ["alpha.txt", "beta.txt"] {
        std::fs::write(dir.join(name), b"test content").unwrap();
    }

    let state = AppState::with_config(
        dir.join("cfg/arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app = arca_ui::UiApp::new(state);
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    app.state.set_view_mode(ViewMode::List);

    let viewport_w = 1040.0;
    let viewport_h = 700.0;
    let full_viewport_area = viewport_w * viewport_h;
    let settled_input = Input::new((viewport_w, viewport_h), 1.0 / 60.0);

    // Warm up and settle initial listing
    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }

    let (settled_count, _) = query_damage(&mut ui);
    assert_eq!(
        settled_count, 0,
        "settled list view must produce zero damage rects"
    );
    assert!(
        !ui.anim_pending(),
        "settled list view must not hold pending animations"
    );

    // Delete trailing row from disk and trigger refresh
    std::fs::remove_file(dir.join("beta.txt")).unwrap();
    app.state.refresh();

    // The immediate next frame must report damage for the departed row (ADR-0144/ADR-0145)
    run_frame(&mut app, &mut ui, &settled_input);
    let (damage_count, damage_area) = query_damage(&mut ui);
    assert!(
        damage_count > 0,
        "removing a trailing row must emit departure damage for the vacated area"
    );
    assert!(
        damage_count <= 64,
        "damage rect count must satisfy the bounded 64-rect contract"
    );
    assert!(
        damage_area < full_viewport_area,
        "damage must be regionally localized, not a full-viewport invalidation (area: {damage_area:.0} vs {full_viewport_area:.0})"
    );

    // After subsequent frames, listing must settle back to quiescence (zero damage)
    for _ in 0..30 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    let (final_count, _) = query_damage(&mut ui);
    assert_eq!(
        final_count, 0,
        "listing must return to zero damage quiescence after row removal"
    );
    assert!(!ui.anim_pending());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_quick_look_modal_preview_damage_and_settling() {
    let dir = std::env::temp_dir().join(format!("arca-test-preview-dmg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sample.txt"), b"quick look content").unwrap();

    let state = AppState::with_config(
        dir.join("cfg/arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app = arca_ui::UiApp::new(state);
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let settled_input = Input::new((1040.0, 700.0), 1.0 / 60.0);

    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    let (count, _) = query_damage(&mut ui);
    assert_eq!(count, 0, "quiescent list before preview");

    // Select the file and trigger Space to open preview
    app.state.select_visible(0);
    let mut space_input = Input::new((1040.0, 700.0), 1.0 / 60.0);
    space_input.push_key(' ' as i32, true, false);
    run_frame(&mut app, &mut ui, &space_input);

    // Advance frames during spring opening animation
    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }

    // Modal preview spring must settle and enter quiescence
    let (open_settled_count, _) = query_damage(&mut ui);
    assert_eq!(
        open_settled_count, 0,
        "opened preview modal must settle to zero damage"
    );
    assert!(
        !ui.anim_pending(),
        "preview spring animation must terminate"
    );

    // Dismiss preview with Escape
    let mut esc_input = Input::new((1040.0, 700.0), 1.0 / 60.0);
    esc_input.push_key(optics_ui::key::ESCAPE, true, false);
    run_frame(&mut app, &mut ui, &esc_input);

    // Advance frames during closing animation
    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }

    let (closed_settled_count, _) = query_damage(&mut ui);
    assert_eq!(
        closed_settled_count, 0,
        "dismissed preview must return to zero damage quiescence"
    );
    assert!(!ui.anim_pending());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_scrolling_virtualized_listing_damages_only_the_viewport_then_settles() {
    let dir = std::env::temp_dir().join(format!(
        "arca-test-scroll-dmg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for index in 0..120 {
        std::fs::write(dir.join(format!("file-{index:03}.txt")), b"row").unwrap();
    }

    let state = AppState::with_config(
        dir.join("cfg/arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app = arca_ui::UiApp::new(state);
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    app.state.set_view_mode(ViewMode::List);

    let viewport_w = 1040.0;
    let viewport_h = 700.0;
    let full_viewport_area = viewport_w * viewport_h;
    let settled_input = Input::new((viewport_w, viewport_h), 1.0 / 60.0);
    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    assert_eq!(query_damage(&mut ui).0, 0, "list must start quiescent");

    // One wheel frame is one viewport change. Its clip shift and restacked
    // rows are damage even though no descendant's own state changed
    // (ADR-0149), and the set stays local rather than widening the window.
    let mut wheel = Input::new((viewport_w, viewport_h), 1.0 / 60.0);
    wheel.set_cursor(600.0, 420.0);
    wheel.set_scroll(0.0, -2.0);
    run_frame(&mut app, &mut ui, &wheel);
    let (scroll_rects, scroll_area) = query_damage(&mut ui);
    assert!(
        scroll_rects > 0,
        "scrolling a virtualized viewport must emit damage"
    );
    assert!(
        scroll_rects <= 64,
        "scrolling must honour the 64-rect contract"
    );
    assert!(
        scroll_area > 0.0 && scroll_area < full_viewport_area,
        "scrolling damage must stay localized (area: {scroll_area:.0} vs {full_viewport_area:.0})"
    );

    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    assert_eq!(
        query_damage(&mut ui).0,
        0,
        "a scrolled list must settle back to zero damage"
    );
    assert!(!ui.anim_pending());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_focused_location_caret_reports_only_phase_flips() {
    let dir = std::env::temp_dir().join(format!(
        "arca-test-caret-dmg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sample.txt"), b"sample").unwrap();

    let state = AppState::with_config(
        dir.join("cfg/arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app = arca_ui::UiApp::new(state);
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let viewport_w = 1040.0;
    let viewport_h = 700.0;
    let full_viewport_area = viewport_w * viewport_h;
    let settled_input = Input::new((viewport_w, viewport_h), 1.0 / 60.0);

    for _ in 0..30 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    assert_eq!(query_damage(&mut ui).0, 0, "list must start quiescent");

    // Ctrl+L focuses the location field and selects all of its text.
    let mut focus_input = Input::new((1040.0, 700.0), 1.0 / 60.0);
    focus_input.set_mods(optics_ui::mods::CTRL);
    focus_input.push_key('l' as i32, true, false);
    run_frame(&mut app, &mut ui, &focus_input);
    for _ in 0..20 {
        run_frame(&mut app, &mut ui, &settled_input);
        if query_damage(&mut ui).0 == 0 {
            break;
        }
    }
    assert_eq!(
        query_damage(&mut ui).0,
        0,
        "the focused location field must not damage every frame"
    );
    assert!(!ui.anim_pending());

    assert!(
        ui.caret_clock_live(),
        "a focused widget-owned caret must run optics-ui's blink clock"
    );

    // A blink is a phase edge (ADR-0151): within 0.75 s one or two flips are
    // pending, and only those frames report damage. The other frames stay at
    // zero, so a focused field can sleep between blinks.
    let mut flipped_frames = 0;
    let mut damaged_frames = 0;
    let mut flipped_rects = 0;
    let mut max_flip_area = 0.0f32;
    for _ in 0..45 {
        ui.frame(&settled_input, |frame| app.build(frame, &settled_input));
        let pending = ui.caret_flip_pending();
        let (count, area) = query_damage(&mut ui);
        if pending {
            flipped_frames += 1;
        }
        if count > 0 {
            damaged_frames += 1;
            flipped_rects += count;
            max_flip_area = max_flip_area.max(area);
        }
        if let Ok(snapshot) = ui.snapshot() {
            let _ = ui.activate(&snapshot);
        }
    }
    assert_eq!(
        damaged_frames, flipped_frames,
        "only caret phase flips may damage a focused field"
    );
    assert!(
        (1..=2).contains(&flipped_frames),
        "a 0.75 s window must contain one or two caret phase flips, saw {flipped_frames}"
    );
    assert!(
        flipped_rects <= 4,
        "a blink may touch only its field, saw {flipped_rects} rectangles"
    );
    assert!(
        max_flip_area < full_viewport_area,
        "caret damage must stay bounded to the focused field, saw {max_flip_area:.0} px"
    );
    assert!(
        !ui.anim_pending(),
        "caret blink is a discrete phase, not a pending animation"
    );

    for _ in 0..4 {
        run_frame(&mut app, &mut ui, &settled_input);
        assert_eq!(
            query_damage(&mut ui).0,
            0,
            "a focused field must return to zero damage between flips"
        );
    }

    // Reduced motion is host policy: Arca disables the blink instead of
    // letting a timer wake the window, then restores the library default.
    ui.set_reduced_motion(true);
    run_frame(&mut app, &mut ui, &settled_input);
    run_frame(&mut app, &mut ui, &settled_input);
    assert!(
        !ui.caret_clock_live(),
        "reduced motion must stop the caret clock"
    );
    assert!(ui.caret_visible(), "the caret stays solid, never hidden");
    for _ in 0..20 {
        run_frame(&mut app, &mut ui, &settled_input);
        assert_eq!(
            query_damage(&mut ui).0,
            0,
            "a solid caret under reduced motion must produce no damage"
        );
    }

    ui.set_reduced_motion(false);
    run_frame(&mut app, &mut ui, &settled_input);
    run_frame(&mut app, &mut ui, &settled_input);
    assert!(
        ui.caret_clock_live(),
        "leaving reduced motion must restore the default blink period"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_segmented_control_selection_animates_and_terminates() {
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let mut selected_idx = 0usize;
    let input = Input::new((400.0, 60.0), 1.0 / 60.0);

    let render_seg = |ui: &mut optics_ui::Ui, idx: &mut usize| {
        ui.frame(&input, |f: &mut Frame| {
            f.size_next(400.0, 60.0);
            f.row_ex(
                &LayoutOpts {
                    width: 400.0,
                    height: 60.0,
                    pad: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |f| {
                    let items = [
                        SegmentedItem::text("Grid"),
                        SegmentedItem::text("List"),
                        SegmentedItem::text("Columns"),
                    ];
                    let control = SegmentedControl::new("##test-seg")
                        .height(30.0)
                        .min_item_width(32.0)
                        .compact(true);
                    let _ = control.show(f, &items, idx);
                },
            );
        });
        if let Ok(s) = ui.snapshot() {
            let _ = ui.activate(&s);
        }
    };

    // Settle initial state
    for _ in 0..30 {
        render_seg(&mut ui, &mut selected_idx);
    }
    let (settled_count, _) = query_damage(&mut ui);
    assert_eq!(settled_count, 0);
    assert!(!ui.anim_pending());

    // Switch selection index
    selected_idx = 1;
    render_seg(&mut ui, &mut selected_idx);

    // Initial transition frame should produce damage
    let (active_count, _) = query_damage(&mut ui);
    assert!(
        active_count > 0,
        "changing segmented control selection must emit damage"
    );

    // Under Optics ADR-0152, easing must terminate within a bounded number of frames
    // (values snap to target rather than drifting with float epsilon forever).
    for _ in 0..50 {
        render_seg(&mut ui, &mut selected_idx);
    }

    assert!(
        !ui.anim_pending(),
        "segmented control animation must cleanly terminate (ADR-0152 ease termination)"
    );
    let (final_count, _) = query_damage(&mut ui);
    assert_eq!(
        final_count, 0,
        "settled segmented control must produce zero damage"
    );
}
