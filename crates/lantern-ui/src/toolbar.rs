//! Primary command bar: navigation, location/search and view controls.

use iris::{Align, Frame, LayoutOpts, Rect};
use lens::patterns::{SegmentedControl, SegmentedItem};
use lantern_core::ViewMode;

use crate::app::UiApp;
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

            if !app.location_focused {
                let cwd = app.state.cwd().to_string();
                if app.location.as_str() != cwd {
                    app.location.set(&cwd);
                }
            }
            icons::icon(frame, ids::Folder, 16.0);
            frame.flex(1.0);
            frame.size_next(0.0, 32.0);
            frame.textfield("##location", &mut app.location);
            capture(frame, &mut app.location_id, &mut app.location_focused_now);

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            icons::icon(frame, ids::Search, 16.0);
            frame.size_next(180.0, 32.0);
            let changed =
                frame.textfield_placeholder("##filter", &mut app.filter, "Search / Filter...");
            capture(frame, &mut app.filter_id, &mut app.filter_focused_now);
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

            if control.show(frame, &items, &mut current_idx) {
                let new_mode = match current_idx {
                    0 => ViewMode::Grid,
                    1 => ViewMode::List,
                    _ => ViewMode::Miller,
                };
                app.state.set_view_mode(new_mode);
            }

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            if icons::icon_toggle_button(
                frame,
                ids::EyeOff,
                ids::Eye,
                30.0,
                app.state.show_hidden,
            ) {
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

fn capture(frame: &mut Frame, id: &mut lens_sys::lens_id, focused: &mut bool) {
    let response = frame.response();
    *id = response.id;
    *focused = response.focused;
}
