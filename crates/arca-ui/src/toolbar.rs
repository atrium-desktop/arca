//! Primary command bar: navigation, location/search and view controls.

use arca_engine::ViewMode;
use iris::{Align, Frame, LayoutOpts, Rect, SegmentedControl, SegmentedItem};

use crate::app::{focus_any, FocusTarget, UiApp};
use crate::icons::{self, ids};
use crate::theme::Tones;

pub(crate) fn build_toolbar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.size_next(0.0, 48.0);
    frame.row_ex(
        &LayoutOpts {
            height: 48.0,
            gap: 6.0,
            pad: 8.0,
            cross: Align::Center,
            bg: tones.toolbar,
            ..Default::default()
        },
        |frame| {
            if icons::icon_button(frame, ids::ArrowLeft, 30.0) {
                app.state.go_back();
            }
            if icons::icon_button(frame, ids::ArrowRight, 30.0) {
                app.state.go_forward();
            }
            if icons::icon_button(frame, ids::ArrowUp, 30.0) {
                app.state.go_up();
            }
            if icons::icon_button(frame, ids::RefreshCw, 30.0) {
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
                .with_border(iris::Color::TRANSPARENT)
                .with_border_width(1.0)
                .with_corner_radius(8.0)
                .with_padding(10.0);

            icons::icon(frame, ids::Folder, 16.0);
            frame.flex(1.0);
            frame.size_next(0.0, 32.0);
            frame.set_theme(input_theme);
            let loc_resp = frame.textfield_ex("##location", &mut app.location, true);
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

            icons::icon(frame, ids::Search, 16.0);
            frame.size_next(180.0, 32.0);
            frame.set_theme(input_theme);
            let changed =
                frame.textfield_placeholder("##filter", &mut app.filter, "Search / Filter...");
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
                SegmentedItem::icon(icons::lens_id(ids::Grid)),
                SegmentedItem::icon(icons::lens_id(ids::List)),
                SegmentedItem::icon(icons::lens_id(ids::Columns)),
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

            if icons::icon_toggle_button(frame, ids::EyeOff, ids::Eye, 30.0, app.state.show_hidden)
            {
                app.state.toggle_hidden();
            }

            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            if icons::icon_button(frame, ids::Settings, 30.0) {
                let r = frame.response();
                let anchor = Rect {
                    x: r.rect.x,
                    y: r.rect.y,
                    w: r.rect.w,
                    h: r.rect.h,
                };
                app.toggle_settings_menu(anchor);
            }
        },
    );
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
    /// strip; `iris_window_close` outside an active app run is a no-op.
    #[test]
    fn window_close_button_is_clickable() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        let close = (1019.0, 21.0);
        let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(close.0, close.1);
        press.set_mouse_down(lens::MouseButton::Left, true);
        press.set_mouse_pressed(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);
        let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(close.0, close.1);
        release.set_mouse_released(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    /// The tab strip drag region fills the space between the last tab and the close button;
    /// pressing it triggers `iris_window_start_move` (a safe no-op headless).
    #[test]
    fn titlebar_drag_region_is_interactive() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        // Middle of the top bar in the drag region
        let drag_point = (500.0, 21.0);
        let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(drag_point.0, drag_point.1);
        press.set_mouse_down(lens::MouseButton::Left, true);
        press.set_mouse_pressed(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);
        let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(drag_point.0, drag_point.1);
        release.set_mouse_released(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
    }

    #[test]
    fn settings_button_toggles_appearance_popup() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.settings_menu.is_none());

        // The settings button is the right-most 32px icon in the 54px
        // toolbar, which sits below the 42px tab strip.
        let gear = (1016.0, 69.0);
        let click = |app: &mut UiApp, ui: &mut lens::Ui| {
            let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(gear.0, gear.1);
            press.set_mouse_down(lens::MouseButton::Left, true);
            press.set_mouse_pressed(lens::MouseButton::Left, true);
            frame(app, ui, &press);
            let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(gear.0, gear.1);
            release.set_mouse_released(lens::MouseButton::Left, true);
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
}
