//! Browser tab strip using the lens TabStrip pattern.

use iris::{Align, Frame, LayoutOpts};
use lens::patterns::{TabAction, TabItem, TabStrip};

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::Tones;

fn tab_title(cwd: &str) -> String {
    let name = std::path::Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "/".into());
    name
}

pub(crate) fn build_tabs(app: &mut UiApp, frame: &mut Frame, input: &iris::Input, tones: &Tones) {
    frame.size_next(0.0, 38.0);
    frame.row_ex(
        &LayoutOpts {
            gap: 6.0,
            pad: 4.0,
            cross: Align::Center,
            bg: tones.tab_bar,
            ..Default::default()
        },
        |frame| {
            // Subtle logo icon
            icons::icon(frame, ids::Aperture, 16.0);
            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            let tabs: Vec<TabItem> = app
                .state
                .tabs()
                .iter()
                .map(|tab| {
                    TabItem::new(tab_title(&tab.cwd))
                        .icon(icons::lens_id(ids::Folder))
                        .closable(app.state.tabs().len() > 1)
                })
                .collect();

            let active_idx = app.state.active_tab_index();

            let strip = TabStrip::new("##main-tabs")
                .height(30.0)
                .min_tab_width(100.0)
                .max_tab_width(180.0)
                .show_new_button(true)
                .close_icon(icons::lens_id(ids::X))
                .new_icon(icons::lens_id(ids::Plus));

            let action = strip.show(frame, &tabs, active_idx);

            match action {
                TabAction::Select(index) => app.switch_tab(index),
                TabAction::Close(index) => app.close_tab(index),
                TabAction::NewTab => app.new_tab(),
                TabAction::None => {}
            }

            // Remainder space in the tab strip (pure layout flex spacer, not a widget)
            frame.flex(1.0);
            frame.spacer(0.0);

            // Window close button at the far right
            let close_clicked = icons::icon_button(frame, ids::X, 26.0);
            if close_clicked {
                iris::window_close();
            }

            // Window drag behavior (CSD):
            // The top bar background is purely non-interactive (not a button widget,
            // no hover, no focus outline). When pointer press occurs on the top bar
            // area outside the close button, request interactive move from compositor.
            let raw_in = input.as_raw();
            if raw_in.mouse_pressed[0]
                && !close_clicked
                && matches!(action, TabAction::None | TabAction::Select(_))
            {
                let cursor = raw_in.cursor;
                let display = raw_in.display_size;
                if cursor.y >= 0.0 && cursor.y <= 38.0 && cursor.x >= 0.0 && cursor.x < display.x - 34.0 {
                    iris::window_start_move();
                }
            }
        },
    );
}
