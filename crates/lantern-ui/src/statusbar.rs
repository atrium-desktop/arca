//! Compact status and keyboard-discovery bar.

use iris::{Align, Frame, LayoutOpts};

use crate::app::UiApp;
use crate::theme::{self, Tones};

const HINT: &str = "SPACE Preview   ↑↓ Select   ENTER Open   CTRL+T New tab   CTRL+W Close tab";

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
    if let Some(selected) = app.state.selected_entry() {
        left.push_str(&format!("  ·  {}", selected.name));
    }

    let right = app.state.status.clone().unwrap_or_else(|| HINT.into());
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
