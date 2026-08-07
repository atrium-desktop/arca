//! Three production content presentations: grid, detailed list and Miller
//! columns. All share the same selection and file-operation model.

use iris::{Align, Color, Frame, LayoutOpts};
use lantern_core::entry::{FileType, SortKey};
use lantern_core::format::{format_size, format_time};
use lantern_core::{Entry, ViewMode};

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

const GRID_CARD_WIDTH: f32 = 136.0;
const GRID_CARD_HEIGHT: f32 = 120.0;

pub(crate) fn build_content(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.flex(1.0);
    frame.column_ex(
        &LayoutOpts {
            flex: 1.0,
            gap: 8.0,
            pad: 12.0,
            cross: Align::Stretch,
            bg: tones.content,
            ..Default::default()
        },
        |frame| {
            build_directory_heading(app, frame, tones);

            if let Some(error) = app.state.read_error().map(str::to_string) {
                placeholder(
                    frame,
                    tones,
                    ids::LENS_ICON_ALERT_CIRCLE,
                    "Cannot read this folder",
                    &error,
                );
                return;
            }

            let visible = app.state.visible();
            if visible.is_empty() {
                if app.state.filter().is_empty() {
                    placeholder(
                        frame,
                        tones,
                        ids::LENS_ICON_FOLDER,
                        "This folder is empty",
                        "Drop files here or create a new folder",
                    );
                } else {
                    placeholder(
                        frame,
                        tones,
                        ids::LENS_ICON_SEARCH,
                        "No matching files",
                        "Try a broader search",
                    );
                }
                return;
            }

            match app.state.view_mode {
                ViewMode::Grid => build_grid(app, frame, tones, &visible),
                ViewMode::List => build_list(app, frame, tones, &visible),
                ViewMode::Miller => build_miller(app, frame, tones),
            }
        },
    );
}

fn build_directory_heading(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let title = std::path::Path::new(app.state.cwd())
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".into());
    let count = app.state.visible().len();
    frame.size_next(0.0, 40.0);
    frame.row_ex(
        &LayoutOpts {
            height: 40.0,
            gap: 12.0,
            pad: 4.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            frame.heading(&title, 2);
            theme::label_colored_sized(
                frame,
                &format!("{count} item{}", if count == 1 { "" } else { "s" }),
                12.0,
                tones.muted,
            );
            frame.flex(1.0);
            if app.state.view_mode != ViewMode::List {
                sort_link(app, frame, SortKey::Name, "Name");
                sort_link(app, frame, SortKey::Size, "Size");
                sort_link(app, frame, SortKey::Mtime, "Modified");
            }
        },
    );
}

fn build_grid(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    let columns = app.grid_columns();
    frame.flex(1.0);
    frame.scroll("grid-files", |frame| {
        frame.column_ex(
            &LayoutOpts {
                gap: 10.0,
                pad: 2.0,
                cross: Align::Stretch,
                ..Default::default()
            },
            |frame| {
                for (row, chunk) in visible.chunks(columns).enumerate() {
                    frame.push_id(&format!("grid-row-{row}"));
                    frame.row_ex(
                        &LayoutOpts {
                            gap: 10.0,
                            cross: Align::Center,
                            ..Default::default()
                        },
                        |frame| {
                            for (column, &entry_index) in chunk.iter().enumerate() {
                                let visible_index = row * columns + column;
                                build_grid_card(app, frame, tones, entry_index, visible_index);
                            }
                        },
                    );
                    frame.pop_id();
                }
            },
        );
    });
}

fn build_grid_card(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
    entry_index: usize,
    visible_index: usize,
) {
    let entry = app.state.entries()[entry_index].clone();
    let selected = app.state.selected() == Some(visible_index);
    let options = LayoutOpts {
        width: GRID_CARD_WIDTH,
        height: GRID_CARD_HEIGHT,
        gap: 0.0,
        pad: 10.0,
        cross: Align::Center,
        bg: if selected {
            tones.selected
        } else {
            Color::TRANSPARENT
        },
        radius: 10.0,
        ..Default::default()
    };

    frame.push_id(&entry.name);
    if app.renaming.as_deref() == Some(entry.name.as_str()) {
        frame.column_ex(&options, |frame| {
            frame.size_next(52.0, 52.0);
            icons::icon(frame, icons::entry_icon(&entry), 46.0);
            frame.size_next(0.0, 8.0);
            frame.spacer(8.0);
            frame.size_next(GRID_CARD_WIDTH - 20.0, 0.0);
            frame.textfield("##grid-rename", &mut app.rename);
            capture_rename(app, frame);
        });
        frame.pop_id();
        return;
    }

    let (response, ()) = frame.pressable_row(
        &format!("grid-{}", entry.name),
        &entry.name,
        &options,
        |frame, _| {
            frame.column_ex(
                &LayoutOpts {
                    width: GRID_CARD_WIDTH - 20.0,
                    gap: 7.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
                    frame.size_next(54.0, 54.0);
                    icons::icon(frame, icons::entry_icon(&entry), 48.0);
                    frame.label_wrapped_sized(&entry.name, 12.5, GRID_CARD_WIDTH - 22.0);
                },
            );
        },
    );
    frame.pop_id();
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(frame, visible_index, response.rect);
    }
}

fn build_list(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    build_list_header(app, frame, tones);
    frame.flex(1.0);
    frame.scroll("list-files", |frame| {
        frame.column_ex(
            &LayoutOpts {
                gap: 2.0,
                pad: 2.0,
                cross: Align::Stretch,
                ..Default::default()
            },
            |frame| {
                for (visible_index, &entry_index) in visible.iter().enumerate() {
                    build_list_row(app, frame, tones, entry_index, visible_index);
                }
            },
        );
    });
}

fn build_list_header(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.size_next(0.0, 32.0);
    frame.row_ex(
        &LayoutOpts {
            height: 32.0,
            gap: 10.0,
            pad: 7.0,
            cross: Align::Center,
            bg: tones.card,
            radius: 7.0,
            ..Default::default()
        },
        |frame| {
            frame.size_next(24.0, 16.0);
            frame.label("");
            frame.flex(1.0);
            sort_link(app, frame, SortKey::Name, "Name");
            frame.size_next(86.0, 16.0);
            sort_link(app, frame, SortKey::Size, "Size");
            frame.size_next(124.0, 16.0);
            sort_link(app, frame, SortKey::Mtime, "Modified");
        },
    );
}

fn build_list_row(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
    entry_index: usize,
    visible_index: usize,
) {
    let entry = app.state.entries()[entry_index].clone();
    let size = if entry.file_type == FileType::Directory {
        "—".into()
    } else {
        format_size(entry.size)
    };
    let modified = format_time(entry.mtime);
    let options = LayoutOpts {
        height: 42.0,
        gap: 10.0,
        pad: 7.0,
        cross: Align::Center,
        bg: if app.state.selected() == Some(visible_index) {
            tones.selected
        } else {
            Color::TRANSPARENT
        },
        radius: 7.0,
        ..Default::default()
    };

    frame.push_id(&entry.name);
    if app.renaming.as_deref() == Some(entry.name.as_str()) {
        frame.row_ex(&options, |frame| {
            row_icon(frame, &entry);
            frame.flex(1.0);
            frame.textfield("##list-rename", &mut app.rename);
            capture_rename(app, frame);
            metadata_label(frame, tones, &size, 86.0);
            metadata_label(frame, tones, &modified, 124.0);
        });
        frame.pop_id();
        return;
    }

    let (response, ()) = frame.pressable_row(&entry.name, &entry.name, &options, |frame, _| {
        row_icon(frame, &entry);
        frame.flex(1.0);
        frame.label(&entry.name);
        metadata_label(frame, tones, &size, 86.0);
        metadata_label(frame, tones, &modified, 124.0);
    });
    frame.pop_id();
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(frame, visible_index, response.rect);
    }
}

fn build_miller(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let all_columns = app.state.miller_columns();
    let available = (app.viewport.0 - 212.0 - 48.0).max(240.0);
    let fit = if available < 500.0 {
        1
    } else {
        (available / 248.0).floor().max(2.0) as usize
    };
    let first = all_columns.len().saturating_sub(fit);
    let columns = all_columns[first..].to_vec();
    let column_width = ((available - 8.0 * columns.len().saturating_sub(1) as f32)
        / columns.len().max(1) as f32)
        .max(220.0);
    let column_height = (app.viewport.1 - 200.0).max(280.0);
    let active_path = app.state.cwd().to_string();
    let seed_scroll = app.miller_seeded_for.as_deref() != Some(active_path.as_str());
    let visible = app.state.visible();
    frame.flex(1.0);
    frame.scroll("miller-columns", |frame| {
        frame.row_ex(
            &LayoutOpts {
                gap: 8.0,
                pad: 2.0,
                cross: Align::Stretch,
                ..Default::default()
            },
            |frame| {
                for (column_index, column) in columns.iter().enumerate() {
                    let current = column.path == active_path;
                    frame.push_id(&format!("miller-column-{column_index}"));
                    frame.size_next(column_width, column_height);
                    frame.column_ex(
                        &LayoutOpts {
                            width: column_width,
                            height: column_height,
                            min_height: 280.0,
                            gap: 3.0,
                            pad: 7.0,
                            cross: Align::Stretch,
                            bg: if current { tones.card } else { tones.content },
                            radius: 9.0,
                            ..Default::default()
                        },
                        |frame| {
                            miller_header(frame, tones, &column.path, current);
                            frame.flex(1.0);
                            let scroll_id = format!("miller-list-{column_index}");
                            frame.scroll(&scroll_id, |frame| {
                                frame.column_ex(
                                    &LayoutOpts {
                                        gap: 2.0,
                                        cross: Align::Stretch,
                                        ..Default::default()
                                    },
                                    |frame| {
                                        if current {
                                            for (visible_index, &entry_index) in
                                                visible.iter().enumerate()
                                            {
                                                if let Some(entry) = column.entries.get(entry_index)
                                                {
                                                    miller_row(
                                                        app,
                                                        frame,
                                                        tones,
                                                        &column.path,
                                                        entry,
                                                        true,
                                                        Some(visible_index),
                                                        column.selected == Some(entry_index),
                                                    );
                                                }
                                            }
                                        } else {
                                            for (entry_index, entry) in
                                                column.entries.iter().enumerate()
                                            {
                                                miller_row(
                                                    app,
                                                    frame,
                                                    tones,
                                                    &column.path,
                                                    entry,
                                                    false,
                                                    None,
                                                    column.selected == Some(entry_index),
                                                );
                                            }
                                        }
                                    },
                                );
                            });
                            if seed_scroll && !current {
                                if let Some(selected) = column.selected {
                                    let offset = (selected as f32 * 38.0 - 260.0).max(0.0);
                                    frame.scroll_to(&scroll_id, 0.0, offset);
                                }
                            }
                        },
                    );
                    frame.pop_id();
                }
            },
        );
    });
    app.miller_seeded_for = Some(active_path);
}

#[allow(clippy::too_many_arguments)]
fn miller_row(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
    directory: &str,
    entry: &Entry,
    current: bool,
    visible_index: Option<usize>,
    selected: bool,
) {
    let options = LayoutOpts {
        height: 36.0,
        gap: 7.0,
        pad: 6.0,
        cross: Align::Center,
        bg: if selected {
            tones.selected
        } else {
            Color::TRANSPARENT
        },
        radius: 6.0,
        ..Default::default()
    };
    frame.push_id(&entry.name);
    let (response, ()) = frame.pressable_row(
        &format!("miller-{}-{}", directory, entry.name),
        &entry.name,
        &options,
        |frame, _| {
            frame.size_next(20.0, 20.0);
            icons::icon(frame, icons::entry_icon(entry), 17.0);
            frame.flex(1.0);
            frame.label(&entry.name);
            if entry.navigable {
                frame.size_next(15.0, 15.0);
                icons::icon(frame, ids::LENS_ICON_CHEVRON_RIGHT, 13.0);
            }
        },
    );
    frame.pop_id();
    if response.clicked {
        app.miller_clicked(directory, &entry.name);
    } else if response.right_clicked && current {
        if let Some(index) = visible_index {
            app.row_right_clicked(frame, index, response.rect);
        }
    }
}

fn miller_header(frame: &mut Frame, tones: &Tones, path: &str, current: bool) {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".into());
    frame.size_next(0.0, 34.0);
    frame.row_ex(
        &LayoutOpts {
            height: 34.0,
            gap: 7.0,
            pad: 5.0,
            cross: Align::Center,
            bg: if current { tones.selected } else { tones.card },
            radius: 6.0,
            ..Default::default()
        },
        |frame| {
            icons::icon(frame, ids::LENS_ICON_FOLDER, 15.0);
            if current {
                frame.heading(&name, 4);
            } else {
                frame.label(&name);
            }
        },
    );
}

fn row_icon(frame: &mut Frame, entry: &Entry) {
    frame.size_next(24.0, 24.0);
    icons::icon(frame, icons::entry_icon(entry), 19.0);
}

fn metadata_label(frame: &mut Frame, tones: &Tones, text: &str, width: f32) {
    frame.size_next(width, 16.0);
    theme::label_colored_sized(frame, text, 12.0, tones.muted);
}

fn sort_link(app: &mut UiApp, frame: &mut Frame, key: SortKey, label: &str) {
    let marker = if app.state.sort_key == key {
        if app.state.sort_ascending {
            " ↑"
        } else {
            " ↓"
        }
    } else {
        ""
    };
    if frame.link(&format!("{label}{marker}")) {
        app.state.toggle_sort(key);
    }
}

fn capture_rename(app: &mut UiApp, frame: &mut Frame) {
    let response = unsafe { lens_sys::lens_get_response(frame.as_raw()) };
    app.rename_id = response.id;
    app.rename_focused_now = response.focused;
}

fn placeholder(
    frame: &mut Frame,
    tones: &Tones,
    icon: icons::IconId,
    headline: &str,
    detail: &str,
) {
    frame.flex(1.0);
    frame.column_ex(
        &LayoutOpts {
            flex: 1.0,
            gap: 10.0,
            pad: 40.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            frame.size_next(74.0, 74.0);
            frame.column_ex(
                &LayoutOpts {
                    width: 74.0,
                    height: 74.0,
                    cross: Align::Center,
                    bg: tones.card,
                    radius: 18.0,
                    ..Default::default()
                },
                |frame| icons::icon(frame, icon, 34.0),
            );
            frame.heading(headline, 3);
            theme::label_colored(frame, detail, tones.muted);
        },
    );
}
