//! Right-click context menu for file rows.

use iris::{Align, Frame, Input, OverlayOpts};

use crate::app::{UiApp, CTX_MENU_ID};
use crate::theme::Tones;

pub(crate) fn build_ctx_menu(app: &mut UiApp, f: &mut Frame, input: &Input, tones: &Tones) {
    let Some(menu) = app.ctx_menu.take() else {
        return;
    };
    let mut keep_open = true;

    // Press anywhere outside the menu dismisses it.
    let pressed_outside = input.as_raw().mouse_pressed[0] && !f.overlay_hovered(CTX_MENU_ID);
    if pressed_outside {
        keep_open = false;
    }

    let theme = f.theme();
    let opts = OverlayOpts {
        gap: 2.0,
        pad: 4.0,
        cross: Align::Stretch,
        bg: tones.card,
        border: theme.border(),
        border_width: 1.0,
        radius: 6.0,
        min_width: 180.0,
    };

    f.overlay(CTX_MENU_ID, menu.anchor, &opts, |f| {
        if f.selectable("Open", false) {
            app.state.open_selected();
            keep_open = false;
        }
        if f.selectable("Quick Look", false) {
            app.toggle_preview();
            keep_open = false;
        }
        let selected_directory = app
            .state
            .selected_entry()
            .filter(|entry| entry.navigable)
            .and_then(|_| app.state.selected_path());
        if selected_directory.is_some() && f.selectable("Open in New Tab", false) {
            if let Some(path) = selected_directory {
                app.new_tab_at(&path);
            }
            keep_open = false;
        }
        if f.selectable("Rename", false) {
            if let Some(name) = app.state.selected_entry().map(|e| e.name.clone()) {
                app.begin_rename(name);
            }
            keep_open = false;
        }
        if f.selectable("Copy Path", false) {
            if let Some(path) = app.state.selected_path() {
                // SAFETY: the frame is live; path bytes outlive the call.
                unsafe { lens_sys::lens_copy(f.as_raw(), path.as_ptr().cast(), path.len()) };
                app.state.set_status("Path copied to clipboard".into());
            }
            keep_open = false;
        }
        f.separator();
        if f.selectable("Move to Trash", false) {
            app.state.trash_selected();
            keep_open = false;
        }
    });

    if keep_open {
        app.ctx_menu = Some(menu);
    } else {
        f.overlay_close(CTX_MENU_ID);
        app.ctx_menu = None;
    }
}
