//! Browser tab strip. Each tab owns an independent navigation stack in core;
//! this module only renders the strip and forwards selection/close actions.
//! Chrome-style anatomy: every tab carries its own close button and the
//! new-tab button trails the last tab instead of sitting at the far edge.

use iris::{Align, Color, Frame, LayoutOpts};

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

/// One tab in the strip: title plus a close button on a pressable cell.
/// Returns which action the frame produced, if any.
enum TabAction {
    Switch(usize),
    Close(usize),
}

fn tab_cell(app: &mut UiApp, frame: &mut Frame, tones: &Tones, index: usize, title: &str) -> Option<TabAction> {
    let active = index == app.state.active_tab_index();
    let options = LayoutOpts {
        height: 30.0,
        gap: 2.0,
        pad: 8.0,
        cross: Align::Center,
        bg: if active {
            tones.selected
        } else {
            Color::TRANSPARENT
        },
        radius: 8.0,
        ..Default::default()
    };
    let mut action = None;
    frame.push_id(&format!("tab-{index}"));
    let (response, ()) = frame.pressable_row(
        &format!("tab-{}", app.state.tabs()[index].id),
        title,
        &options,
        |frame, _| {
            let fg = if active {
                frame.theme().fg()
            } else {
                tones.muted
            };
            theme::label_colored_sized(frame, title, 12.5, fg);
            // Only the active tab needs a visible close affordance at rest;
            // core refuses to close the final tab anyway, so render it
            // always and let `close_tab` decide.
            if icons::icon_button(frame, ids::X, 20.0) {
                action = Some(TabAction::Close(index));
            }
        },
    );
    frame.pop_id();
    if response.clicked {
        action = Some(TabAction::Switch(index));
    }
    action
}

fn tab_title(cwd: &str) -> String {
    let name = std::path::Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "/".into());
    // Cap the strip cell: long directory names would otherwise eat the bar.
    const MAX_CHARS: usize = 18;
    if name.chars().count() > MAX_CHARS {
        let truncated: String = name.chars().take(MAX_CHARS - 1).collect();
        format!("{truncated}…")
    } else {
        name
    }
}

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
            icons::icon(frame, ids::Aperture, 18.0);
            theme::label_colored_sized(frame, "LANTERN", 11.0, tones.muted);

            frame.size_next(10.0, 0.0);
            frame.spacer(10.0);

            let titles = app
                .state
                .tabs()
                .iter()
                .map(|tab| tab_title(&tab.cwd))
                .collect::<Vec<_>>();
            let mut action = None;
            for (index, title) in titles.iter().enumerate() {
                if let Some(a) = tab_cell(app, frame, tones, index, title) {
                    action = Some(a);
                }
            }

            if icons::icon_button(frame, ids::Plus, 30.0) {
                app.new_tab();
            }

            // Window close sits at the strip's right edge: the compositor
            // provides no server-side decoration, so the app draws its own.
            frame.flex(1.0);
            frame.spacer(0.0);
            if icons::icon_button(frame, ids::X, 30.0) {
                // SAFETY: sets iris's running flag; iris_app_run returns as
                // on a compositor close. A no-op outside an active app.
                unsafe { iris::sys::iris_window_close() };
            }

            match action {
                Some(TabAction::Switch(index)) => app.switch_tab(index),
                Some(TabAction::Close(index)) => app.close_tab(index),
                None => {}
            }
        },
    );
}
