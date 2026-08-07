//! Browser tab strip. Each tab owns an independent navigation stack in core;
//! this module only renders the strip and forwards selection/close actions.

use iris::{Align, Frame, LayoutOpts, TabStyle, TabsOpts};

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) fn build_tabs(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.size_next(0.0, 42.0);
    frame.row_ex(
        &LayoutOpts {
            gap: 6.0,
            pad: 6.0,
            cross: Align::Center,
            bg: tones.tab_bar,
            ..Default::default()
        },
        |frame| {
            frame.size_next(22.0, 22.0);
            icons::icon(frame, ids::LENS_ICON_APERTURE, 18.0);
            theme::label_colored_sized(frame, "LANTERN", 11.0, tones.muted);

            frame.size_next(10.0, 0.0);
            frame.spacer(10.0);

            let labels = app
                .state
                .tabs()
                .iter()
                .map(|tab| {
                    let title = std::path::Path::new(&tab.cwd)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "/".into());
                    format!("{title}##tab-{}", tab.id)
                })
                .collect::<Vec<_>>();
            let mut active = app.state.active_tab_index() as i32;
            frame.flex(1.0);
            frame.tabs_ex(
                "browser-tabs",
                &mut active,
                &TabsOpts {
                    style: TabStyle::Indicator,
                    indicator_color: frame.theme().accent(),
                    indicator_thickness: 2.5,
                    indicator_gap: 1.0,
                    indicator_padding: 10.0,
                    ..Default::default()
                },
                |frame| {
                    for label in &labels {
                        frame.tab(label);
                    }
                },
            );
            if active >= 0 {
                app.switch_tab(active as usize);
            }

            frame.size_next(30.0, 30.0);
            if icons::icon_button(frame, ids::LENS_ICON_X) {
                app.close_active_tab();
            }
            frame.size_next(30.0, 30.0);
            if icons::icon_button(frame, ids::LENS_ICON_PLUS) {
                app.new_tab();
            }
        },
    );
}
