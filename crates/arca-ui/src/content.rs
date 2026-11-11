//! Three production content presentations: grid, detailed list and Miller
//! columns. All share the same selection and file-operation model.

use arca_engine::entry::{FileType, SortKey};
use arca_engine::format::{format_size, format_time};
use arca_engine::{Entry, ViewMode};
use iris::{Align, Color, Frame, LayoutOpts};
use std::path::PathBuf;

use crate::app::{focus_any, FocusTarget, UiApp};
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) const GRID_MIN_CARD_WIDTH: f32 = 104.0;
pub(crate) const GRID_MAX_CARD_WIDTH: f32 = 144.0;
pub(crate) const GRID_GAP: f32 = 6.0;
pub(crate) const GRID_ICON_SIZE: f32 = 50.0;
pub(crate) const GRID_FONT_SIZE: f32 = 12.5;
pub(crate) const GRID_CARD_PAD: f32 = 4.0;
pub(crate) const GRID_ART_LABEL_GAP: f32 = 4.0;
pub(crate) const GRID_LABEL_LINE_GAP: f32 = 2.0;
pub(crate) const GRID_MAX_LABEL_LINES: usize = 2;

/// Canonical default resting card height for headless initial state and test assertions.
pub(crate) const GRID_DEFAULT_CARD_HEIGHT: f32 = 102.0;

/// Dynamically derive the card height required to fit `GRID_ICON_SIZE` art plus
/// `GRID_MAX_LABEL_LINES` of text based on the active theme font's line box metrics.
/// Measures both Latin ("Ag") and CJK ("永") glyph bounds to ensure universal multi-script clearance.
pub(crate) fn derive_grid_card_height(frame: &Frame) -> f32 {
    let m_latin = frame.measure_text("Ag", GRID_FONT_SIZE);
    let m_cjk = frame.measure_text("永", GRID_FONT_SIZE);
    let line_height = m_latin
        .height
        .max(m_cjk.height)
        .ceil()
        .max(GRID_FONT_SIZE * 1.4);
    let text_height = (line_height * GRID_MAX_LABEL_LINES as f32)
        + (GRID_LABEL_LINE_GAP * (GRID_MAX_LABEL_LINES - 1) as f32);
    (GRID_CARD_PAD * 2.0 + GRID_ICON_SIZE + GRID_ART_LABEL_GAP + text_height).ceil()
}

/// Dynamically derive the row pitch (card height + row gap) for the active frame.
pub(crate) fn derive_grid_row_pitch(frame: &Frame) -> f32 {
    derive_grid_card_height(frame) + GRID_GAP
}

/// Row pitches (row height + the parent column's gap) for the virtualized
/// listings.
const LIST_ROW_PITCH: f32 = 42.0 + 2.0;
const MILLER_ROW_PITCH: f32 = 36.0 + 2.0;
/// Extra rows built past each viewport edge so one-frame-stale geometry
/// never flashes blank while scrolling.
const OVERSCAN_ROWS: usize = 2;

/// Drag-and-drop action bitmask advertised to the compositor
/// (`lens_dnd_source_desc.actions`: 1 = copy, 2 = move).
const DND_ACTIONS: u32 = 1 | 2;

/// The interchange payload for a drag started on `row_path`.
///
/// A pointer press that lands inside an existing multi-selection starts a
/// drag of the *whole* selection; otherwise just that row. The payload is an
/// RFC 2483 `text/uri-list`, which is what both the Wayland host and every
/// external file manager speak, and what [`AppState::drop_into`] parses.
fn drag_payload(app: &UiApp, row_selected: bool, row_path: &str) -> String {
    let paths: Vec<PathBuf> = if row_selected && app.state.selected_count() > 1 {
        app.state
            .selected_paths()
            .into_iter()
            .map(PathBuf::from)
            .collect()
    } else {
        vec![PathBuf::from(row_path)]
    };
    arca_xdg::FileClipboard::new(paths, arca_xdg::TransferAction::Copy).to_uri_list()
}

// Test-only record of the last `build_list` virtualized window, so a test can
// assert scrolling advances `(first, last)` instead of freezing on the
// first-frame fallback.
#[cfg(test)]
thread_local! {
    static LIST_WINDOW: std::cell::Cell<Option<(usize, usize)>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn record_list_window(window: (usize, usize)) {
    LIST_WINDOW.with(|slot| slot.set(Some(window)));
}

#[cfg(test)]
fn last_list_window() -> Option<(usize, usize)> {
    LIST_WINDOW.with(std::cell::Cell::get)
}

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

/// Offset and viewport height of the scroll node with `scroll_id`, retained
/// from the previous frame. Both queries must run from the scroll's *parent*
/// id scope: inside the scroll closure the id is re-hashed against the
/// scroll's own scope and never matches, so the lookup would silently fail
/// and freeze the virtualized window on its first-frame fallback.
fn scroll_geometry(frame: &mut Frame, scroll_id: &str) -> Option<(f32, f32)> {
    let offset = scroll_offset(frame, scroll_id).map(|(_, y)| y)?;
    let viewport_h = frame.node_bounds(scroll_id).map(|rect| rect.h)?;
    Some((offset, viewport_h))
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
    geometry: Option<(f32, f32)>,
    row_count: usize,
    pitch: f32,
    fallback_rows: usize,
) -> (usize, usize) {
    if row_count == 0 {
        return (0, 0);
    }
    let Some((offset, viewport_h)) = geometry else {
        return (0, row_count.min(fallback_rows));
    };
    let first = ((offset / pitch).floor() as usize).saturating_sub(OVERSCAN_ROWS);
    let last = (((offset + viewport_h) / pitch).ceil() as usize + OVERSCAN_ROWS + 1).min(row_count);
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

fn build_grid(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    let available_w = (app.viewport.0 - app.sidebar_width - 20.0).max(120.0);
    let card_height = derive_grid_card_height(frame);
    let row_pitch = derive_grid_row_pitch(frame);
    let plan = lens::patterns::virtual_grid_calc(
        available_w,
        app.viewport.1,
        0.0,
        visible.len(),
        GRID_MIN_CARD_WIDTH,
        GRID_MAX_CARD_WIDTH,
        card_height,
        GRID_GAP,
        OVERSCAN_ROWS,
    );
    let columns = (plan.columns as usize).max(1);
    app.current_grid_columns = columns;

    let card_width = plan
        .column_width
        .clamp(GRID_MIN_CARD_WIDTH, GRID_MAX_CARD_WIDTH);
    let row_count = visible.len().div_ceil(columns);
    let fallback = (app.viewport.1 / row_pitch).ceil() as usize + OVERSCAN_ROWS + 2;
    let geometry = scroll_geometry(frame, "grid-files");
    frame.flex(1.0);
    frame.scroll("grid-files", |frame| {
        frame.column_ex(
            &LayoutOpts {
                gap: GRID_GAP,
                pad: 4.0,
                cross: Align::Stretch,
                ..Default::default()
            },
            |frame| {
                let (first, last) = visible_rows(geometry, row_count, row_pitch, fallback);
                row_spacer(
                    frame,
                    "##grid-vtop",
                    spacer_height(first, row_pitch, GRID_GAP),
                );
                for row in first..last {
                    let start = row * columns;
                    let chunk = &visible[start..(start + columns).min(visible.len())];
                    frame.push_id(&format!("grid-row-{row}"));
                    frame.row_ex(
                        &LayoutOpts {
                            gap: GRID_GAP,
                            cross: Align::Start,
                            ..Default::default()
                        },
                        |frame| {
                            for (column, &entry_index) in chunk.iter().enumerate() {
                                let visible_index = row * columns + column;
                                build_grid_card(
                                    app,
                                    frame,
                                    tones,
                                    entry_index,
                                    visible_index,
                                    card_width,
                                    card_height,
                                );
                            }
                        },
                    );
                    frame.pop_id();
                }
                row_spacer(
                    frame,
                    "##grid-vbot",
                    spacer_height(row_count - last, row_pitch, GRID_GAP),
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
    card_width: f32,
    card_height: f32,
) {
    // A row handler earlier in this frame's build may have navigated and
    // re-read the directory; the cloned `visible` list then holds stale
    // indices. Skip such rows — the next frame is consistent again.
    let Some(entry) = app.state.entries().get(entry_index).cloned() else {
        return;
    };
    let selected = app.state.is_selected(visible_index);
    let full_path = std::path::Path::new(app.state.cwd())
        .join(&entry.name)
        .to_string_lossy()
        .into_owned();
    let grid_id = format!("grid-{}", entry.name);

    let drop_info = if entry.navigable {
        frame.dnd_drop_target(&grid_id, 1)
    } else {
        None
    };
    if let Some(drop) = &drop_info {
        if let Some(payload) = &drop.payload {
            app.state
                .drop_into(payload, &full_path, arca_engine::TransferAction::Move);
        }
    }
    let is_drag_active = app.active_drag.as_ref().is_some_and(|d| d.started);
    let drop_hovered = is_drag_active && drop_info.as_ref().map(|d| d.is_hovered).unwrap_or(false);

    let is_dragged = is_drag_active
        && app
            .active_drag
            .as_ref()
            .is_some_and(|d| d.path == full_path);

    let is_hovered = app.hovered_grid_index == Some(visible_index);
    // macOS Finder-grade desktop presentation:
    // Grid items do not paint heavy background cards on pointer hover.
    // At rest and on hover, the card canvas remains completely transparent.
    // Selection highlights only the label text pill with an accent capsule
    // and high-contrast white text, conforming snugly to the label lines.
    // Active drag-and-drop targets receive an explicit highlight wash and border.
    let (text_color, label_bg) = if selected || drop_hovered {
        (Color::rgba(255, 255, 255, 255), frame.theme().accent())
    } else if is_hovered {
        (tones.file_label_hover, Color::TRANSPARENT)
    } else {
        (tones.file_label, Color::TRANSPARENT)
    };

    let (card_bg, card_border, card_border_w) = if drop_hovered {
        (tones.selected, frame.theme().accent(), 1.5)
    } else {
        (Color::TRANSPARENT, Color::TRANSPARENT, 0.0)
    };

    let text_width = card_width - (GRID_CARD_PAD * 2.0);
    let options = LayoutOpts {
        width: card_width,
        height: card_height,
        gap: 0.0,
        pad: GRID_CARD_PAD,
        cross: Align::Start,
        bg: card_bg,
        border: card_border,
        border_width: card_border_w,
        radius: 6.0,
        ..Default::default()
    };

    let prev_opacity = frame.opacity();
    if is_dragged {
        frame.set_opacity(0.35);
    }

    if app.renaming.as_deref() == Some(entry.name.as_str()) {
        frame.push_id(&entry.name);
        frame.column_ex(
            &LayoutOpts {
                cross: Align::Center,
                ..options
            },
            |frame| {
                icons::icon(frame, icons::entry_icon(&entry), GRID_ICON_SIZE);
                frame.size_next(0.0, 2.0);
                frame.spacer(2.0);
                frame.size_next(text_width, 0.0);
                let resp = frame.textedit_ex("##grid-rename", &mut app.rename, false, false, false);
                capture_rename(app, frame, "##grid-rename");
                if resp.submitted {
                    app.commit_rename(frame);
                }
            },
        );
        frame.pop_id();
        if is_dragged {
            frame.set_opacity(prev_opacity);
        }
        return;
    }

    // Suppress lens's automatic pressable card hover/active wash and focus border so the
    // grid item follows native desktop canvas conventions (no flashing gray cards, no focus border).
    let prev_theme = frame.theme();
    frame.set_theme(
        prev_theme
            .with_hover(Color::TRANSPARENT)
            .with_active(Color::TRANSPARENT)
            .with_accent(Color::TRANSPARENT),
    );

    let (response, ()) = frame
        .row()
        .id_label(&grid_id, &entry.name)
        .with_opts(&options)
        .show(|frame| {
            frame.column_ex(
                &LayoutOpts {
                    width: text_width,
                    gap: GRID_ART_LABEL_GAP,
                    cross: Align::Center,
                    ..Default::default()
                },
                |frame| {
                    frame.column_ex(
                        &LayoutOpts {
                            width: text_width,
                            height: GRID_ICON_SIZE,
                            cross: Align::Center,
                            ..Default::default()
                        },
                        |frame| {
                            card_art(app, frame, &entry, GRID_ICON_SIZE, selected);
                        },
                    );
                    theme::label_centered_styled(
                        frame,
                        &entry.name,
                        GRID_FONT_SIZE,
                        text_width,
                        GRID_MAX_LABEL_LINES,
                        text_color,
                        label_bg,
                    );
                },
            );
        });

    frame.set_theme(prev_theme);
    if response.hovered {
        app.next_hovered_grid_index = Some(visible_index);
    }
    let dnd_payload = drag_payload(app, selected, &full_path);
    // COPY | MOVE: the negotiated action decides, and the payload carries
    // the path list so the drop side never has to re-derive it.
    frame.dnd_source(&grid_id, &dnd_payload, DND_ACTIONS);
    if is_dragged {
        frame.set_opacity(prev_opacity);
    }
    if response.pressed {
        app.start_drag_candidate(
            entry.name.clone(),
            full_path.clone(),
            entry.navigable,
            dnd_payload.clone(),
        );
    }
    if entry.navigable {
        app.drop_targets.push(crate::app::DropTargetZone {
            rect: response.rect,
            target_path: full_path.clone(),
            is_bookmark_zone: false,
        });
    }
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(visible_index, response.rect);
    }
    suppress_item_pointer_cursor(frame);
}

/// Restore standard desktop arrow cursor over content listing items (ADR desktop conventions:
/// file manager items are desktop objects, not web hyperlinks, so they use the standard arrow cursor).
pub(crate) fn suppress_item_pointer_cursor(frame: &mut Frame) {
    frame.set_cursor_hint(lens::CursorHint::Default);
}

fn build_list(app: &mut UiApp, frame: &mut Frame, tones: &Tones, visible: &[usize]) {
    build_list_header(app, frame, tones, header_trailing_inset(app, visible.len()));
    let fallback = (app.viewport.1 / LIST_ROW_PITCH).ceil() as usize + OVERSCAN_ROWS + 2;
    let geometry = scroll_geometry(frame, "list-files");
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
                let (first, last) = visible_rows(geometry, visible.len(), LIST_ROW_PITCH, fallback);
                #[cfg(test)]
                record_list_window((first, last));
                row_spacer(
                    frame,
                    "##list-vtop",
                    spacer_height(first, LIST_ROW_PITCH, 2.0),
                );
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
    // A Trash listing is a view over `.trashinfo` records: the on-disk size
    // and mtime describe the trash copy, while what the user needs is where
    // the item came from and when it was deleted.
    let (size, modified) = match app.state.trash_record(&entry.name) {
        Some(item) => (
            item.original_path
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_default(),
            item.deletion_date.clone(),
        ),
        None => (
            if entry.file_type == FileType::Directory {
                "—".into()
            } else {
                format_size(entry.size)
            },
            format_time(entry.mtime),
        ),
    };
    let full_path = std::path::Path::new(app.state.cwd())
        .join(&entry.name)
        .to_string_lossy()
        .into_owned();

    let drop_info = if entry.navigable {
        frame.dnd_drop_target(&entry.name, 1)
    } else {
        None
    };
    if let Some(drop) = &drop_info {
        if let Some(payload) = &drop.payload {
            app.state
                .drop_into(payload, &full_path, arca_engine::TransferAction::Move);
        }
    }
    let is_drag_active = app.active_drag.as_ref().is_some_and(|d| d.started);
    let drop_hovered = is_drag_active && drop_info.as_ref().map(|d| d.is_hovered).unwrap_or(false);
    let selected = app.state.is_selected(visible_index);

    let is_dragged = is_drag_active
        && app
            .active_drag
            .as_ref()
            .is_some_and(|d| d.path == full_path);

    let options = LayoutOpts {
        height: 42.0,
        gap: 10.0,
        pad: 7.0,
        cross: Align::Center,
        bg: if drop_hovered || selected {
            tones.selected
        } else {
            Color::TRANSPARENT
        },
        radius: 7.0,
        ..Default::default()
    };

    let prev_opacity = frame.opacity();
    if is_dragged {
        frame.set_opacity(0.35);
    }

    if app.renaming.as_deref() == Some(entry.name.as_str()) {
        frame.push_id(&entry.name);
        frame.row_ex(&options, |frame| {
            row_icon(frame, &entry);
            frame.flex(1.0);
            let resp = frame.textedit_ex("##list-rename", &mut app.rename, false, false, false);
            capture_rename(app, frame, "##list-rename");
            if resp.submitted {
                app.commit_rename(frame);
            }
            metadata_label(frame, tones, &size, 86.0);
            metadata_label(frame, tones, &modified, 124.0);
        });
        frame.pop_id();
        if is_dragged {
            frame.set_opacity(prev_opacity);
        }
        return;
    }

    let prev_theme = frame.theme();
    frame.set_theme(
        prev_theme
            .with_hover(Color::TRANSPARENT)
            .with_active(Color::TRANSPARENT)
            .with_accent(Color::TRANSPARENT),
    );

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
    frame.set_theme(prev_theme);
    let dnd_payload = drag_payload(app, selected, &full_path);
    frame.dnd_source(&entry.name, &dnd_payload, DND_ACTIONS);
    if is_dragged {
        frame.set_opacity(prev_opacity);
    }
    if response.pressed {
        app.start_drag_candidate(
            entry.name.clone(),
            full_path.clone(),
            entry.navigable,
            dnd_payload.clone(),
        );
    }
    if entry.navigable {
        app.drop_targets.push(crate::app::DropTargetZone {
            rect: response.rect,
            target_path: full_path.clone(),
            is_bookmark_zone: false,
        });
    }
    if entry.name == "sub" {
        app.sub_row_rect = Some(response.rect);
    }
    if response.clicked {
        app.row_clicked(visible_index, &entry.name);
    } else if response.right_clicked {
        app.row_right_clicked(visible_index, response.rect);
    }
    suppress_item_pointer_cursor(frame);
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
                            let fallback = (column_height / MILLER_ROW_PITCH).ceil() as usize
                                + OVERSCAN_ROWS
                                + 2;
                            let geometry = scroll_geometry(frame, &scroll_id);
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
                                            geometry,
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
                                            for (visible_index, &entry_index) in
                                                visible.iter().enumerate().take(last).skip(first)
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
    let prev_theme = frame.theme();
    frame.set_theme(
        prev_theme
            .with_hover(Color::TRANSPARENT)
            .with_active(Color::TRANSPARENT)
            .with_accent(Color::TRANSPARENT),
    );
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
    frame.set_theme(prev_theme);
    frame.pop_id();
    if response.clicked {
        app.miller_clicked(directory, &entry.name);
    } else if response.right_clicked && current {
        if let Some(index) = visible_index {
            app.row_right_clicked(index, response.rect);
        }
    }
    suppress_item_pointer_cursor(frame);
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
/// When selected, the art is slightly shaded (macOS Finder-grade darkening filter).
/// Only built — i.e. visible — cards ever reach here, so requests stay lazy.
fn card_art(app: &mut UiApp, frame: &mut Frame, entry: &Entry, size: f32, selected: bool) {
    let prev_opacity = frame.opacity();
    if selected {
        frame.set_opacity(0.72);
    }
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
            if selected {
                unsafe { frame.image_tinted(image, dw, dh, Color::rgba(180, 180, 180, 255)) };
            } else {
                unsafe { frame.image(image, dw, dh) };
            }
            if selected {
                frame.set_opacity(prev_opacity);
            }
            return;
        }
    }
    icons::icon(frame, icons::entry_icon(entry), size);
    if selected {
        frame.set_opacity(prev_opacity);
    }
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
    let prev_theme = frame.theme();
    frame.set_theme(
        prev_theme
            .with_hover(Color::TRANSPARENT)
            .with_active(Color::TRANSPARENT)
            .with_accent(Color::TRANSPARENT),
    );
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
    frame.set_theme(prev_theme);
    suppress_item_pointer_cursor(frame);
    if response.clicked {
        app.state.toggle_sort(key);
    }
}

fn capture_rename(app: &mut UiApp, frame: &mut Frame, label: &str) {
    app.rename_focused_now = frame.response().focused;
    if app.pending_focus == Some(FocusTarget::Rename) {
        if let Some(name) = &app.renaming {
            let stem_len = if let Some(dot_idx) = name.rfind('.') {
                if dot_idx > 0 {
                    dot_idx
                } else {
                    name.len()
                }
            } else {
                name.len()
            };
            frame.textedit_set_selection(label, 0, stem_len as u32);
            if focus_any(frame, &[label]) {
                app.pending_focus = None;
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use crate::test_support::{fixture, frame};

    use super::*;
    use arca_engine::config::Config;
    use arca_engine::state::AppState;

    /// The virtualized list window must follow the scroll offset. The offset
    /// and viewport are only resolvable from the scroll's parent scope; when
    /// they were queried inside the scroll closure the lookup always failed,
    /// pinning the window to the first frame's fallback so scrolling a long
    /// directory showed blank rows.
    #[test]
    fn scrolling_advances_the_virtualized_list_window() {
        let dir = std::env::temp_dir().join(format!(
            "arca-content-scroll-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..200 {
            std::fs::write(dir.join(format!("file-{i:04}.txt")), b"x").unwrap();
        }
        let config_file = dir.join("cfg").join("arca.conf");
        let mut app = UiApp::new(AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.to_str().unwrap(),
        ));
        app.state.set_view_mode(ViewMode::List);

        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..3 {
            ui.frame(&settle, |f| app.build(f, &settle));
            if let Ok(snap) = ui.snapshot() {
                let _ = ui.activate(&snap);
            }
        }
        let before = last_list_window().expect("list window recorded");
        assert_eq!(before.0, 0, "unscrolled list starts at the top");

        // Wheel down over the list area; lens scrolls with negative scroll_y.
        let mut wheel = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        wheel.set_cursor(600.0, 420.0);
        wheel.set_scroll(0.0, -2.0);
        for _ in 0..4 {
            ui.frame(&wheel, |f| app.build(f, &wheel));
            if let Ok(snap) = ui.snapshot() {
                let _ = ui.activate(&snap);
            }
        }
        let after = last_list_window().expect("list window recorded");
        assert!(
            after.0 > before.0,
            "scroll must advance the window: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn all_three_view_modes_build_headless() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1280.0, 760.0), 1.0 / 60.0);
        for mode in [
            arca_engine::ViewMode::Grid,
            arca_engine::ViewMode::List,
            arca_engine::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            for _ in 0..2 {
                frame(&mut app, &mut ui, &input);
            }
        }
    }

    #[test]
    fn compact_window_builds_every_view() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((720.0, 520.0), 1.0 / 60.0);
        for mode in [
            arca_engine::ViewMode::Grid,
            arca_engine::ViewMode::List,
            arca_engine::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            frame(&mut app, &mut ui, &input);
        }
    }

    /// A listing of a few thousand entries once overflowed lens's 1 MiB
    /// per-frame arena: draw calls past the limit are dropped silently, which
    /// blanked every grid icon. The virtualized listings must stay far below
    /// the budget no matter how large the directory.
    #[test]
    fn large_directory_does_not_overflow_frame_arena() {
        let dir = std::env::temp_dir().join(format!(
            "arca-ui-big-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..2500 {
            std::fs::write(dir.join(format!("file-{i:04}.txt")), b"x").unwrap();
        }
        let config_file = dir.join("cfg").join("arca.conf");
        let state = AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.to_str().unwrap(),
        );
        let mut app = UiApp::new(state);
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1280.0, 760.0), 1.0 / 60.0);

        for mode in [
            arca_engine::ViewMode::Grid,
            arca_engine::ViewMode::List,
            arca_engine::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            let mut overflowed = false;
            for _ in 0..3 {
                ui.frame(&input, |f| {
                    app.build(f, &input);
                });
                overflowed |= ui.overflowed();
            }
            assert!(!overflowed, "{mode:?} overflowed the lens frame arena");
        }
    }

    #[test]
    fn light_and_dark_mode_renders_all_views_and_icons() {
        let (dir, mut app) = fixture();
        std::fs::write(dir.join("photo.png"), b"fake png").unwrap();
        std::fs::write(dir.join("song.mp3"), b"fake mp3").unwrap();
        std::fs::write(dir.join("clip.mp4"), b"fake mp4").unwrap();
        std::fs::write(dir.join("archive.zip"), b"fake zip").unwrap();
        std::fs::write(dir.join("main.rs"), b"fn main() {}").unwrap();
        std::fs::write(dir.join("build.sh"), b"#!/bin/sh\n").unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);

        for theme in [arca_engine::ThemeMode::Light, arca_engine::ThemeMode::Dark] {
            app.state.set_theme(theme);
            for view_mode in [
                arca_engine::ViewMode::Grid,
                arca_engine::ViewMode::List,
                arca_engine::ViewMode::Miller,
            ] {
                app.state.set_view_mode(view_mode);
                frame(&mut app, &mut ui, &input);
            }
        }
    }

    #[test]
    fn grid_cards_with_two_line_names_do_not_overlap_rows() {
        let (dir, mut app) = fixture();
        let names = [
            "浅影阿 - 牡丹亭.flac",
            "21 Savage,J. Cole - a lot.flac",
            "R. Kelly - The World's Greatest.mp3",
            "Janet Jackson - Together Again.flac",
            "Kanye West - Late Registration.mp3",
            "Kanye West - Graduation.mp3",
        ];
        for name in &names {
            std::fs::write(dir.join(name), b"audio data").unwrap();
        }
        app.state.navigate(dir.to_str().unwrap());
        app.state.set_view_mode(arca_engine::ViewMode::Grid);

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((800.0, 600.0), 1.0 / 60.0);

        ui.frame(&input, |f| {
            app.build(f, &input);

            // Test derived typographic metrics on live Frame
            let derived_card_h = crate::content::derive_grid_card_height(f);
            let derived_pitch = crate::content::derive_grid_row_pitch(f);
            assert!(
                derived_card_h >= GRID_DEFAULT_CARD_HEIGHT,
                "derived height ({derived_card_h}) must cover default height"
            );
            assert_eq!(derived_pitch, derived_card_h + GRID_GAP);

            // Verify fluid column allocation
            assert!(
                app.current_grid_columns >= 4,
                "expected at least 4 fluid columns in 800px width, got {}",
                app.current_grid_columns
            );

            // Verify that CJK and Latin names wrap into two lines inside fluid width.
            let text_width = GRID_MIN_CARD_WIDTH
                - (GRID_CARD_PAD * 2.0);
            let cjk_lines = theme::break_lines(
                f,
                names[0],
                GRID_FONT_SIZE,
                text_width,
                GRID_MAX_LABEL_LINES,
            );
            assert_eq!(cjk_lines.len(), 2, "CJK name wraps into 2 lines");
            let cjk_h1 = f.measure_text(&cjk_lines[0], GRID_FONT_SIZE).height;
            let cjk_h2 = f.measure_text(&cjk_lines[1], GRID_FONT_SIZE).height;
            let total_text_h = cjk_h1 + GRID_LABEL_LINE_GAP + cjk_h2;

            // Total card content: pad(4) + art(50) + gap(4) + text + pad(4)
            let total_card_content = (GRID_CARD_PAD * 2.0)
                + GRID_ICON_SIZE
                + GRID_ART_LABEL_GAP
                + total_text_h;
            assert!(
                total_card_content <= derived_card_h,
                "card content ({total_card_content}px) must fit inside derived card height ({derived_card_h}px)"
            );

            // Clearance between row 0 text bottom and row 1 art top
            let row_0_text_bottom = GRID_CARD_PAD
                + GRID_ICON_SIZE
                + GRID_ART_LABEL_GAP
                + total_text_h;
            let row_1_art_top = derived_pitch + GRID_CARD_PAD;
            assert!(
                row_1_art_top > row_0_text_bottom,
                "row 1 preview ({row_1_art_top}px) must be below row 0 text ({row_0_text_bottom}px)"
            );
            let clearance = row_1_art_top - row_0_text_bottom;
            assert!(
                clearance >= 8.0,
                "clearance between row 0 text and row 1 preview should be comfortable, got {clearance}px"
            );
        });
    }

    #[test]
    fn grid_layout_edge_cases_and_interaction_integrity() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        app.state.set_view_mode(arca_engine::ViewMode::Grid);

        // 1. Edge Case: Empty Directory
        let empty_dir = dir.join("empty_folder");
        std::fs::create_dir(&empty_dir).unwrap();
        app.state.navigate(empty_dir.to_str().unwrap());
        let input_std = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input_std);
        assert!(
            !ui.overflowed(),
            "empty directory in grid view should render clean placeholder without arena overflow"
        );

        // 2. Edge Case: Single Item Directory (items < columns)
        let single_dir = dir.join("single_folder");
        std::fs::create_dir(&single_dir).unwrap();
        std::fs::write(single_dir.join("sole_file.txt"), b"only one").unwrap();
        app.state.navigate(single_dir.to_str().unwrap());
        frame(&mut app, &mut ui, &input_std);
        assert!(
            !ui.overflowed(),
            "single item directory in grid view should render properly"
        );

        // 3. Edge Case: Extreme Viewport Resizing (Very Narrow vs Very Wide)
        let input_narrow = lens::Input::new((300.0, 400.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input_narrow);
        assert_eq!(
            app.current_grid_columns, 1,
            "narrow viewport should clamp to 1 column"
        );

        let input_wide = lens::Input::new((2560.0, 1440.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input_wide);
        assert!(
            app.current_grid_columns >= 15,
            "ultrawide viewport should allocate 15+ fluid columns"
        );

        // 4. Edge Case: Keyboard Navigation (2D Grid Step Movement)
        let nav_dir = dir.join("nav_folder");
        std::fs::create_dir(&nav_dir).unwrap();
        for i in 0..12 {
            std::fs::write(nav_dir.join(format!("track_{i:02}.flac")), b"audio").unwrap();
        }
        app.state.navigate(nav_dir.to_str().unwrap());
        let input_nav = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input_nav);
        let cols = app.current_grid_columns;
        assert!(cols >= 4);

        // Move Down from None selects index 0
        let mut down = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        down.push_key(lens::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert_eq!(app.state.selected(), Some(0));

        // Move Down from 0 jumps by `cols`
        frame(&mut app, &mut ui, &down);
        assert_eq!(app.state.selected(), Some(cols));

        // Move Right moves by +1
        let mut right = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        right.push_key(lens::key::RIGHT, true, false);
        frame(&mut app, &mut ui, &right);
        assert_eq!(app.state.selected(), Some(cols + 1));

        // Move Left moves by -1
        let mut left = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        left.push_key(lens::key::LEFT, true, false);
        frame(&mut app, &mut ui, &left);
        assert_eq!(app.state.selected(), Some(cols));

        // Move Up jumps back by `cols` to 0
        let mut up = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        up.push_key(lens::key::UP, true, false);
        frame(&mut app, &mut ui, &up);
        assert_eq!(app.state.selected(), Some(0));

        // 5. Edge Case: Inline Renaming Workflow in Grid View
        app.begin_rename("track_00.flac".to_string());
        assert_eq!(app.renaming.as_deref(), Some("track_00.flac"));
        // Render frame with rename textfield active
        frame(&mut app, &mut ui, &input_nav);
        assert!(
            !ui.overflowed(),
            "rename textfield renders cleanly inside card"
        );

        // Cancel rename with Escape
        let mut esc = lens::Input::new((800.0, 600.0), 1.0 / 60.0);
        esc.push_key(lens::key::ESCAPE, true, false);
        frame(&mut app, &mut ui, &esc);
        assert!(app.renaming.is_none());
    }

    #[test]
    fn visual_hierarchy_metrics_and_borderless_chrome() {
        assert_eq!(GRID_MIN_CARD_WIDTH, 104.0);
        assert_eq!(GRID_MAX_CARD_WIDTH, 144.0);
        assert_eq!(GRID_GAP, 6.0);
        assert_eq!(GRID_DEFAULT_CARD_HEIGHT, 102.0);
        assert_eq!(GRID_DEFAULT_CARD_HEIGHT + GRID_GAP, 108.0);

        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        // Sidebar rows should have compact 30px height
        let places = app
            .drop_targets
            .iter()
            .filter(|t| !t.is_bookmark_zone && t.rect.x < app.sidebar_width)
            .collect::<Vec<_>>();
        assert!(!places.is_empty());
        for target in places {
            assert_eq!(
                target.rect.h, 30.0,
                "sidebar place/bookmark row height should be 30px"
            );
        }
    }

    #[test]
    fn grid_card_desktop_presentation_hover_and_selection_invariants() {
        let (dir, mut app) = fixture();
        let f1 = dir.join("vault");
        let f2 = dir.join("notes.txt");
        std::fs::create_dir(&f1).unwrap();
        std::fs::write(&f2, b"hello").unwrap();

        app.state.navigate(dir.to_str().unwrap());
        app.state.set_view_mode(arca_engine::ViewMode::Grid);

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);

        // Frame 1: Resting state — build initial geometry and settle
        frame(&mut app, &mut ui, &input);
        frame(&mut app, &mut ui, &input);
        assert!(!ui.overflowed(), "grid cards render cleanly at rest");

        // Frame 2: Select the first item ("vault")
        app.state.select_visible(0);
        assert!(app.state.is_selected(0));
        assert!(!app.state.is_selected(1));

        frame(&mut app, &mut ui, &input);
        assert!(
            !ui.overflowed(),
            "selected grid card with label pill renders without arena overflow"
        );

        // Frame 3: Hover over the first card's drop/hit target
        let f1_path = f1.to_string_lossy().into_owned();
        let target = app
            .drop_targets
            .iter()
            .find(|t| !t.is_bookmark_zone && t.target_path == f1_path)
            .cloned()
            .expect("must register navigable directory drop target for vault in grid");

        let vault_idx = app
            .state
            .entries()
            .iter()
            .position(|e| e.name == "vault")
            .expect("vault must exist in entries");

        let mut hover_in = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        hover_in.set_cursor(target.rect.x + 10.0, target.rect.y + 10.0);
        frame(&mut app, &mut ui, &hover_in);

        assert_eq!(
            app.next_hovered_grid_index,
            Some(vault_idx),
            "hovering inside card hit zone must capture hovered grid index (entries: {:?}, target: {:?})",
            app.state
                .entries()
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            target.rect
        );
        assert!(
            !ui.overflowed(),
            "hovered grid card renders cleanly without arena overflow"
        );

        // Frame 4: Run next frame with the hovered index applied
        frame(&mut app, &mut ui, &hover_in);
        assert_eq!(app.hovered_grid_index, Some(vault_idx));
        assert_eq!(ui.cursor_hint(), lens::CursorHint::Default);
        assert!(!app.active_drag.as_ref().is_some_and(|d| d.started));
    }

    #[test]
    fn inline_rename_submit_on_return() {
        let (dir, mut app) = fixture();
        let file_path = dir.join("test_file.txt");
        std::fs::write(&file_path, "hello").unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Start rename
        app.begin_rename("test_file.txt".to_string());
        assert_eq!(app.renaming.as_deref(), Some("test_file.txt"));
        app.rename.set("renamed_file.txt");

        // First frame with rename textfield active
        frame(&mut app, &mut ui, &input);

        // Press Return to submit rename
        let mut enter_input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        enter_input.push_key(lens::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter_input);

        assert!(app.renaming.is_none());
        assert!(dir.join("renamed_file.txt").exists());
        assert!(!dir.join("test_file.txt").exists());
    }

    #[test]
    fn inline_rename_preselects_stem_so_backspace_removes_stem_only() {
        let (dir, mut app) = fixture();
        let file_path = dir.join("report.pdf");
        std::fs::write(&file_path, "pdf data").unwrap();
        app.state.refresh();

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Initiate rename: "report.pdf", stem is "report" (len 6)
        app.begin_rename("report.pdf".to_string());
        assert_eq!(app.renaming.as_deref(), Some("report.pdf"));

        // Frame 1: rename textfield is activated and pre-selects the stem (0..6)
        frame(&mut app, &mut ui, &input);

        // Frame 2: Backspace with selection active deletes "report", leaving ".pdf"
        let mut bs_input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        bs_input.push_key(lens::key::BACKSPACE, true, false);
        frame(&mut app, &mut ui, &bs_input);

        assert_eq!(app.rename.as_str(), ".pdf");
    }
}
