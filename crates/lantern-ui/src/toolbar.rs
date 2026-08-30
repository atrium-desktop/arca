//! Primary command bar: navigation, location/search and view controls.

use iris::{Align, Frame, LayoutOpts, Rect};
use lantern_core::ViewMode;

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::Tones;

pub(crate) fn build_toolbar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.size_next(0.0, 54.0);
    frame.row_ex(
        &LayoutOpts {
            height: 54.0,
            gap: 5.0,
            pad: 8.0,
            cross: Align::Center,
            bg: tones.toolbar,
            ..Default::default()
        },
        |frame| {
            if icons::icon_button(frame, ids::ArrowLeft, 32.0) {
                app.state.go_back();
            }
            if icons::icon_button(frame, ids::ArrowRight, 32.0) {
                app.state.go_forward();
            }
            if icons::icon_button(frame, ids::ArrowUp, 32.0) {
                app.state.go_up();
            }
            if icons::icon_button(frame, ids::RefreshCw, 32.0) {
                app.state.refresh();
            }

            frame.size_next(5.0, 0.0);
            frame.spacer(5.0);

            if !app.location_focused {
                let cwd = app.state.cwd().to_string();
                if app.location.as_str() != cwd {
                    app.location.set(&cwd);
                }
            }
            icons::icon(frame, ids::Folder, 16.0);
            frame.flex(1.0);
            frame.textfield("##location", &mut app.location);
            capture(frame, &mut app.location_id, &mut app.location_focused_now);

            frame.size_next(7.0, 0.0);
            frame.spacer(7.0);

            icons::icon(frame, ids::Search, 16.0);
            frame.size_next(150.0, 0.0);
            let changed =
                frame.textfield_placeholder("##filter", &mut app.filter, "Filter");
            capture(frame, &mut app.filter_id, &mut app.filter_focused_now);
            if changed {
                app.state.set_filter(app.filter.as_str().into_owned());
            } else if !app.filter_focused_now && app.filter.as_str() != app.state.filter() {
                app.filter.set(app.state.filter());
            }

            frame.size_next(8.0, 0.0);
            frame.spacer(8.0);

            view_button(app, frame, ViewMode::Grid, ids::Grid);
            view_button(app, frame, ViewMode::List, ids::List);
            view_button(app, frame, ViewMode::Miller, ids::Columns);

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            if icons::icon_toggle_button(
                frame,
                ids::EyeOff,
                ids::Eye,
                32.0,
                app.state.show_hidden,
            ) {
                app.state.toggle_hidden();
            }

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            if icons::icon_button(frame, ids::Settings, 32.0) {
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

fn view_button(app: &mut UiApp, frame: &mut Frame, mode: ViewMode, icon: icons::AssetId) {
    if icons::icon_button_active_rounded(frame, icon, 32.0, app.state.view_mode == mode) {
        app.state.set_view_mode(mode);
    }
}

fn capture(frame: &mut Frame, id: &mut lens_sys::lens_id, focused: &mut bool) {
    let response = frame.response();
    *id = response.id;
    *focused = response.focused;
}
