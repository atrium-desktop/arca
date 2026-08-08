//! Floating menus: the right-click context menu for file rows and the
//! toolbar settings popup. Both use the aegis popover material.
//!
//! Overlay ids hash per id-scope, so every `overlay_open`/`overlay`/
//! `overlay_close` for one menu must run in the same scope — these builders
//! (called from the root build scope) own all of them; the triggers deeper
//! in the tree only record anchor state on [`UiApp`]. lens itself dismisses
//! an open overlay on Escape or an outside press (with a same-frame grace
//! for the opening click), which these builders mirror back into app state.

use iris::{Align, Frame, OverlayOpts};
use lantern_core::config::ThemeMode;

use crate::app::{CTX_MENU_ID, SETTINGS_MENU_ID, UiApp};
use crate::icons::{self, ids};
use crate::theme::Tones;

/// Frosted popover body shared by both menus (aegis `popover` material:
/// translucent surface with a 1 px hairline — without lensing, the painted
/// edge is what separates the panel from the content beneath it).
fn popover_opts(tones: &Tones) -> OverlayOpts {
    OverlayOpts {
        gap: 2.0,
        pad: 4.0,
        cross: Align::Stretch,
        bg: tones.popover,
        border: tones.popover_border,
        border_width: 1.0,
        radius: 12.0,
        min_width: 180.0,
    }
}

/// Run the first open for a pending menu; returns false when the overlay is
/// (or has just been) closed, meaning the app-side state stays cleared.
fn ensure_open(f: &mut Frame, id: &str, opened: &mut bool) -> bool {
    if !*opened {
        if f.overlay_is_open(id) {
            // Re-anchored while open: cycle the open state so the dismissal
            // grace covers this frame's triggering click.
            f.overlay_close(id);
        }
        f.overlay_open(id);
        *opened = true;
    }
    f.overlay_is_open(id)
}

pub(crate) fn build_ctx_menu(app: &mut UiApp, f: &mut Frame, tones: &Tones) {
    let Some(mut menu) = app.ctx_menu.take() else {
        return;
    };
    if !ensure_open(f, CTX_MENU_ID, &mut menu.opened) {
        return;
    }

    let mut keep_open = true;
    f.overlay(CTX_MENU_ID, menu.anchor, &popover_opts(tones), |f| {
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

/// Appearance picker hanging off the toolbar settings button.
pub(crate) fn build_settings_menu(app: &mut UiApp, f: &mut Frame, tones: &Tones) {
    let Some(mut menu) = app.settings_menu.take() else {
        // Toggled off this frame: keep the lens overlay in sync.
        if f.overlay_is_open(SETTINGS_MENU_ID) {
            f.overlay_close(SETTINGS_MENU_ID);
        }
        return;
    };
    if !ensure_open(f, SETTINGS_MENU_ID, &mut menu.opened) {
        return;
    }

    let mut keep_open = true;
    f.overlay(SETTINGS_MENU_ID, menu.anchor, &popover_opts(tones), |f| {
        for (mode, icon, label) in [
            (ThemeMode::System, ids::Monitor, "System"),
            (ThemeMode::Light, ids::Sun, "Light"),
            (ThemeMode::Dark, ids::Moon, "Dark"),
        ] {
            if icons::selectable_icon(f, icon, label, app.state.theme == mode) {
                app.state.set_theme(mode);
                keep_open = false;
            }
        }
        f.separator();
        // Checkbox-style row: toggling keeps the popup open.
        if f.selectable("Show thumbnails", app.state.show_thumbnails) {
            app.state.toggle_thumbnails();
        }
    });

    if keep_open {
        app.settings_menu = Some(menu);
    } else {
        f.overlay_close(SETTINGS_MENU_ID);
        app.settings_menu = None;
    }
}
