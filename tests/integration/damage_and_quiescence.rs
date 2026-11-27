//! Integration tests verifying Optics regional damage localization,
//! OPTICS_UI_NODE_LEAVING departure damage, and animation quiescence.

use arca_engine::AppState;
use arca_engine::config::{Config, ViewMode};
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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
    std::fs::create_dir_all(&dir).unwrap();

    for name in ["alpha.txt", "beta.txt"] {
        std::fs::write(dir.join(name), b"test content").unwrap();
    }

    let state = AppState::with_config(
        dir.with_extension("conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, dir.with_extension("cache")).sync_navigation();
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    // Mirror the window's start hook: register the icon set on this context.
    app.start(&mut ui);
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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
}

#[test]
fn test_quick_look_modal_preview_damage_and_settling() {
    let dir = std::env::temp_dir().join(format!("arca-test-preview-dmg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sample.txt"), b"quick look content").unwrap();

    let state = AppState::with_config(
        dir.with_extension("conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, dir.with_extension("cache")).sync_navigation();
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    // Mirror the window's start hook: register the icon set on this context.
    app.start(&mut ui);
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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
}

/// Opens a 120-row List view at `viewport`, scrolls it by one wheel frame, and
/// returns that frame's damage rectangles. Asserts the list starts quiescent
/// and settles back to zero damage afterwards.
fn scroll_list_once(viewport_w: f32, viewport_h: f32) -> Vec<Rect> {
    let dir = std::env::temp_dir().join(format!(
        "arca-test-scroll-dmg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
    std::fs::create_dir_all(&dir).unwrap();
    for index in 0..120 {
        std::fs::write(dir.join(format!("file-{index:03}.txt")), b"row").unwrap();
    }

    let state = AppState::with_config(
        dir.with_extension("conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, dir.with_extension("cache")).sync_navigation();
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    // Mirror the window's start hook: register the icon set on this context.
    app.start(&mut ui);
    app.state.set_view_mode(ViewMode::List);

    let settled_input = Input::new((viewport_w, viewport_h), 1.0 / 60.0);
    for _ in 0..40 {
        run_frame(&mut app, &mut ui, &settled_input);
    }
    assert_eq!(query_damage(&mut ui).0, 0, "list must start quiescent");

    // One wheel frame is one viewport change. Its clip shift and moved rows
    // are damage even though no descendant's own state changed (ADR-0149).
    let mut wheel = Input::new((viewport_w, viewport_h), 1.0 / 60.0);
    wheel.set_cursor(600.0, viewport_h * 0.6);
    wheel.set_scroll(0.0, -2.0);
    run_frame(&mut app, &mut ui, &wheel);
    let mut buf = [Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }; 64];
    let n = ui.damage_rects(&mut buf);
    let rects = buf[..n].to_vec();

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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
    rects
}

#[test]
fn test_scrolling_virtualized_listing_damages_only_the_viewport_then_settles() {
    // Since Optics v0.0.87 a moved node reports both the pixels it painted
    // last frame and the ones it paints now, so every visible row costs about
    // two rectangles per element. A viewport this short keeps one wheel frame
    // inside the 64-rect contract, where the set must stay local rather than
    // widen to the window. (The full-height case is covered below.)
    let (viewport_w, viewport_h) = (1040.0, VIEWPORT_WITHIN_DAMAGE_BUDGET_H);
    let rects = scroll_list_once(viewport_w, viewport_h);
    let area: f32 = rects.iter().map(|r| r.w * r.h).sum();
    let full_viewport_area = viewport_w * viewport_h;
    assert!(
        !rects.is_empty(),
        "scrolling a virtualized viewport must emit damage"
    );
    assert!(
        !rects.iter().any(|r| r.x <= 0.0 && r.w >= viewport_w),
        "scrolling damage must not widen to the full window width: {rects:?}"
    );
    assert!(
        area > 0.0 && area < full_viewport_area,
        "scrolling damage must stay localized (area: {area:.0} vs {full_viewport_area:.0})"
    );
}

/// Height at which one List-view wheel frame fits Optics's 64-rect damage
/// contract with every moved row reported at its old and new position.
const VIEWPORT_WITHIN_DAMAGE_BUDGET_H: f32 = 320.0;

#[test]
fn test_scrolling_a_full_height_listing_past_the_damage_budget_reports_one_cover_then_settles() {
    // A full-height list moves more elements than the bounded rectangle list
    // can name. Optics then reports exactly one full-viewport cover instead of
    // a partial list that would leave stale pixels; the frame must still honour
    // the 64-rect contract and the list must settle afterwards.
    let (viewport_w, viewport_h) = (1040.0, 700.0);
    let rects = scroll_list_once(viewport_w, viewport_h);
    assert!(
        !rects.is_empty() && rects.len() <= 64,
        "scrolling must emit damage within the 64-rect contract: {} rects",
        rects.len()
    );
    let area: f32 = rects.iter().map(|r| r.w * r.h).sum();
    if area >= viewport_w * viewport_h {
        assert_eq!(
            rects,
            vec![Rect {
                x: 0.0,
                y: 0.0,
                w: viewport_w,
                h: viewport_h,
            }],
            "full-window scroll damage must be Optics's single overflow cover"
        );
    }
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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sample.txt"), b"sample").unwrap();

    let state = AppState::with_config(
        dir.with_extension("conf").to_str().unwrap().into(),
        Config::default(),
        dir.to_str().unwrap(),
    );
    let mut app =
        arca_ui::UiApp::with_thumb_cache(state, dir.with_extension("cache")).sync_navigation();
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    // Mirror the window's start hook: register the icon set on this context.
    app.start(&mut ui);
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
    let _ = std::fs::remove_file(dir.with_extension("conf"));
    let _ = std::fs::remove_dir_all(dir.with_extension("cache"));
}

#[test]
fn test_segmented_control_selection_animates_and_terminates() {
    let mut ui = optics_ui::Ui::headless().expect("headless ui");
    let mut selected_idx = 0usize;
    let input = Input::new((400.0, 60.0), 1.0 / 60.0);

    let render_seg = |ui: &mut optics_ui::Ui, idx: &mut usize| {
        ui.frame(&input, |f: &mut Frame| {
            f.size_next(400.0, 60.0);
            f.row()
                .with_opts(&LayoutOpts {
                    width: 400.0,
                    height: 60.0,
                    pad: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                })
                .show(|f| {
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
                });
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
