//! Chooser / Picker implementation for portal interactions.
//!
//! Provides the complete UI and interaction layer for
//! `org.freedesktop.impl.portal.FileChooser` requests.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arca_engine::chooser::{
    BytePath, Choice, FileChooserMode, FileChooserRequest, FileChooserResponse, FileFilter,
};
use arca_engine::state::AppState;
use optics_ui::{
    Align, Band, CornerRadii, Frame, Input, LayoutOpts, PlaceMode, PlaceOpts, TextBuf, TextEditOpts,
};

use crate::app::UiApp;
use crate::icons::ids;
use crate::theme::{self, Tones};

pub(crate) const CHOOSER_OVERWRITE_MODAL: &str = "chooser-overwrite-modal";
pub(crate) const CHOOSER_FILTER_DROPDOWN: &str = "chooser-filter-dropdown";

pub struct ChooserState {
    pub request: FileChooserRequest,
    pub save_name: TextBuf,
    pub save_name_id: optics_ui_sys::optics_ui_id,
    pub save_name_focused: bool,
    pub save_name_focused_now: bool,
    pub filter_index: usize,
    pub filters: Vec<FileFilter>,
    pub filter_menu_open: bool,
    pub choices: Vec<(String, String)>,
    pub choice_menus_open: HashMap<String, bool>,
    /// Paths staged by Ctrl/Shift-click in `multiple` mode. When the request
    /// allows it, this — not just the active cursor — is what Accept returns.
    pub selected_paths: BTreeSet<PathBuf>,
    pub overwrite_confirm: Option<PathBuf>,
    pub result: Option<FileChooserResponse>,
    pub done: bool,
}

impl ChooserState {
    pub fn new(request: FileChooserRequest) -> ChooserState {
        let initial_save_name = if let Some(name) = request.current_name.as_deref() {
            name.to_string()
        } else if let Some(file) = &request.current_file {
            file.to_path_buf()
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let save_name = TextBuf::new(1024, &initial_save_name);

        let mut filters = request.filters.clone();
        if filters.is_empty()
            && let Some(cf) = &request.current_filter
        {
            filters.push(cf.clone());
        }

        let filter_index = if let Some(cf) = &request.current_filter {
            filters.iter().position(|f| f == cf).unwrap_or(0)
        } else {
            0
        };

        // Per freedesktop FileChooser spec, an empty `selected` string means
        // "pick a default": `"false"` for boolean checkbuttons, or the first
        // option id for comboboxes. Normalizing upfront guarantees that
        // accepting without editing the control still emits a contract-valid
        // answer.
        let choices = request
            .choices
            .iter()
            .map(|c| {
                let value = if c.options.is_empty() {
                    if c.selected == "true" {
                        "true".to_owned()
                    } else {
                        "false".to_owned()
                    }
                } else if c.selected.is_empty() {
                    c.options
                        .first()
                        .map(|(id, _)| id.clone())
                        .unwrap_or_default()
                } else {
                    c.selected.clone()
                };
                (c.id.clone(), value)
            })
            .collect();

        ChooserState {
            request,
            save_name,
            save_name_id: 0,
            save_name_focused: false,
            save_name_focused_now: false,
            filter_index,
            filters,
            filter_menu_open: false,
            choices,
            choice_menus_open: HashMap::new(),
            selected_paths: BTreeSet::new(),
            overwrite_confirm: None,
            result: None,
            done: false,
        }
    }

    /// Whether the request permits selecting more than one path.
    pub fn allows_multiple(&self) -> bool {
        self.request.multiple
    }

    /// Stage or unstage `path` in multiple-select mode.
    pub fn toggle_selected_path(&mut self, path: PathBuf) {
        if !self.selected_paths.remove(&path) {
            self.selected_paths.insert(path);
        }
    }

    /// The paths Accept should return, most-specific rule first.
    ///
    /// In multiple mode an explicit staged selection wins; otherwise the
    /// cursor's row is the answer. In single mode staging is ignored
    /// entirely, so a request can never receive more paths than it allowed.
    pub fn accept_selection(&self, cursor: Option<PathBuf>) -> Vec<PathBuf> {
        if self.allows_multiple() && !self.selected_paths.is_empty() {
            return self.selected_paths.iter().cloned().collect();
        }
        cursor.into_iter().collect()
    }

    pub fn active_filter(&self) -> Option<FileFilter> {
        self.filters.get(self.filter_index).cloned()
    }

    pub fn accept_paths(&mut self, paths: Vec<PathBuf>) {
        if self.done {
            return;
        }
        let byte_paths: Vec<BytePath> = paths.into_iter().map(BytePath::from_path).collect();
        let current_filter = self.active_filter();
        self.result = Some(FileChooserResponse::Selected {
            paths: byte_paths,
            current_filter,
            choices: self.choices.clone(),
        });
        self.done = true;
    }

    pub fn cancel(&mut self) {
        if self.done {
            return;
        }
        self.result = Some(FileChooserResponse::Cancelled);
        self.done = true;
    }
}

/// Build the top header for Chooser dialog mode (titlebar + close button).
pub(crate) fn build_chooser_header(
    app: &mut UiApp,
    frame: &mut Frame,
    input: &Input,
    tones: &Tones,
) {
    let icons = app.icons;
    let title = app
        .chooser
        .as_ref()
        .map(|c| c.request.title.as_str())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| match app.chooser.as_ref().map(|c| c.request.mode) {
            Some(FileChooserMode::SaveFile | FileChooserMode::SaveFiles) => "Save File",
            Some(FileChooserMode::OpenDirectory) => "Select Folder",
            _ => "Open File",
        });

    let mut close_clicked = false;
    frame.size_next(0.0, 42.0);
    frame
        .row()
        .with_opts(&LayoutOpts {
            height: 42.0,
            pad: 6.0,
            gap: 8.0,
            cross: Align::Center,
            bg: tones.tab_bar,
            ..Default::default()
        })
        .show(|frame| {
            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            icons.icon(frame, ids::Folder, 18.0);
            frame.label(title);

            // Center drag region
            frame.flex(1.0);
            frame.spacer(0.0);

            close_clicked = icons.button(frame, ids::X, 26.0);
        });

    if close_clicked {
        if let Some(chooser) = &mut app.chooser {
            chooser.cancel();
        }
        optics_window::window_close();
    } else {
        let raw_in = input.as_raw();
        if raw_in.mouse_pressed[0] {
            let cursor = raw_in.cursor;
            let display = raw_in.display_size;
            if cursor.y >= 0.0 && cursor.y <= 42.0 && cursor.x >= 0.0 && cursor.x < display.x - 34.0
            {
                optics_window::window_start_move();
            }
        }
    }
}

/// Build the bottom action bar for Chooser mode (filename field, filters, choices, cancel & accept buttons).
pub(crate) fn build_chooser_footer(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let Some(chooser) = &mut app.chooser else {
        return;
    };

    let mode = chooser.request.mode;
    let accept_label = chooser
        .request
        .accept_label
        .clone()
        .unwrap_or_else(|| match mode {
            FileChooserMode::SaveFile | FileChooserMode::SaveFiles => "Save".into(),
            FileChooserMode::OpenDirectory => "Select Folder".into(),
            FileChooserMode::OpenFile => "Open".into(),
        });
    let choice_defs = chooser.request.choices.clone();

    let mut trigger_accepted = false;
    let mut cancel_clicked = false;

    frame
        .column()
        .with_opts(&LayoutOpts {
            pad: 10.0,
            gap: 8.0,
            cross: Align::Stretch,
            bg: tones.toolbar,
            border: tones.popover_border,
            border_width: 1.0,
            ..Default::default()
        })
        .show(|frame| {
            // In SaveFile mode, show save name input row
            if mode == FileChooserMode::SaveFile {
                frame
                    .row()
                    .with_opts(&LayoutOpts {
                        gap: 8.0,
                        cross: Align::Center,
                        ..Default::default()
                    })
                    .show(|frame| {
                        frame.label("File name:");
                        frame.flex(1.0);
                        frame.size_next(0.0, 32.0);
                        let resp = frame.textedit(
                            "##save-name",
                            &mut chooser.save_name,
                            &TextEditOpts {
                                select_all_on_focus: true,
                                submit_on_enter: true,
                                ..Default::default()
                            },
                        );
                        capture_save_name(chooser, frame);
                        if resp.submitted {
                            trigger_accepted = true;
                        }
                    });
            }

            // In SaveFiles mode, show summary of files to be saved
            if mode == FileChooserMode::SaveFiles {
                frame
                    .row()
                    .with_opts(&LayoutOpts {
                        gap: 8.0,
                        cross: Align::Center,
                        ..Default::default()
                    })
                    .show(|frame| {
                        let count = chooser.request.files.len();
                        let text = if count == 1 {
                            "Saving 1 file to selected folder".to_string()
                        } else {
                            format!("Saving {count} files to selected folder")
                        };
                        frame.label(&text);
                    });
            }

            // Choices row (if choices are configured)
            if !choice_defs.is_empty() {
                frame
                    .row()
                    .with_opts(&LayoutOpts {
                        gap: 16.0,
                        cross: Align::Center,
                        ..Default::default()
                    })
                    .show(|frame| {
                        for choice in &choice_defs {
                            build_choice_widget(chooser, frame, choice, tones);
                        }
                    });
            }

            // Action row: Filter dropdown on left, Cancel / Accept buttons on right
            frame
                .row()
                .with_opts(&LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                })
                .show(|frame| {
                    // Filter dropdown
                    if !chooser.filters.is_empty() {
                        let current_label = chooser
                            .filters
                            .get(chooser.filter_index)
                            .map(|f| f.label.as_str())
                            .unwrap_or("All files");
                        let label_btn = format!("{current_label} ▾");
                        if frame.button(&label_btn) {
                            chooser.filter_menu_open = !chooser.filter_menu_open;
                        }
                    }

                    frame.flex(1.0);
                    frame.spacer(0.0);

                    // Cancel button
                    frame.size_next(84.0, 32.0);
                    if frame.button("Cancel") {
                        cancel_clicked = true;
                    }

                    // Accept button
                    frame.size_next(100.0, 32.0);
                    let theme = frame.theme();
                    frame.set_theme(theme.with_bg(tones.selected).with_active(tones.selected));
                    let accepted = frame.button(&accept_label);
                    frame.set_theme(theme);

                    if accepted {
                        trigger_accepted = true;
                    }
                });
        });

    if cancel_clicked {
        if let Some(chooser) = &mut app.chooser {
            chooser.cancel();
        }
        optics_window::window_close();
    } else if trigger_accepted {
        trigger_accept(app);
    }
}

fn capture_save_name(chooser: &mut ChooserState, frame: &mut Frame) {
    let resp = frame.response();
    chooser.save_name_id = resp.id;
    chooser.save_name_focused_now = resp.focused;
}

fn build_choice_widget(
    chooser: &mut ChooserState,
    frame: &mut Frame,
    choice: &Choice,
    tones: &Tones,
) {
    let current_val = chooser
        .choices
        .iter()
        .find(|(id, _)| id == &choice.id)
        .map(|(_, v)| v.as_str())
        .unwrap_or(&choice.selected);

    if choice.options.is_empty() {
        // Boolean check button
        let mut checked = current_val == "true";
        if frame.checkbox(&choice.label, &mut checked) {
            let new_val = if checked { "true" } else { "false" };
            if let Some(pos) = chooser.choices.iter().position(|(id, _)| id == &choice.id) {
                chooser.choices[pos].1 = new_val.to_string();
            } else {
                chooser
                    .choices
                    .push((choice.id.clone(), new_val.to_string()));
            }
        }
    } else {
        // Combobox with options
        let opt_label = choice
            .options
            .iter()
            .find(|(id, _)| id == current_val)
            .map(|(_, l)| l.as_str())
            .unwrap_or(current_val);
        let btn_text = format!("{}: {} ▾", choice.label, opt_label);
        let popup_id = format!("choice-popup-{}", choice.id);
        if frame.button(&btn_text) {
            frame.place_toggle(&popup_id);
        }
        let anchor = frame.response().rect;
        frame.place(
            &popup_id,
            &PlaceOpts {
                mode: PlaceMode::Anchored,
                rect: anchor,
                layout: LayoutOpts {
                    gap: 2.0,
                    pad: 4.0,
                    cross: Align::Stretch,
                    bg: tones.popover,
                    border: tones.popover_border,
                    border_width: 1.0,
                    corner_radii: CornerRadii::uniform(8.0),
                    min_width: anchor.w.max(140.0),
                    ..Default::default()
                },
                transient: true,
                ..Default::default()
            },
            |frame| {
                for (opt_id, opt_label) in &choice.options {
                    if frame.button(opt_label) {
                        if let Some(pos) =
                            chooser.choices.iter().position(|(id, _)| id == &choice.id)
                        {
                            chooser.choices[pos].1 = opt_id.clone();
                        } else {
                            chooser.choices.push((choice.id.clone(), opt_id.clone()));
                        }
                        frame.place_close(&popup_id);
                    }
                }
            },
        );
    }
}

/// Render the filter dropdown popup if active.
pub(crate) fn build_filter_popover(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let Some(chooser) = &mut app.chooser else {
        return;
    };
    if !chooser.filter_menu_open {
        return;
    }

    if frame.place_is_open(CHOOSER_FILTER_DROPDOWN) {
        // Already open
    } else {
        frame.place_open(CHOOSER_FILTER_DROPDOWN);
    }

    let mut selected_idx = None;
    frame.place(
        CHOOSER_FILTER_DROPDOWN,
        &PlaceOpts {
            mode: PlaceMode::Centered,
            band: Band::Modal,
            layout: LayoutOpts {
                gap: 4.0,
                pad: 8.0,
                cross: Align::Stretch,
                bg: tones.popover,
                border: tones.popover_border,
                border_width: 1.0,
                corner_radii: CornerRadii::uniform(12.0),
                min_width: 260.0,
                ..Default::default()
            },
            transient: true,
            ..Default::default()
        },
        |frame| {
            theme::label_colored(frame, "Select Filter", tones.muted);
            for (idx, filter) in chooser.filters.iter().enumerate() {
                if frame.button(&filter.label) {
                    selected_idx = Some(idx);
                }
            }
        },
    );

    if let Some(idx) = selected_idx {
        chooser.filter_index = idx;
        chooser.filter_menu_open = false;
        frame.place_close(CHOOSER_FILTER_DROPDOWN);
        let filter = chooser.active_filter();
        app.state.set_file_filter(filter);
    } else if !frame.place_is_open(CHOOSER_FILTER_DROPDOWN) {
        chooser.filter_menu_open = false;
    }
}

/// Render the overwrite confirmation modal if needed.
pub(crate) fn build_overwrite_modal(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let icons = app.icons;
    let Some(chooser) = &mut app.chooser else {
        return;
    };
    let Some(target) = chooser.overwrite_confirm.clone() else {
        return;
    };

    if !frame.place_is_open(CHOOSER_OVERWRITE_MODAL) {
        frame.place_open(CHOOSER_OVERWRITE_MODAL);
    }

    let target_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut action = None;
    frame.place(
        CHOOSER_OVERWRITE_MODAL,
        &PlaceOpts {
            mode: PlaceMode::Centered,
            band: Band::Modal,
            layout: LayoutOpts {
                gap: 12.0,
                pad: 16.0,
                cross: Align::Stretch,
                bg: tones.card,
                border: tones.popover_border,
                border_width: 1.0,
                corner_radii: CornerRadii::uniform(14.0),
                min_width: 360.0,
                ..Default::default()
            },
            transient: true,
            ..Default::default()
        },
        |frame| {
            frame.row().with_opts(&LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                }).show(|frame| {
                    icons.icon(frame, ids::AlertCircle, 24.0);
                    frame.label("Replace existing file?");
                });

            frame.label(&format!(
                "A file named \"{target_name}\" already exists. Replacing it will overwrite its contents."
            ));

            frame.row().with_opts(&LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                }).show(|frame| {
                    frame.flex(1.0);
                    frame.spacer(0.0);

                    if frame.button("Cancel") {
                        action = Some(false);
                    }

                    frame.size_next(90.0, 32.0);
                    let theme = frame.theme();
                    frame.set_theme(theme.with_bg(tones.selected));
                    if frame.button("Replace") {
                        action = Some(true);
                    }
                    frame.set_theme(theme);
                });
        },
    );

    match action {
        Some(true) => {
            frame.place_close(CHOOSER_OVERWRITE_MODAL);
            let Some(chooser) = &mut app.chooser else {
                return;
            };
            chooser.overwrite_confirm = None;
            chooser.accept_paths(vec![target]);
            optics_window::window_close();
        }
        Some(false) => {
            frame.place_close(CHOOSER_OVERWRITE_MODAL);
            let Some(chooser) = &mut app.chooser else {
                return;
            };
            chooser.overwrite_confirm = None;
        }
        None => {
            if !frame.place_is_open(CHOOSER_OVERWRITE_MODAL) {
                let Some(chooser) = &mut app.chooser else {
                    return;
                };
                chooser.overwrite_confirm = None;
            }
        }
    }
}

/// Trigger accept logic depending on current chooser mode.
pub(crate) fn trigger_accept(app: &mut UiApp) {
    let Some(chooser) = &mut app.chooser else {
        return;
    };

    match chooser.request.mode {
        FileChooserMode::OpenFile => {
            // A directory row means "descend", never "accept": a picker must
            // not return a folder for an OpenFile request.
            if let Some(entry) = app.state.selected_entry().filter(|e| e.navigable) {
                let name = entry.name.clone();
                app.state.open_child(&name);
                return;
            }
            let cursor = app
                .state
                .selected_entry()
                .filter(|entry| !entry.navigable)
                .map(|entry| Path::new(app.state.cwd()).join(&entry.name));
            let paths = chooser.accept_selection(cursor);
            if paths.is_empty() {
                return;
            }
            chooser.accept_paths(paths);
            optics_window::window_close();
        }
        FileChooserMode::OpenDirectory => {
            let paths = if chooser.allows_multiple() && !chooser.selected_paths.is_empty() {
                let staged: Vec<PathBuf> = chooser
                    .selected_paths
                    .iter()
                    .filter(|p| p.is_dir())
                    .cloned()
                    .collect();
                if !staged.is_empty() {
                    staged
                } else {
                    vec![PathBuf::from(app.state.cwd())]
                }
            } else {
                let path = if let Some(entry) = app.state.selected_entry() {
                    if entry.navigable {
                        Path::new(app.state.cwd()).join(&entry.name)
                    } else {
                        PathBuf::from(app.state.cwd())
                    }
                } else {
                    PathBuf::from(app.state.cwd())
                };
                vec![path]
            };
            chooser.accept_paths(paths);
            optics_window::window_close();
        }
        FileChooserMode::SaveFile => {
            let mut name = chooser.save_name.as_str().trim().to_string();
            if name.is_empty() || name.contains('/') || name.contains('\0') {
                return;
            }
            if !name.contains('.')
                && let Some(filter) = chooser.active_filter()
            {
                for rule in &filter.rules {
                    if rule.kind == arca_engine::chooser::FilterRuleKind::Glob
                        && let Some(ext) = rule.value.strip_prefix("*.")
                        && !ext.contains('*')
                        && !ext.contains('?')
                    {
                        name = format!("{name}.{ext}");
                        break;
                    }
                }
            }
            let target = Path::new(app.state.cwd()).join(&name);
            if target.exists() {
                chooser.overwrite_confirm = Some(target);
            } else {
                chooser.accept_paths(vec![target]);
                optics_window::window_close();
            }
        }
        FileChooserMode::SaveFiles => {
            let folder = if let Some(entry) = app.state.selected_entry() {
                if entry.navigable {
                    Path::new(app.state.cwd()).join(&entry.name)
                } else {
                    PathBuf::from(app.state.cwd())
                }
            } else {
                PathBuf::from(app.state.cwd())
            };
            match chooser.request.finish_paths(vec![folder]) {
                Ok(paths) => {
                    chooser.accept_paths(paths);
                    optics_window::window_close();
                }
                Err(e) => {
                    app.state.set_status(format!("Save error: {e}"));
                }
            }
        }
    }
}

/// Run Arca in FileChooser portal mode.
pub fn run_chooser(
    request: FileChooserRequest,
) -> Result<FileChooserResponse, optics_app::RunError> {
    let start_dir = if let Some(folder) = &request.current_folder {
        folder.to_path_buf().to_string_lossy().into_owned()
    } else if let Some(file) = &request.current_file {
        file.to_path_buf()
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(arca_engine::path::home_dir)
    } else {
        arca_engine::path::home_dir()
    };

    let config_file = arca_engine::config::config_path();
    let cfg = arca_engine::config::load(&config_file);

    let mut state = AppState::with_config(config_file, cfg, &start_dir);
    let chooser_state = ChooserState::new(request.clone());
    if let Some(filter) = chooser_state.active_filter() {
        state.set_file_filter(Some(filter));
    }

    let result_holder = Arc::new(Mutex::new(None));
    let result_clone = result_holder.clone();

    let mut app = UiApp::new(state).sync_navigation();
    app.chooser = Some(chooser_state);

    let window_title = if !request.title.is_empty() {
        request.title.clone()
    } else {
        match request.mode {
            FileChooserMode::SaveFile | FileChooserMode::SaveFiles => "Save File".into(),
            FileChooserMode::OpenDirectory => "Select Folder".into(),
            FileChooserMode::OpenFile => "Open File".into(),
        }
    };

    let app_id = if !request.app_id.is_empty() {
        request.app_id.clone()
    } else {
        "io.arca.Chooser".into()
    };

    let config = optics_app::Config::new(&window_title)?
        .app_id(&app_id)?
        .size(960, 640);

    let app = std::rc::Rc::new(std::cell::RefCell::new(app));
    let app_start = app.clone();
    let app_build = app.clone();
    let app_stop = app.clone();

    optics_app::Application::run_with_lifecycle(
        config,
        Some(move |host: optics_app::StartHost| {
            crate::device::set_device(host.optics_gfx_device().as_raw());
            app_start.borrow_mut().start(&mut host.ui());
            true
        }),
        Some(move |host: optics_app::StopHost| {
            app_stop.borrow_mut().shutdown(&mut host.ui());
            crate::device::clear_device();
        }),
        move |frame, input| {
            let mut app = app_build.borrow_mut();
            app.build(frame, input);
            if let Some(chooser) = &app.chooser
                && chooser.done
                && chooser.result.is_some()
            {
                *result_clone.lock().unwrap() = chooser.result.clone();
            }
        },
        // Pure optics-ui chrome: no host canvas recording (Optics ADR-0127).
        None::<fn(optics_app::RecordHost)>,
    )?;

    let res = result_holder
        .lock()
        .unwrap()
        .take()
        .unwrap_or(FileChooserResponse::Cancelled);
    Ok(res)
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use super::*;
    use crate::test_support::{fixture, fixture_two_files, frame};

    #[test]
    fn chooser_renders_headless_open_mode() {
        let (dir, mut app) = fixture();
        let req = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::OpenFile,
            app_id: "org.test.App".into(),
            title: "Select Document".into(),
            accept_label: Some("Pick".into()),
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: Some(arca_engine::chooser::BytePath::from_path(dir.path())),
            current_name: None,
            current_file: None,
            filters: vec![arca_engine::chooser::FileFilter::new(
                "Text Files (*.txt)",
                vec![arca_engine::chooser::FilterRule {
                    kind: arca_engine::chooser::FilterRuleKind::Glob,
                    value: "*.txt".into(),
                }],
            )],
            current_filter: None,
            choices: vec![arca_engine::chooser::Choice {
                id: "read_only".into(),
                label: "Read only".into(),
                options: Vec::new(),
                selected: "false".into(),
            }],
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(req));
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        for _ in 0..3 {
            frame(&mut app, &mut ui, &input);
        }

        assert!(app.chooser.is_some());
        assert!(!app.chooser.as_ref().unwrap().done);
    }

    #[test]
    fn chooser_renders_headless_save_mode_and_accepts() {
        let (dir, mut app) = fixture();
        let req = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::SaveFile,
            app_id: "org.test.App".into(),
            title: "Save Report".into(),
            accept_label: Some("Save Report".into()),
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: Some(arca_engine::chooser::BytePath::from_path(dir.path())),
            current_name: Some("report.pdf".into()),
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(req));
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        assert_eq!(
            app.chooser.as_ref().unwrap().save_name.as_str(),
            "report.pdf"
        );

        // Press Return to accept
        let mut enter = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        enter.push_key(optics_ui::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter);

        let res = app.chooser.as_ref().unwrap().result.as_ref().unwrap();
        match res {
            arca_engine::chooser::FileChooserResponse::Selected { paths, .. } => {
                assert_eq!(paths.len(), 1);
                assert_eq!(paths[0].to_path_buf(), dir.join("report.pdf"));
            }
            _ => panic!("Expected Selected response, got {res:?}"),
        }
    }

    #[test]
    fn chooser_save_overwrite_modal_triggers() {
        let (dir, mut app) = fixture();
        let req = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::SaveFile,
            app_id: "org.test.App".into(),
            title: "Save File".into(),
            accept_label: None,
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: Some(arca_engine::chooser::BytePath::from_path(dir.path())),
            current_name: Some("a.txt".into()), // "a.txt" already exists in fixture!
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(req));
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let input = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        // Trigger accept (Return key)
        let mut enter = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        enter.push_key(optics_ui::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter);

        // Since a.txt exists, overwrite_confirm modal should be set
        assert_eq!(
            app.chooser.as_ref().unwrap().overwrite_confirm,
            Some(dir.join("a.txt"))
        );
        assert!(!app.chooser.as_ref().unwrap().done);

        // Escape dismisses the overwrite modal without cancelling the chooser
        let mut esc = optics_ui::Input::new((960.0, 640.0), 1.0 / 60.0);
        esc.push_key(optics_ui::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);

        assert_eq!(app.chooser.as_ref().unwrap().overwrite_confirm, None);
        assert!(!app.chooser.as_ref().unwrap().done);

        // Another Escape cancels the chooser
        frame(&mut app, &mut ui, &esc);
        assert_eq!(
            app.chooser.as_ref().unwrap().result,
            Some(arca_engine::chooser::FileChooserResponse::Cancelled)
        );
    }

    /// A `multiple` OpenFile request must return every staged path. The
    /// staged set (`ChooserState::selected_paths`) was never written to, so
    /// the picker silently degraded to single selection and the
    /// `--choose-files` / portal `multiple: true` contracts were broken.
    #[test]
    fn chooser_multiple_returns_every_staged_path() {
        let (_dir, mut app) = fixture_two_files();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        let request = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::OpenFile,
            app_id: "io.arca.Test".into(),
            title: "Pick files".into(),
            accept_label: None,
            modal: true,
            parent_window: None,
            multiple: true,
            current_folder: None,
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(request));
        frame(&mut app, &mut ui, &settle);

        // Stage the two files (directories sort first, so rows 1 and 2).
        app.state.select_visible(1);
        app.row_clicked(1, "a.txt");
        app.row_clicked(2, "b.txt");

        trigger_accept(&mut app);

        let result = app
            .chooser
            .as_ref()
            .and_then(|c| c.result.clone())
            .expect("a response must be produced");
        match result {
            arca_engine::chooser::FileChooserResponse::Selected { paths, .. } => {
                let names: Vec<String> = paths
                    .iter()
                    .map(|p| {
                        p.to_path_buf()
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                assert_eq!(names, vec!["a.txt", "b.txt"], "both staged paths return");
            }
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    /// Double-clicking accepts the *whole* staged selection in multiple
    /// mode — the fast path must not silently discard the other rows the
    /// user had already clicked.
    #[test]
    fn chooser_double_click_accepts_the_staged_selection() {
        let (_dir, mut app) = fixture_two_files();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let request = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::OpenFile,
            app_id: "io.arca.Test".into(),
            title: "Pick files".into(),
            accept_label: None,
            modal: true,
            parent_window: None,
            multiple: true,
            current_folder: None,
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(request));
        frame(
            &mut app,
            &mut ui,
            &optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0),
        );

        // Stage both files, then double-click the second one.
        app.row_clicked(1, "a.txt");
        app.row_clicked(2, "b.txt");
        app.row_clicked(2, "b.txt");

        let result = app
            .chooser
            .as_ref()
            .and_then(|chooser| chooser.result.clone())
            .expect("a double-click must accept");
        match result {
            arca_engine::chooser::FileChooserResponse::Selected { paths, .. } => {
                assert_eq!(
                    paths.len(),
                    2,
                    "double-click must accept every staged path, got {paths:?}"
                );
            }
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    /// The same request with `multiple: false` must never return more than
    /// one path, even if several rows carry a highlight.
    #[test]
    fn chooser_single_returns_only_the_cursor() {
        let (_dir, mut app) = fixture();
        let mut ui = optics_ui::Ui::headless().expect("headless ui");
        let settle = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
        let request = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::OpenFile,
            app_id: "io.arca.Test".into(),
            title: "Pick one".into(),
            accept_label: None,
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: None,
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: Vec::new(),
            files: Vec::new(),
        };
        app.chooser = Some(ChooserState::new(request));
        app.state.select_all();
        app.state.select_visible(1);
        frame(&mut app, &mut ui, &settle);

        trigger_accept(&mut app);

        let result = app
            .chooser
            .as_ref()
            .and_then(|c| c.result.clone())
            .expect("a response must be produced");
        match result {
            arca_engine::chooser::FileChooserResponse::Selected { paths, .. } => {
                assert_eq!(paths.len(), 1, "single mode must return exactly one path");
                assert!(paths[0].to_path_buf().ends_with("a.txt"));
            }
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    #[test]
    fn chooser_normalizes_empty_initial_choice_values_for_validation() {
        let req = arca_engine::chooser::FileChooserRequest {
            mode: arca_engine::chooser::FileChooserMode::OpenFile,
            app_id: "io.arca.Test".into(),
            title: "Open".into(),
            accept_label: None,
            modal: true,
            parent_window: None,
            multiple: false,
            current_folder: None,
            current_name: None,
            current_file: None,
            filters: Vec::new(),
            current_filter: None,
            choices: vec![
                Choice {
                    id: "read_only".into(),
                    label: "Read only".into(),
                    options: Vec::new(),
                    selected: String::new(),
                },
                Choice {
                    id: "encoding".into(),
                    label: "Encoding".into(),
                    options: vec![
                        ("utf8".into(), "UTF-8".into()),
                        ("latin1".into(), "Latin-1".into()),
                    ],
                    selected: String::new(),
                },
            ],
            files: Vec::new(),
        };
        let mut state = ChooserState::new(req.clone());
        state.accept_paths(vec![PathBuf::from("/tmp/example.txt")]);
        let response = state.result.expect("response recorded");
        assert!(
            response.validate_for(&req).is_ok(),
            "normalized empty initial choices must satisfy contract validation: {response:?}"
        );
    }
}
