//! Primary command bar: navigation, location/search and view controls.

use iris::{Align, Frame, LayoutOpts};
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
            frame.size_next(32.0, 32.0);
            if icons::icon_button(frame, ids::LENS_ICON_ARROW_LEFT) {
                app.state.go_back();
            }
            frame.size_next(32.0, 32.0);
            if icons::icon_button(frame, ids::LENS_ICON_ARROW_RIGHT) {
                app.state.go_forward();
            }
            frame.size_next(32.0, 32.0);
            if icons::icon_button(frame, ids::LENS_ICON_ARROW_UP) {
                app.state.go_up();
            }
            frame.size_next(32.0, 32.0);
            if icons::icon_button(frame, ids::LENS_ICON_REFRESH_CW) {
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
            frame.size_next(18.0, 18.0);
            icons::icon(frame, ids::LENS_ICON_FOLDER, 15.0);
            frame.flex(1.0);
            frame.textfield("##location", &mut app.location);
            capture(frame, &mut app.location_id, &mut app.location_focused_now);

            frame.size_next(7.0, 0.0);
            frame.spacer(7.0);

            frame.size_next(18.0, 18.0);
            icons::icon(frame, ids::LENS_ICON_SEARCH, 15.0);
            frame.size_next(150.0, 0.0);
            let changed = frame.textfield("##filter", &mut app.filter);
            capture(frame, &mut app.filter_id, &mut app.filter_focused_now);
            if changed {
                app.state.set_filter(app.filter.as_str().into_owned());
            } else if !app.filter_focused_now && app.filter.as_str() != app.state.filter() {
                app.filter.set(app.state.filter());
            }

            frame.size_next(8.0, 0.0);
            frame.spacer(8.0);

            view_button(app, frame, ViewMode::Grid, ids::LENS_ICON_GRID);
            view_button(app, frame, ViewMode::List, ids::LENS_ICON_LIST);
            view_button(app, frame, ViewMode::Miller, ids::LENS_ICON_COLUMNS);

            frame.size_next(6.0, 0.0);
            frame.spacer(6.0);

            frame.size_next(32.0, 32.0);
            if icons::icon_toggle_button(
                frame,
                ids::LENS_ICON_EYE_OFF,
                ids::LENS_ICON_EYE,
                16.0,
                app.state.show_hidden,
            ) {
                app.state.toggle_hidden();
            }

            frame.size_next(32.0, 32.0);
            let cwd = app.state.cwd().to_string();
            let bookmarked = app.state.is_bookmarked(&cwd);
            if icons::icon_toggle_button(
                frame,
                ids::LENS_ICON_STAR_ROUNDED,
                ids::LENS_ICON_STAR_ROUNDED_FILLED,
                16.0,
                bookmarked,
            ) {
                app.state.toggle_bookmark(&cwd);
            }
        },
    );
}

fn view_button(app: &mut UiApp, frame: &mut Frame, mode: ViewMode, icon: icons::IconId) {
    frame.size_next(32.0, 32.0);
    if icons::icon_button_active(frame, icon, app.state.view_mode == mode) {
        app.state.set_view_mode(mode);
    }
}

fn capture(frame: &mut Frame, id: &mut lens_sys::lens_id, focused: &mut bool) {
    let response = unsafe { lens_sys::lens_get_response(frame.as_raw()) };
    *id = response.id;
    *focused = response.focused;
}
