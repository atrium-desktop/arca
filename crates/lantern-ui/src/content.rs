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

/// Row pitches (row height + the parent column's gap) for the virtualized
/// listings.
const GRID_ROW_PITCH: f32 = GRID_CARD_HEIGHT + 10.0;
const LIST_ROW_PITCH: f32 = 42.0 + 2.0;
const MILLER_ROW_PITCH: f32 = 36.0 + 2.0;
/// Extra rows built past each viewport edge so one-frame-stale geometry
/// never flashes blank while scrolling.
const OVERSCAN_ROWS: usize = 2;

fn scroll_offset(frame: &mut Frame, id: &str) -> Option<(f32, f32)> {
    let c = std::ffi::CString::new(id).ok()?;
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let ok = unsafe { lens_sys::lens_scroll_offset(frame.as_raw(), c.as_ptr(), &mut x, &mut y) };
    ok.then_some((x, y))
}

fn scroll_to(frame: &mut Frame, id: &str, x: f32, y: f32) {
    if let Ok(c) = std::ffi::CString::new(id) {
        unsafe { lens_sys::lens_scroll_to(frame.as_raw(), c.as_ptr(), x, y) };
    }
}

/// Row range intersecting the scroll viewport: the retained scroll offset
/// plus the viewport height from last frame's bounds, plus overscan. Falls
/// back to the first `fallback_rows` rows when no geometry exists yet
/// (first frame).
///
/// lens's per-frame arena is 1 MiB and draw calls past the limit are dropped
/// silently (blank icons and labels), so a large listing must only ever
/// build the rows that are actually visible.
fn visible_rows(
    frame: &mut Frame,
    scroll_id: &str,
    row_count: usize,
    pitch: f32,
    fallback_rows: usize,
) -> (usize, usize) {
    if row_count == 0 {
        return (0, 0);
    }
    // Same id scope as the scroll itself, so both queries resolve.
    let offset = scroll_offset(frame, scroll_id).map(|(_, y)| y);
    let viewport_h = frame.node_bounds(scroll_id).map(|rect| rect.h);
    let (Some(offset), Some(viewport_h)) = (offset, viewport_h) else {
        return (0, row_count.min(fallback_rows));
    };
    let first = ((offset / pitch).floor() as usize).saturating_sub(OVERSCAN_ROWS);
    let last =
        (((offset + viewport_h) / pitch).ceil() as usize + OVERSCAN_ROWS + 1).min(row_count);
    if first >= last {
        // Stale offset right after navigating away from a longer listing:
        // anchor to the content end — lens clamps the offset during layout,
        // so the next frame is consistent again.
        return (row_count.saturating_sub(fallback_rows), row_count);
    }
    (first, last)
}

/// Height a spacer needs to stand in for `rows` elided rows: the rows plus
/// their gaps, minus the one gap the spacer's own presence already adds.
/// Never zero so the column's child count — and with it the total content
/// height — stays constant.
fn spacer_height(rows: usize, pitch: f32, gap: f32) -> f32 {
    (rows as f32 * pitch - gap).max(0.001)
}

/// Fixed-height stand-in for a run of elided rows. The "##" id renders no
/// text; the node only carries geometry.
fn row_spacer(frame: &mut Frame, id: &str, height: f32) {
    frame.size_next(0.0, height);
    frame.label(id);
}

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
                    ids::AlertCircle,
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
                        ids::Folder,
                        "This folder is empty",
                        "Drop files here or create a new folder",
                    );
                } else {
                    placeholder(
                        frame,
                        tones,
                        ids::Search,
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
            // In Grid mode, provide a clean sort selector instead of confusing list headers
            if app.state.view_mode == ViewMode::Grid {
                frame.flex(1.0);
                frame.spacer(0.0);
                sort_cell(app, frame, tones, SortKey::Name, "Name", 0.0);
            } else if app.state.view_mode == ViewMode::Miller {
                frame.flex(1.0);
                frame.spacer(0.0);
                sort_cell(app, frame, tones, SortKey::Name, "Sort: Name", 0.0);
            }
        },
    );
}

fn build_grid(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    let columns = app.grid_columns();
    let row_count = visible.len().div_ceil(columns);
    let fallback = (app.viewport.1 / GRID_ROW_PITCH).ceil() as usize + OVERSCAN_ROWS + 2;
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
                let (first, last) =
                    visible_rows(frame, "grid-files", row_count, GRID_ROW_PITCH, fallback);
                row_spacer(frame, "##grid-vtop", spacer_height(first, GRID_ROW_PITCH, 10.0));
                for row in first..last {
                    let start = row * columns;
                    let chunk = &visible[start..(start + columns).min(visible.len())];
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
                row_spacer(
                    frame,
                    "##grid-vbot",
                    spacer_height(row_count - last, GRID_ROW_PITCH, 10.0),
                );
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
    // A row handler earlier in this frame's build may have navigated and
    // re-read the directory; the cloned `visible` list then holds stale
    // indices. Skip such rows — the next frame is consistent again.
    let Some(entry) = app.state.entries().get(entry_index).cloned() else {
        return;
    };
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

    let grid_id = format!("grid-{}", entry.name);
    let (response, ()) = frame
        .row()
        .id_label(&grid_id, &entry.name)
        .with_opts(&options)
        .show(|frame| {
            frame.column_ex(
                &LayoutOpts {
                    width: GRID_CARD_WIDTH - 20.0,
                    gap: 7.0,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
                    card_art(app, frame, &entry, 48.0);
                    theme::label_centered(frame, &entry.name, 12.5, GRID_CARD_WIDTH - 22.0, 2);
                },
            );
        });
    frame.pop_id();
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(visible_index, response.rect);
    }
}

fn build_list(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    build_list_header(app, frame, tones, header_trailing_inset(app, visible.len()));
    let fallback = (app.viewport.1 / LIST_ROW_PITCH).ceil() as usize + OVERSCAN_ROWS + 2;
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
                let (first, last) =
                    visible_rows(frame, "list-files", visible.len(), LIST_ROW_PITCH, fallback);
                row_spacer(frame, "##list-vtop", spacer_height(first, LIST_ROW_PITCH, 2.0));
                for (visible_index, &entry_index) in
                    visible.iter().enumerate().take(last).skip(first)
                {
                    build_list_row(app, frame, tones, entry_index, visible_index);
                }
                row_spacer(
                    frame,
                    "##list-vbot",
                    spacer_height(visible.len() - last, LIST_ROW_PITCH, 2.0),
                );
            },
        );
    });
}

/// Trailing inset the header needs so its right-hand columns line up with
/// the scrolled rows: the scroll column pads content by 2 px, and the layout
/// pass reserves a scrollbar-wide gutter once the rows overflow.
fn header_trailing_inset(app: &UiApp, rows: usize) -> f32 {
    const CHROME_H: f32 = 42.0 + 54.0 + 32.0; // tab strip + toolbar + status bar
    let content_h = (app.viewport.1 - CHROME_H - 24.0).max(0.0); // content pad 12×2
    let scroll_h = content_h - 40.0 - 32.0 - 16.0; // heading + header + two 8px gaps
    let rows_h = rows as f32 * (42.0 + 2.0) - 2.0 + 4.0; // 42px rows, 2px gap, 2×2 pad
    let gutter = if rows_h > scroll_h {
        theme::SCROLLBAR_W
    } else {
        0.0
    };
    2.0 + gutter
}

fn build_list_header(app: &mut UiApp, frame: &mut Frame, tones: &Tones, trailing: f32) {
    frame.size_next(0.0, 32.0);
    frame.row_ex(
        &LayoutOpts {
            height: 32.0,
            // No container gap: every separation is an explicit spacer so the
            // scroll-geometry insets (2 px scroll pad, scrollbar gutter) land
            // exactly instead of being shifted by inter-child gaps.
            gap: 0.0,
            pad: 7.0,
            cross: Align::Center,
            bg: tones.card,
            radius: 7.0,
            ..Default::default()
        },
        |frame| {
            // 24 px icon slot + 2 px the scroll column pads its content with.
            frame.size_next(26.0, 16.0);
            frame.label("");
            frame.spacer(10.0);
            frame.flex(1.0);
            sort_cell(app, frame, tones, SortKey::Name, "Name", 0.0);
            frame.spacer(10.0);
            sort_cell(app, frame, tones, SortKey::Size, "Size", 86.0);
            frame.spacer(10.0);
            sort_cell(app, frame, tones, SortKey::Mtime, "Modified", 124.0);
            frame.spacer(trailing);
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
    // See build_grid_card: indices from the cloned `visible` list may be
    // stale after a mid-build navigation.
    let Some(entry) = app.state.entries().get(entry_index).cloned() else {
        return;
    };
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

    let (response, ()) = frame
        .row()
        .id_label(&entry.name, &entry.name)
        .with_opts(&options)
        .show(|frame| {
            row_icon(frame, &entry);
            frame.flex(1.0);
            let font_size = frame.theme().font_size();
            frame.label_compact_sized(&entry.name, font_size);
            metadata_label(frame, tones, &size, 86.0);
            metadata_label(frame, tones, &modified, 124.0);
        });
    frame.pop_id();
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(visible_index, response.rect);
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
                            let fallback =
                                (column_height / MILLER_ROW_PITCH).ceil() as usize + OVERSCAN_ROWS + 2;
                            frame.scroll(&scroll_id, |frame| {
                                frame.column_ex(
                                    &LayoutOpts {
                                        gap: 2.0,
                                        cross: Align::Stretch,
                                        ..Default::default()
                                    },
                                    |frame| {
                                        let entry_count = if current {
                                            visible.len()
                                        } else {
                                            column.entries.len()
                                        };
                                        let (first, last) = visible_rows(
                                            frame,
                                            &scroll_id,
                                            entry_count,
                                            MILLER_ROW_PITCH,
                                            fallback,
                                        );
                                        row_spacer(
                                            frame,
                                            "##miller-vtop",
                                            spacer_height(first, MILLER_ROW_PITCH, 2.0),
                                        );
                                        if current {
                                            for (visible_index, &entry_index) in visible
                                                .iter()
                                                .enumerate()
                                                .take(last)
                                                .skip(first)
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
                                            for (entry_index, entry) in column
                                                .entries
                                                .iter()
                                                .enumerate()
                                                .take(last)
                                                .skip(first)
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
                                        row_spacer(
                                            frame,
                                            "##miller-vbot",
                                            spacer_height(
                                                entry_count - last,
                                                MILLER_ROW_PITCH,
                                                2.0,
                                            ),
                                        );
                                    },
                                );
                            });
                            if seed_scroll && !current {
                                if let Some(selected) = column.selected {
                                    let offset = (selected as f32 * 38.0 - 260.0).max(0.0);
                                    scroll_to(frame, &scroll_id, 0.0, offset);
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
    let miller_id = format!("miller-{}-{}", directory, entry.name);
    let (response, ()) = frame
        .row()
        .id_label(&miller_id, &entry.name)
        .with_opts(&options)
        .show(|frame| {
            icons::icon(frame, icons::entry_icon(entry), 17.0);
            frame.flex(1.0);
            let font_size = frame.theme().font_size();
            frame.label_compact_sized(&entry.name, font_size);
            if entry.navigable {
                icons::icon(frame, ids::ChevronRight, 13.0);
            }
        });
    frame.pop_id();
    if response.clicked {
        app.miller_clicked(directory, &entry.name);
    } else if response.right_clicked && current {
        if let Some(index) = visible_index {
            app.row_right_clicked(index, response.rect);
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
            // 8 + 15 + 7 lands the title at the same inset as the rows'
            // labels (pad 6 + icon 17 + gap 7).
            pad: 8.0,
            cross: Align::Center,
            bg: if current { tones.selected } else { tones.card },
            radius: 6.0,
            ..Default::default()
        },
        |frame| {
            icons::icon(frame, ids::Folder, 15.0);
            // Compact labels: the padded label/heading carry theme padding
            // that breaks row-inset alignment (and padded labels used to
            // overflow fixed rows downward). The current column's header
            // reads as a slightly larger title, same left edge as the rows.
            let size = if current {
                15.0
            } else {
                frame.theme().font_size()
            };
            frame.label_compact_sized(&name, size);
        },
    );
}

fn row_icon(frame: &mut Frame, entry: &Entry) {
    icons::icon(frame, icons::entry_icon(entry), 19.0);
}

/// Grid-card artwork: the entry's decoded thumbnail (album cover, image
/// file) fitted into a `size` square, or the type glyph while it decodes.
/// Only built — i.e. visible — cards ever reach here, so requests stay lazy.
fn card_art(app: &mut UiApp, frame: &mut Frame, entry: &Entry, size: f32) {
    if app.state.show_thumbnails && crate::thumbs::is_thumbable(&entry.name) {
        let path = std::path::Path::new(app.state.cwd())
            .join(&entry.name)
            .to_string_lossy()
            .into_owned();
        if let Some(image) = app.thumbs.image_for(&path) {
            // SAFETY: the frame is live; the store owns the image and it
            // outlives the frame. Dimensions of an uploaded image are valid.
            let (w, h) = unsafe {
                (
                    flux_sys::flux_image_width(image),
                    flux_sys::flux_image_height(image),
                )
            };
            let (dw, dh) = if w >= h {
                (size, size * h as f32 / w.max(1) as f32)
            } else {
                (size * w as f32 / h.max(1) as f32, size)
            };
            frame.size_next(dw, dh);
            // SAFETY: as above.
            unsafe { frame.image(image, dw, dh) };
            return;
        }
    }
    icons::icon(frame, icons::entry_icon(entry), size);
}

fn metadata_label(frame: &mut Frame, tones: &Tones, text: &str, width: f32) {
    frame.size_next(width, 16.0);
    theme::label_colored_sized(frame, text, 12.0, tones.muted);
}

/// Clickable column-sort cell: compact label plus a chevron glyph for the
/// active direction. A pressable row (neutral hover wash) rather than a text
/// link — the link widget's animated accent underline read as a stray bar
/// under the header. `width` pins the cell (list columns); 0 leaves it at
/// its natural width (heading row).
fn sort_cell(
    app: &mut UiApp,
    frame: &mut Frame,
    tones: &Tones,
    key: SortKey,
    label: &str,
    width: f32,
) {
    let active = app.state.sort_key == key;
    let ascending = app.state.sort_ascending;
    let options = LayoutOpts {
        width,
        height: 22.0,
        gap: 4.0,
        pad: 0.0,
        cross: Align::Center,
        radius: 6.0,
        ..Default::default()
    };
    let sort_id = format!("sort-{label}");
    let (response, ()) = frame
        .row()
        .id_label(&sort_id, label)
        .with_opts(&options)
        .show(|frame| {
            let fg = if active {
                frame.theme().fg()
            } else {
                tones.muted
            };
            theme::label_colored_sized(frame, label, 12.0, fg);
            if active {
                let icon = if ascending {
                    ids::ChevronUp
                } else {
                    ids::ChevronDown
                };
                icons::icon(frame, icon, 12.0);
            }
        });
    if response.clicked {
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
    icon: icons::AssetId,
    headline: &str,
    detail: &str,
) {
    frame.flex(1.0);
    frame.column_ex(
        &LayoutOpts {
            flex: 1.0,
            gap: 14.0,
            pad: 40.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            frame.size_next(64.0, 64.0);
            frame.column_ex(
                &LayoutOpts {
                    width: 64.0,
                    height: 64.0,
                    cross: Align::Center,
                    bg: tones.card,
                    radius: 16.0,
                    ..Default::default()
                },
                |frame| icons::icon(frame, icon, 34.0),
            );
            frame.heading(headline, 3);
            theme::label_colored(frame, detail, tones.muted);
        },
    );
}
