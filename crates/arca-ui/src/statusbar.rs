//! Compact status and keyboard-discovery bar.

use iris::{Align, Frame, LayoutOpts};

use crate::app::UiApp;
use crate::theme::{self, Tones};

pub(crate) fn build_statusbar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let total = app.state.entries().len();
    let filtered = app.state.visible().len();
    let mut left = if app.state.read_error().is_some() {
        "Folder unavailable".to_string()
    } else if app.state.filter().is_empty() {
        format!("{total} items")
    } else {
        format!("{filtered} of {total} items")
    };
    let count = app.state.selected_count();
    if count > 1 {
        let total_size: u64 = app.state.selected_entries().iter().map(|e| e.size).sum();
        left.push_str(&format!(
            "  ·  {count} items selected ({})",
            arca_engine::format::format_size(total_size)
        ));
    } else if let Some(selected) = app.state.selected_entry() {
        left.push_str(&format!("  ·  {}", selected.name));
    }

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
