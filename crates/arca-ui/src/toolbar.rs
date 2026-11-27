//! Primary command bar: navigation, location/search and view controls.

use arca_engine::ViewMode;
use optics_ui::{Align, Frame, LayoutOpts, Rect, SegmentedControl, SegmentedItem, TextEditOpts};

use crate::app::{FocusTarget, UiApp, focus_any};
use crate::icons::ids;
use crate::theme::Tones;

pub(crate) fn build_toolbar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let icons = app.icons;
    frame.size_next(0.0, 48.0);
    frame
        .row()
        .with_opts(&LayoutOpts {
            height: 48.0,
            gap: 6.0,
            pad: 8.0,
            cross: Align::Center,
            bg: tones.toolbar,
            ..Default::default()
        })
        .show(|frame| {
            if icons.button(frame, ids::ArrowLeft, 30.0) {
                app.go_back();
            }
            if icons.button(frame, ids::ArrowRight, 30.0) {
                app.go_forward();
            }
            if icons.button(frame, ids::ArrowUp, 30.0) {
                app.go_up();
            }
            if icons.button(frame, ids::RefreshCw, 30.0) {
                app.state.refresh();
            }

            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            let cwd = app.state.cwd().to_string();
            if !app.location_focused
                && !app.location_focused_now
                && app.pending_focus != Some(FocusTarget::Location)
                && app.location.as_str() != cwd
            {
                app.location.set(&cwd);
            }

            let prev_theme = frame.theme();
            let input_theme = prev_theme
                .with_bg(tones.input_bg)
                .with_surface_sunken(tones.input_bg)
                .with_border(optics_ui::Color::TRANSPARENT)
                .with_border_width(1.0)
                .with_corner_radius(8.0)
                .with_padding(10.0);

            icons.icon(frame, ids::Folder, 16.0);
            frame.flex(1.0);
            frame.size_next(0.0, 32.0);
            frame.set_theme(input_theme);
            let loc_resp = frame.textedit(
                "##location",
                &mut app.location,
                &TextEditOpts {
                    accept_tab: true,
                    select_all_on_focus: true,
                    submit_on_enter: true,
                    ..Default::default()
                },
            );
            app.location_focused = loc_resp.focused;
            app.location_focused_now = loc_resp.focused;
            if app.pending_focus == Some(FocusTarget::Location) {
                frame.textedit_set_selection("##location", 0, u32::MAX);
                if focus_any(frame, &["##location"]) {
                    app.pending_focus = None;
                }
            }
            frame.set_theme(prev_theme);
            if loc_resp.changed {
                app.location_completion = None;
            }
            if loc_resp.submitted {
                app.submit_location(frame);
            } else if loc_resp.tabbed {
                app.complete_location(frame);
            }

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            icons.icon(frame, ids::Search, 16.0);
            frame.size_next(180.0, 32.0);
            frame.set_theme(input_theme);
            let changed = frame
                .textedit(
                    "##filter",
                    &mut app.filter,
                    &TextEditOpts {
                        placeholder: Some("Search / Filter..."),
                        ..Default::default()
                    },
                )
                .changed;
            capture_focus(frame, &mut app.filter_focused_now);
            if app.pending_focus == Some(FocusTarget::Filter) && focus_any(frame, &["##filter"]) {
                app.pending_focus = None;
            }
            frame.set_theme(prev_theme);
            if changed {
                app.state.set_filter(app.filter.as_str().into_owned());
            } else if !app.filter_focused_now && app.filter.as_str() != app.state.filter() {
                app.filter.set(app.state.filter());
            }

            frame.size_next(8.0, 0.0);
            frame.spacer(8.0);

            // Standard SegmentedControl for View Mode
            let mut current_idx = match app.state.view_mode {
                ViewMode::Grid => 0,
                ViewMode::List => 1,
                ViewMode::Miller => 2,
            };

            let items = [
                SegmentedItem::icon(icons.id(ids::Grid)),
                SegmentedItem::icon(icons.id(ids::List)),
                SegmentedItem::icon(icons.id(ids::Columns)),
            ];

            let control = SegmentedControl::new("##view-mode-seg")
                .height(30.0)
                .min_item_width(32.0)
                .compact(true);

            let seg_theme = prev_theme.with_border_width(0.0);
            frame.set_theme(seg_theme);
            if control.show(frame, &items, &mut current_idx) {
                let new_mode = match current_idx {
                    0 => ViewMode::Grid,
                    1 => ViewMode::List,
                    _ => ViewMode::Miller,
                };
                app.state.set_view_mode(new_mode);
            }
            frame.set_theme(prev_theme);

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            if icons.toggle_button(frame, ids::EyeOff, ids::Eye, 30.0, app.state.show_hidden) {
                app.state.toggle_hidden();
            }

            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            if icons.button(frame, ids::Settings, 30.0) {
                let r = frame.response();
                let anchor = Rect {
                    x: r.rect.x,
                    y: r.rect.y,
                    w: r.rect.w,
                    h: r.rect.h,
                };
                app.toggle_settings_menu(anchor);
            }
        });
}

fn capture_focus(frame: &mut Frame, focused: &mut bool) {
    *focused = frame.response().focused;
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use super::*;
    use crate::test_support::{fixture, frame};

    /// The window close button sits at the right edge of the 42px tab
    /// strip; `optics_window_close` outside an active app run is a no-op.
    #[test]
    fn window_close_button_is_clickable() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        let close = (1019.0, 21.0);
        let mut press = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(close.0, close.1);
        press.set_mouse_down(optics_ui::MouseButton::Left, true);
        press.set_mouse_pressed(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);
        let mut release = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(close.0, close.1);
        release.set_mouse_released(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    /// The tab strip drag region fills the space between the last tab and the close button;
    /// pressing it triggers `optics_window_start_move` (a safe no-op headless).
    #[test]
    fn titlebar_drag_region_is_interactive() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        // Middle of the top bar in the drag region
        let drag_point = (500.0, 21.0);
        let mut press = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(drag_point.0, drag_point.1);
        press.set_mouse_down(optics_ui::MouseButton::Left, true);
        press.set_mouse_pressed(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);
        let mut release = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(drag_point.0, drag_point.1);
        release.set_mouse_released(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    /// Hovering either toolbar text field shows the I-beam in a full app
    /// frame. The listing, sidebar and sort headers are built after the
    /// toolbar and restore the arrow cursor over their own items; that
    /// override must not reach widgets the pointer is not over.
    #[test]
    fn toolbar_text_fields_show_text_cursor_in_full_frame() {
        let (_dir, mut app) = fixture();
        assert!(
            !app.state.entries().is_empty(),
            "fixture must populate the listing so content rows follow the toolbar"
        );
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        // Both fields sit in the 54px toolbar below the 42px tab strip. The
        // location field flexes across the middle; the 180px filter field
        // follows it, left of the view-mode and trailing toolbar buttons.
        for (name, point) in [("location", (400.0, 69.0)), ("filter", (700.0, 69.0))] {
            let mut hover = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
            hover.set_cursor(point.0, point.1);
            frame(&mut app, &mut ui, &hover);
            assert_eq!(
                ui.cursor_hint(),
                optics_ui::CursorHint::Text,
                "hovering the {name} field must show the text cursor"
            );
        }
    }

    #[test]
    fn settings_button_toggles_appearance_popup() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.settings_menu.is_none());

        // The settings button is the right-most 32px icon in the 54px
        // toolbar, which sits below the 42px tab strip.
        let gear = (1016.0, 69.0);
        let click = |app: &mut UiApp, ui: &mut optics_ui::Ui| {
            let mut press = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(gear.0, gear.1);
            press.set_mouse_down(optics_ui::MouseButton::Left, true);
            press.set_mouse_pressed(optics_ui::MouseButton::Left, true);
            frame(app, ui, &press);
            let mut release = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(gear.0, gear.1);
            release.set_mouse_released(optics_ui::MouseButton::Left, true);
            frame(app, ui, &release);
        };

        click(&mut app, &mut ui);
        assert!(app.settings_menu.is_some(), "gear click opens the popup");
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(
            app.settings_menu.is_some(),
            "popup stays open across quiet frames"
        );

        click(&mut app, &mut ui);
        assert!(app.settings_menu.is_none(), "second gear click closes it");
    }

    /// Blurring either toolbar input (transparent resting border, 1px accent
    /// focus border) must schedule a follow-up frame when focus transitions
    /// mid/post-build and damage the outer halo outside its 32px layout box so
    /// a regional repaint erases the focus border completely.
    #[test]
    fn blurring_toolbar_inputs_damages_outer_focus_border() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..3 {
            frame(&mut app, &mut ui, &settle);
        }

        // 1. Focus `##location` via Ctrl+L; programmatic focus after build must
        // request a follow-up frame so the border renders without waiting for
        // key release.
        let mut ctrl_l = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        ctrl_l.set_mods(optics_ui::mods::CTRL);
        ctrl_l.push_key('l' as i32, true, false);
        frame(&mut app, &mut ui, &ctrl_l);
        assert!(
            ui.anim_pending(),
            "Ctrl+L focus change must request a follow-up frame"
        );
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.location_focused, "location field must be focused");

        // 2. Click `##filter` at (700, 69) to switch focus from `##location` to
        // `##filter`: the press frame must request a follow-up frame so
        // `##location` un-highlights immediately even before mouse release.
        let mut click_filter = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        click_filter.set_cursor(700.0, 69.0);
        click_filter.set_mouse_down(optics_ui::MouseButton::Left, true);
        click_filter.set_mouse_pressed(optics_ui::MouseButton::Left, true);
        frame(&mut app, &mut ui, &click_filter);
        assert!(
            ui.anim_pending(),
            "switching focus to filter must request a follow-up frame"
        );

        // Next frame: `##location` builds unfocused (border removed). Capture
        // damage BEFORE activating, matching the Wayland host lifecycle.
        ui.frame(&settle, |f| app.build(f, &settle));
        assert!(!app.location_focused, "location field must lose focus");
        assert!(app.filter_focused_now, "filter field must gain focus");

        let mut rects = [Rect::default(); 32];
        let count = ui.damage_rects(&mut rects);
        assert!(count >= 1, "blurring location field must emit damage");

        // The 32px location box sits centered in the 48px toolbar below the
        // 38px tab strip (y = 46..78, spanning across x = 400). The outer half
        // of its 1px focus border + AA fringe extends beyond 46..78 (h > 32)
        // and must be covered so regional repaint erases the halo completely.
        let covers_loc_halo = rects[..count].iter().any(|r| {
            r.x <= 400.0 && r.x + r.w >= 400.0 && r.y <= 45.0 && r.y + r.h >= 79.0 && r.h > 32.0
        });
        assert!(
            covers_loc_halo,
            "location blur damage {:?} must cover outer border halo (y <= 45..>= 79)",
            &rects[..count]
        );
        if let Ok(snap) = ui.snapshot() {
            let _ = ui.activate(&snap);
        }
        frame(&mut app, &mut ui, &settle);
        assert!(app.filter_focused, "filter field must remain focused");

        // 3. Dismiss `##filter` via Escape: `handle_shortcuts` clears focus
        // before `build_toolbar`, so `##filter` builds unfocused on the Escape
        // frame itself and must emit damage covering its outer focus halo.
        let mut esc = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        ui.frame(&esc, |f| app.build(f, &esc));
        assert!(!app.filter_focused_now, "filter field must lose focus");
        let count = ui.damage_rects(&mut rects);
        assert!(count >= 1, "blurring filter field must emit damage");
        let covers_filter_halo = rects[..count].iter().any(|r| {
            r.x <= 700.0 && r.x + r.w >= 700.0 && r.y <= 45.0 && r.y + r.h >= 79.0 && r.h > 32.0
        });
        assert!(
            covers_filter_halo,
            "filter blur damage {:?} must cover outer border halo (y <= 45..>= 79)",
            &rects[..count]
        );
    }
}
