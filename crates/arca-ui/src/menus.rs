//! Floating menus: the right-click context menu for file rows and the
//! toolbar settings popup. Both use the tessera popover material.
//!
//! Overlay ids hash per id-scope, so every `overlay_open`/`overlay`/
//! `overlay_close` for one menu must run in the same scope — these builders
//! (called from the root build scope) own all of them; the triggers deeper
//! in the tree only record anchor state on [`UiApp`]. optics-ui itself dismisses
//! an open overlay on Escape or an outside press (with a same-frame grace
//! for the opening click), which these builders mirror back into app state.

use arca_engine::config::ThemeMode;
use optics_ui::patterns::{popup_menu, MenuItem};
use optics_ui::{Align, Frame, LayoutOpts, PlaceMode, PlaceOpts, Rect};

use crate::app::{UiApp, CTX_MENU_ID, SETTINGS_MENU_ID, SIDEBAR_MENU_ID};
use crate::icons::{self, ids};
use crate::theme::Tones;

/// Frosted popover body shared by both menus (tessera `popover` material:
/// translucent surface with a 1 px hairline — without the lensing material, the painted
/// edge is what separates the panel from the content beneath it).
fn popover_opts(tones: &Tones, anchor: Rect) -> PlaceOpts {
    PlaceOpts {
        mode: PlaceMode::Anchored,
        rect: anchor,
        layout: LayoutOpts {
            gap: 2.0,
            pad: 6.0,
            cross: Align::Stretch,
            bg: tones.popover,
            border: tones.popover_border,
            border_width: 1.0,
            radius: 10.0,
            min_width: 180.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Run the first open for a pending menu; returns false when the overlay is
/// (or has just been) closed, meaning the app-side state stays cleared.
fn ensure_open(f: &mut Frame, id: &str, opened: &mut bool) -> bool {
    if !*opened {
        if f.place_is_open(id) {
            // Re-anchored while open: cycle the open state so the dismissal
            // grace covers this frame's triggering click.
            f.place_close(id);
        }
        f.place_open(id);
        *opened = true;
    }
    f.place_is_open(id)
}

pub(crate) fn build_ctx_menu(app: &mut UiApp, f: &mut Frame, _tones: &Tones) {
    let Some(mut menu) = app.ctx_menu.take() else {
        return;
    };
    if !ensure_open(f, CTX_MENU_ID, &mut menu.opened) {
        return;
    }

    enum Action {
        None,
        Open,
        QuickLook,
        OpenInNewTab(String),
        ToggleBookmark(String),
        Rename(String),
        Copy,
        Cut,
        Paste,
        CopyPath(String),
        OpenWith(arca_xdg::DesktopEntry),
        Restore,
        Purge(Vec<String>),
        Trash,
    }

    let mut items = Vec::new();
    let mut actions = Vec::new();

    items.push(MenuItem::new("Open").icon(icons::icon_id(ids::Folder)));
    actions.push(Action::Open);

    items.push(
        MenuItem::new("Quick Look")
            .icon(icons::icon_id(ids::Eye))
            .shortcut("Space"),
    );
    actions.push(Action::QuickLook);

    let selected_directory = app
        .state
        .selected_entry()
        .filter(|entry| entry.navigable)
        .and_then(|_| app.state.selected_path());
    if let Some(ref path) = selected_directory {
        items.push(MenuItem::new("Open in New Tab").icon(icons::icon_id(ids::Plus)));
        actions.push(Action::OpenInNewTab(path.clone()));

        let is_bookmarked = app.state.is_bookmarked(path);
        let bookmark_label = if is_bookmarked {
            "Remove from Bookmarks"
        } else {
            "Add to Bookmarks"
        };
        items.push(MenuItem::new(bookmark_label).icon(icons::icon_id(ids::Folder)));
        actions.push(Action::ToggleBookmark(path.clone()));
    }

    if let Some(name) = app.state.selected_entry().map(|e| e.name.clone()) {
        items.push(MenuItem::new("Rename").shortcut("F2"));
        actions.push(Action::Rename(name));
    }

    items.push(
        MenuItem::new("Copy")
            .icon(icons::icon_id(ids::FileText))
            .shortcut("Ctrl+C"),
    );
    actions.push(Action::Copy);

    items.push(
        MenuItem::new("Cut")
            .icon(icons::icon_id(ids::FileText))
            .shortcut("Ctrl+X"),
    );
    actions.push(Action::Cut);

    items.push(
        MenuItem::new("Paste")
            .icon(icons::icon_id(ids::FileText))
            .shortcut("Ctrl+V"),
    );
    actions.push(Action::Paste);

    if let Some(path) = app.state.selected_path() {
        items.push(MenuItem::new("Copy Path").icon(icons::icon_id(ids::Link2)));
        actions.push(Action::CopyPath(path));
    }

    for app_entry in menu.open_with.iter().take(3) {
        let label = format!("Open with {}", app_entry.name);
        items.push(MenuItem::new(label).icon(icons::icon_id(ids::Terminal)));
        actions.push(Action::OpenWith(app_entry.clone()));
    }

    items.push(MenuItem::separator());
    actions.push(Action::None);

    if app.state.is_showing_trash() {
        let names = app.state.selected_entry_names();
        let label = if names.len() > 1 {
            format!("Restore {} items", names.len())
        } else {
            "Restore".to_string()
        };
        items.push(MenuItem::new(label).icon(icons::icon_id(ids::RefreshCw)));
        actions.push(Action::Restore);

        let purge_label = if names.len() > 1 {
            format!("Delete {} items permanently", names.len())
        } else {
            "Delete permanently".to_string()
        };
        items.push(MenuItem::new(purge_label).icon(icons::icon_id(ids::Trash)));
        actions.push(Action::Purge(names));
    } else {
        items.push(
            MenuItem::new("Move to Trash")
                .icon(icons::icon_id(ids::Trash))
                .shortcut("Delete"),
        );
        actions.push(Action::Trash);
    }

    let activated = popup_menu(f, CTX_MENU_ID, menu.anchor, &items);

    if let Some(idx) = activated {
        match actions.get(idx) {
            Some(Action::Open) => {
                let token = optics_window::window_create_activation_token(None);
                app.state.open_selected_with_token(token.as_deref());
            }
            Some(Action::QuickLook) => {
                app.toggle_preview();
            }
            Some(Action::OpenInNewTab(path)) => {
                app.new_tab_at(path);
            }
            Some(Action::ToggleBookmark(path)) => {
                app.state.toggle_bookmark(path);
            }
            Some(Action::Rename(name)) => {
                app.begin_rename(name.clone());
            }
            Some(Action::Copy) => {
                app.state.yank_selected(arca_engine::TransferAction::Copy);
                if let Some(payload) = app.state.clipboard_payload() {
                    f.copy(&payload);
                }
            }
            Some(Action::Cut) => {
                app.state.yank_selected(arca_engine::TransferAction::Move);
                if let Some(payload) = app.state.clipboard_payload() {
                    f.copy(&payload);
                }
            }
            Some(Action::Paste) => {
                if let Some(text) = f.take_paste() {
                    app.state.paste_text(&text);
                } else {
                    app.pending_paste = true;
                    f.request_paste();
                    if app.state.clipboard.is_some() {
                        app.state.paste();
                        app.pending_paste = false;
                    }
                }
            }
            Some(Action::CopyPath(path)) => {
                f.copy(path);
                app.state.set_status("Path copied to clipboard".into());
            }
            Some(Action::OpenWith(app_entry)) => {
                if let Some(path_str) = app.state.selected_path() {
                    let token = optics_window::window_create_activation_token(Some(&app_entry.id));
                    let _ = app_entry
                        .spawn_with_token(&[std::path::Path::new(&path_str)], token.as_deref());
                }
            }
            Some(Action::Restore) => {
                app.state.restore_selected();
            }
            Some(Action::Purge(names)) => {
                app.state.purge_trash_ids(names);
            }
            Some(Action::Trash) => {
                app.state.trash_selected();
            }
            _ => {}
        }
        f.place_close(CTX_MENU_ID);
        app.ctx_menu = None;
    } else if f.place_is_open(CTX_MENU_ID) {
        app.ctx_menu = Some(menu);
    } else {
        app.ctx_menu = None;
    }
}

/// Context menu for items in the places / bookmarks sidebar.
pub(crate) fn build_sidebar_menu(app: &mut UiApp, f: &mut Frame, _tones: &Tones) {
    let Some(mut menu) = app.sidebar_menu.take() else {
        return;
    };
    if !ensure_open(f, SIDEBAR_MENU_ID, &mut menu.opened) {
        return;
    }

    enum Action {
        None,
        OpenInNewTab,
        CopyPath,
        RemoveBookmark,
        EmptyTrash,
    }

    let mut items = Vec::new();
    let mut actions = Vec::new();

    items.push(MenuItem::new("Open in New Tab").icon(icons::icon_id(ids::Plus)));
    actions.push(Action::OpenInNewTab);

    items.push(MenuItem::new("Copy Path").icon(icons::icon_id(ids::Link2)));
    actions.push(Action::CopyPath);

    if menu.is_folder {
        items.push(MenuItem::separator());
        actions.push(Action::None);

        items.push(MenuItem::new("Remove from Bookmarks").icon(icons::icon_id(ids::Folder)));
        actions.push(Action::RemoveBookmark);
    }

    if arca_engine::AppState::is_trash_location(&menu.path) {
        items.push(MenuItem::separator());
        actions.push(Action::None);

        items.push(MenuItem::new("Empty Trash").icon(icons::icon_id(ids::Trash)));
        actions.push(Action::EmptyTrash);
    }

    let activated = popup_menu(f, SIDEBAR_MENU_ID, menu.anchor, &items);

    if let Some(idx) = activated {
        match actions.get(idx) {
            Some(Action::OpenInNewTab) => {
                app.new_tab_at(&menu.path);
            }
            Some(Action::CopyPath) => {
                f.copy(&menu.path);
                app.state.set_status("Path copied to clipboard".into());
            }
            Some(Action::RemoveBookmark) => {
                app.state.toggle_bookmark(&menu.path);
            }
            Some(Action::EmptyTrash) => {
                app.state.empty_trash();
            }
            _ => {}
        }
        f.place_close(SIDEBAR_MENU_ID);
        app.sidebar_menu = None;
    } else if f.place_is_open(SIDEBAR_MENU_ID) {
        app.sidebar_menu = Some(menu);
    } else {
        app.sidebar_menu = None;
    }
}

/// Appearance picker hanging off the toolbar settings button.
pub(crate) fn build_settings_menu(app: &mut UiApp, f: &mut Frame, tones: &Tones) {
    let Some(mut menu) = app.settings_menu.take() else {
        // Toggled off this frame: keep the optics-ui overlay in sync.
        if f.place_is_open(SETTINGS_MENU_ID) {
            f.place_close(SETTINGS_MENU_ID);
        }
        return;
    };
    if !ensure_open(f, SETTINGS_MENU_ID, &mut menu.opened) {
        return;
    }

    let mut keep_open = true;
    f.place(SETTINGS_MENU_ID, &popover_opts(tones, menu.anchor), |f| {
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
        f.place_close(SETTINGS_MENU_ID);
        app.settings_menu = None;
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use crate::test_support::{fixture, frame};

    #[test]
    fn context_menu_and_sidebar_bookmark_flow() {
        let (dir, mut app) = fixture();
        let sub_dir = dir.join("subfolder");
        std::fs::create_dir(&sub_dir).unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Add subfolder to bookmarks
        let sub_path = sub_dir.to_str().unwrap();
        assert!(!app.state.is_bookmarked(sub_path));
        app.state.toggle_bookmark(sub_path);
        assert!(app.state.is_bookmarked(sub_path));

        // Rebuild frames with bookmark visible
        frame(&mut app, &mut ui, &input);
        assert!(app.state.bookmarks.iter().any(|b| b.path == sub_path));

        // Remove from bookmarks
        app.state.toggle_bookmark(sub_path);
        assert!(!app.state.is_bookmarked(sub_path));
    }

    #[test]
    fn context_menu_open_with_stability() {
        let (dir, mut app) = fixture();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "sample text").unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Right click row 0
        app.row_right_clicked(
            0,
            optics_ui::Rect {
                x: 100.0,
                y: 100.0,
                w: 10.0,
                h: 10.0,
            },
        );
        assert!(app.ctx_menu.is_some());

        let open_with_snapshot: Vec<String> = app
            .ctx_menu
            .as_ref()
            .unwrap()
            .open_with
            .iter()
            .map(|a| a.id.clone())
            .collect();

        // Build multiple frames while context menu is open; menu content must stay stable
        for _ in 0..5 {
            frame(&mut app, &mut ui, &input);
            assert!(app.ctx_menu.is_some());
            let current_ids: Vec<String> = app
                .ctx_menu
                .as_ref()
                .unwrap()
                .open_with
                .iter()
                .map(|a| a.id.clone())
                .collect();
            assert_eq!(current_ids, open_with_snapshot);
        }
    }

    #[test]
    fn context_menu_escape_dismissal() {
        let (dir, mut app) = fixture();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "sample text").unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Right click row 0
        app.row_right_clicked(
            0,
            optics_ui::Rect {
                x: 100.0,
                y: 100.0,
                w: 10.0,
                h: 10.0,
            },
        );
        assert!(app.ctx_menu.is_some());

        // Frame with menu open
        frame(&mut app, &mut ui, &input);
        assert!(app.ctx_menu.is_some());

        // Press Escape to dismiss
        let mut esc = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);

        assert!(app.ctx_menu.is_none());
    }

    #[test]
    fn sidebar_menu_escape_dismissal() {
        let (dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Open sidebar menu
        app.open_sidebar_menu(
            dir.to_str().unwrap(),
            true,
            optics_ui::Rect {
                x: 50.0,
                y: 50.0,
                w: 10.0,
                h: 10.0,
            },
        );
        assert!(app.sidebar_menu.is_some());

        // Frame with sidebar menu open
        frame(&mut app, &mut ui, &input);
        assert!(app.sidebar_menu.is_some());

        // Press Escape to dismiss
        let mut esc = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);

        assert!(app.sidebar_menu.is_none());
    }
}
