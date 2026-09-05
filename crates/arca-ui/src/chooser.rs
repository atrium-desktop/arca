//! Chooser / Picker implementation for portal interactions.
//!
//! Provides the complete UI and interaction layer for
//! `org.freedesktop.impl.portal.FileChooser` requests.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use iris::{Align, Band, Frame, Input, LayoutOpts, PlaceMode, PlaceOpts, TextBuf};
use arca_engine::chooser::{
    BytePath, Choice, FileChooserMode, FileChooserRequest, FileChooserResponse, FileFilter,
    PromptAppearance, PromptColorScheme,
};
use arca_engine::state::AppState;

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) const CHOOSER_OVERWRITE_MODAL: &str = "chooser-overwrite-modal";
pub(crate) const CHOOSER_FILTER_DROPDOWN: &str = "chooser-filter-dropdown";

pub struct ChooserState {
    pub request: FileChooserRequest,
    pub appearance: Option<PromptAppearance>,
    pub save_name: TextBuf,
    pub save_name_id: lens_sys::lens_id,
    pub save_name_focused: bool,
    pub save_name_focused_now: bool,
    pub filter_index: usize,
    pub filters: Vec<FileFilter>,
    pub filter_menu_open: bool,
    pub choices: Vec<(String, String)>,
    pub choice_menus_open: HashMap<String, bool>,
    pub selected_paths: BTreeSet<PathBuf>,
    pub overwrite_confirm: Option<PathBuf>,
    pub result: Option<FileChooserResponse>,
    pub done: bool,
}

impl ChooserState {
    pub fn new(
        request: FileChooserRequest,
        appearance: Option<PromptAppearance>,
    ) -> ChooserState {
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
        if filters.is_empty() {
            if let Some(cf) = &request.current_filter {
                filters.push(cf.clone());
            }
        }

        let filter_index = if let Some(cf) = &request.current_filter {
            filters.iter().position(|f| f == cf).unwrap_or(0)
        } else {
            0
        };

        let choices = request
            .choices
            .iter()
            .map(|c| (c.id.clone(), c.selected.clone()))
            .collect();

        ChooserState {
            request,
            appearance,
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
    frame.row_ex(
        &LayoutOpts {
            height: 42.0,
            pad: 6.0,
            gap: 8.0,
            cross: Align::Center,
            bg: tones.tab_bar,
            ..Default::default()
        },
        |frame| {
            frame.size_next(4.0, 0.0);
            frame.spacer(4.0);

            icons::icon(frame, ids::Folder, 18.0);
            frame.label_compact(title);

            // Center drag region
            frame.flex(1.0);
            frame.spacer(0.0);

            close_clicked = icons::icon_button(frame, ids::X, 26.0);
        },
    );

    if close_clicked {
        if let Some(chooser) = &mut app.chooser {
            chooser.cancel();
        }
        iris::window_close();
    } else {
        let raw_in = input.as_raw();
        if raw_in.mouse_pressed[0] {
            let cursor = raw_in.cursor;
            let display = raw_in.display_size;
            if cursor.y >= 0.0 && cursor.y <= 42.0 && cursor.x >= 0.0 && cursor.x < display.x - 34.0 {
                iris::window_start_move();
            }
        }
    }
}

/// Build the bottom action bar for Chooser mode (filename field, filters, choices, cancel & accept buttons).
pub(crate) fn build_chooser_footer(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
) {
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

    frame.column_ex(
        &LayoutOpts {
            pad: 10.0,
            gap: 8.0,
            cross: Align::Stretch,
            bg: tones.toolbar,
            border: tones.popover_border,
            border_width: 1.0,
            ..Default::default()
        },
        |frame| {
            // In SaveFile mode, show save name input row
            if mode == FileChooserMode::SaveFile {
                frame.row_ex(
                    &LayoutOpts {
                        gap: 8.0,
                        cross: Align::Center,
                        ..Default::default()
                    },
                    |frame| {
                        frame.label_compact("File name:");
                        frame.flex(1.0);
                        frame.size_next(0.0, 32.0);
                        frame.textfield("##save-name", &mut chooser.save_name);
                        capture_save_name(chooser, frame);
                    },
                );
            }

            // In SaveFiles mode, show summary of files to be saved
            if mode == FileChooserMode::SaveFiles {
                frame.row_ex(
                    &LayoutOpts {
                        gap: 8.0,
                        cross: Align::Center,
                        ..Default::default()
                    },
                    |frame| {
                        let count = chooser.request.files.len();
                        let text = if count == 1 {
                            "Saving 1 file to selected folder".to_string()
                        } else {
                            format!("Saving {count} files to selected folder")
                        };
                        frame.label_compact(&text);
                    },
                );
            }

            // Choices row (if choices are configured)
            if !choice_defs.is_empty() {
                frame.row_ex(
                    &LayoutOpts {
                        gap: 16.0,
                        cross: Align::Center,
                        ..Default::default()
                    },
                    |frame| {
                        for choice in &choice_defs {
                            build_choice_widget(chooser, frame, choice, tones);
                        }
                    },
                );
            }

            // Action row: Filter dropdown on left, Cancel / Accept buttons on right
            frame.row_ex(
                &LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
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
                },
            );
        },
    );

    if cancel_clicked {
        if let Some(chooser) = &mut app.chooser {
            chooser.cancel();
        }
        iris::window_close();
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
                chooser.choices.push((choice.id.clone(), new_val.to_string()));
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
                    radius: 8.0,
                    min_width: anchor.w.max(140.0),
                    ..Default::default()
                },
                transient: true,
                ..Default::default()
            },
            |frame| {
                for (opt_id, opt_label) in &choice.options {
                    if frame.button(opt_label) {
                        if let Some(pos) = chooser.choices.iter().position(|(id, _)| id == &choice.id) {
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
pub(crate) fn build_filter_popover(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
) {
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
                radius: 12.0,
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
pub(crate) fn build_overwrite_modal(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
) {
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
                radius: 14.0,
                min_width: 360.0,
                ..Default::default()
            },
            transient: true,
            ..Default::default()
        },
        |frame| {
            frame.row_ex(
                &LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
                    icons::icon(frame, ids::AlertCircle, 24.0);
                    frame.label_compact("Replace existing file?");
                },
            );

            frame.label(&format!(
                "A file named \"{target_name}\" already exists. Replacing it will overwrite its contents."
            ));

            frame.row_ex(
                &LayoutOpts {
                    gap: 8.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
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
                },
            );
        },
    );

    match action {
        Some(true) => {
            frame.place_close(CHOOSER_OVERWRITE_MODAL);
            let Some(chooser) = &mut app.chooser else { return };
            chooser.overwrite_confirm = None;
            chooser.accept_paths(vec![target]);
            iris::window_close();
        }
        Some(false) => {
            frame.place_close(CHOOSER_OVERWRITE_MODAL);
            let Some(chooser) = &mut app.chooser else { return };
            chooser.overwrite_confirm = None;
        }
        None => {
            if !frame.place_is_open(CHOOSER_OVERWRITE_MODAL) {
                let Some(chooser) = &mut app.chooser else { return };
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
            if let Some(entry) = app.state.selected_entry() {
                if entry.navigable {
                    // Navigate into directory
                    let name = entry.name.clone();
                    app.state.open_child(&name);
                } else {
                    let path = Path::new(app.state.cwd()).join(&entry.name);
                    chooser.accept_paths(vec![path]);
                    iris::window_close();
                }
            } else if !chooser.selected_paths.is_empty() {
                let paths: Vec<PathBuf> = chooser.selected_paths.iter().cloned().collect();
                chooser.accept_paths(paths);
                iris::window_close();
            }
        }
        FileChooserMode::OpenDirectory => {
            let path = if let Some(entry) = app.state.selected_entry() {
                if entry.navigable {
                    Path::new(app.state.cwd()).join(&entry.name)
                } else {
                    PathBuf::from(app.state.cwd())
                }
            } else {
                PathBuf::from(app.state.cwd())
            };
            chooser.accept_paths(vec![path]);
            iris::window_close();
        }
        FileChooserMode::SaveFile => {
            let mut name = chooser.save_name.as_str().trim().to_string();
            if name.is_empty() || name.contains('/') || name.contains('\0') {
                return;
            }
            if !name.contains('.') {
                if let Some(filter) = chooser.active_filter() {
                    for rule in &filter.rules {
                        if rule.kind == arca_engine::chooser::FilterRuleKind::Glob {
                            if let Some(ext) = rule.value.strip_prefix("*.") {
                                if !ext.contains('*') && !ext.contains('?') {
                                    name = format!("{name}.{ext}");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            let target = Path::new(app.state.cwd()).join(&name);
            if target.exists() {
                chooser.overwrite_confirm = Some(target);
            } else {
                chooser.accept_paths(vec![target]);
                iris::window_close();
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
                    iris::window_close();
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
    appearance: Option<PromptAppearance>,
) -> Result<FileChooserResponse, iris::RunError> {
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
    let mut cfg = arca_engine::config::load(&config_file);
    if let Some(appr) = &appearance {
        match appr.color_scheme {
            PromptColorScheme::Dark => cfg.theme = arca_engine::ThemeMode::Dark,
            PromptColorScheme::Light => cfg.theme = arca_engine::ThemeMode::Light,
            PromptColorScheme::System => cfg.theme = arca_engine::ThemeMode::System,
        }
    }

    let mut state = AppState::with_config(config_file, cfg, &start_dir);
    let chooser_state = ChooserState::new(request.clone(), appearance);
    if let Some(filter) = chooser_state.active_filter() {
        state.set_file_filter(Some(filter));
    }

    let result_holder = Arc::new(Mutex::new(None));
    let result_clone = result_holder.clone();

    let mut app = UiApp::new(state);
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

    let config = iris::Config::new(&window_title)?
        .app_id(&app_id)?
        .size(960, 640);

    iris::Application::run_with_start(
        config,
        |host| {
            crate::device::set_device(host.flux_device().as_raw() as *mut std::ffi::c_void);
            true
        },
        move |frame, input| {
            app.build(frame, input);
            if let Some(chooser) = &app.chooser {
                if chooser.done && chooser.result.is_some() {
                    *result_clone.lock().unwrap() = chooser.result.clone();
                }
            }
        },
        None::<fn(iris::PaintHost)>,
    )?;

    let res = result_holder
        .lock()
        .unwrap()
        .take()
        .unwrap_or(FileChooserResponse::Cancelled);
    Ok(res)
}
