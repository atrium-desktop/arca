//! Compact status and keyboard-discovery bar.

use optics_ui::{Align, Frame, LayoutOpts};

use arca_engine::AppState;

use crate::app::UiApp;
use crate::theme::{self, Tones};

/// The left-hand summary: listing size, then the selection.
///
/// Pure so it can be asserted directly; the widget shell below only lays it
/// out.
fn summary(state: &AppState) -> String {
    let total = state.entries().len();
    let filtered = state.visible().len();
    let mut left = if state.read_error().is_some() {
        "Folder unavailable".to_string()
    } else if state.filter().is_empty() {
        format!("{total} items")
    } else {
        format!("{filtered} of {total} items")
    };

    let count = state.selected_count();
    if count > 1 {
        // Directories contribute no bytes: `Entry::size` for a directory is
        // its inode length, which the Size column already shows as "—".
        let total_size: u64 = state
            .selected_entries()
            .iter()
            .filter(|entry| entry.file_type == arca_engine::FileType::Regular)
            .map(|entry| entry.size)
            .sum();
        left.push_str(&format!(
            "  ·  {count} items selected ({})",
            arca_engine::format::format_size(total_size)
        ));
    } else if let Some(selected) = state.selected_entry() {
        left.push_str(&format!("  ·  {}", selected.name));
    }
    left
}

pub(crate) fn build_statusbar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let left = summary(&app.state);

    let right = if let Some(prog) = &app.state.io_engine.current_progress {
        format!(
            "Transferring {}: {:.0}% ({}/{})",
            prog.current_file,
            prog.percent(),
            arca_engine::format::format_size(prog.bytes_copied),
            arca_engine::format::format_size(prog.total_bytes),
        )
    } else {
        app.state.status.clone().unwrap_or_else(|| {
            if app.viewport.0 > 800.0 {
                "SPACE Preview   ENTER Open".into()
            } else {
                String::new()
            }
        })
    };
    frame.size_next(0.0, 32.0);
    frame.row_ex(
        &LayoutOpts {
            height: 32.0,
            gap: 12.0,
            pad: 8.0,
            cross: Align::Center,
            bg: tones.status_bar,
            ..Default::default()
        },
        |frame| {
            theme::label_colored_sized(frame, &left, 11.5, tones.muted);
            frame.flex(1.0);
            frame.spacer(0.0);
            theme::label_colored_sized(frame, &right, 11.5, tones.muted);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, frame};
    use arca_engine::config::Config;

    #[test]
    fn counts_every_entry_when_unfiltered() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        // `sub`, `a.txt` (+ `.hidden`, which is filtered out by default).
        assert_eq!(summary(&app.state), "2 items");
    }

    #[test]
    fn reports_the_filtered_subset() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        app.state.set_filter("txt".into());
        assert_eq!(summary(&app.state), "1 of 2 items");
    }

    #[test]
    fn names_a_single_selection() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        app.state.select_visible(1);
        assert_eq!(summary(&app.state), "2 items  ·  a.txt");
    }

    #[test]
    fn aggregates_a_multi_selection_with_total_size() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        app.state.select_all();
        // `sub` is a directory and contributes no bytes; `a.txt` holds one.
        assert_eq!(summary(&app.state), "2 items  ·  2 items selected (1 B)");
    }

    /// `navigate` refuses a path that is not a directory, so `read_error` is
    /// reachable only when a folder disappears *underneath* the view — which
    /// is exactly what a refresh after deletion does.
    #[test]
    fn a_vanished_folder_reports_unavailable() {
        let (dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        app.state.navigate(dir.join("sub").to_str().unwrap());
        assert_eq!(summary(&app.state), "0 items");

        std::fs::remove_dir(dir.join("sub")).unwrap();
        app.state.refresh();
        assert!(app.state.read_error().is_some());
        assert_eq!(summary(&app.state), "Folder unavailable");
    }

    /// The summary must never claim an empty folder has items.
    #[test]
    fn an_empty_folder_says_zero() {
        let dir = crate::test_support::ScratchDir::new("arca-status-empty");
        let config_file = dir.path().join("cfg").join("arca.conf");
        let state = AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.str(),
        );
        let cache = dir.cache_path().to_path_buf();
        let mut app = UiApp::with_thumb_cache(state, cache).sync_navigation();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        assert_eq!(summary(&app.state), "0 items");
    }
}
