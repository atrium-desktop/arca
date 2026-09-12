//! Persistent places sidebar with interactive split dragging.

use iris::{Align, Frame, LayoutOpts};
use lens::patterns::{SplitOpts, split_handle_v};
use arca_engine::bookmarks::BookmarkKind;

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) fn build_sidebar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    let sidebar_w = app.sidebar_width.clamp(160.0, 360.0);
    frame.size_next(sidebar_w, 0.0);
    frame.column_ex(
        &LayoutOpts {
            width: sidebar_w,
            gap: 2.0,
            pad: 8.0,
            cross: Align::Stretch,
            bg: tones.sidebar,
            ..Default::default()
        },
        |frame| {
            frame.flex(1.0);
            frame.scroll("sidebar-scroll", |frame| {
                frame.column_ex(
                    &LayoutOpts {
                        gap: 2.0,
                        pad: 2.0,
                        cross: Align::Stretch,
                        ..Default::default()
                    },
                    |frame| {
                        section_label(frame, tones, "PLACES");
                        let bookmarks = app.state.bookmarks.clone();
                        let mut target = None;
                        for (index, bookmark) in bookmarks.iter().enumerate() {
                            if bookmark.kind == BookmarkKind::Folder {
                                continue;
                            }
                            let place_id = format!("place-{index}");
                            frame.push_id(&place_id);
                            let _ = icons::selectable_icon(
                                frame,
                                icons::bookmark_icon(bookmark.kind),
                                &bookmark.name,
                                app.state.cwd() == bookmark.path,
                            );
                            let resp = frame.response();
                            if resp.clicked {
                                target = Some(bookmark.path.clone());
                            } else if resp.right_clicked {
                                app.open_sidebar_menu(&bookmark.path, false, resp.rect);
                            }
                            if let Some(drop) = frame.dnd_drop_target(&place_id, 1) {
                                if let Some(payload) = drop.payload {
                                    app.state.drop_into(&payload, &bookmark.path);
                                }
                            }
                            app.drop_targets.push(crate::app::DropTargetZone {
                                rect: resp.rect,
                                target_path: bookmark.path.clone(),
                                is_bookmark_zone: false,
                            });
                            frame.pop_id();
                        }

                        let user_bookmarks = bookmarks
                            .iter()
                            .enumerate()
                            .filter(|(_, bookmark)| bookmark.kind == BookmarkKind::Folder)
                            .collect::<Vec<_>>();

                        frame.size_next(0.0, 10.0);
                        frame.spacer(10.0);
                        section_label(frame, tones, "BOOKMARKS");
                        for (index, bookmark) in user_bookmarks {
                            let b_id = format!("bookmark-{index}");
                            frame.push_id(&b_id);
                            let _ = icons::selectable_icon(
                                frame,
                                ids::Folder,
                                &bookmark.name,
                                app.state.cwd() == bookmark.path,
                            );
                            let resp = frame.response();
                            if resp.clicked {
                                target = Some(bookmark.path.clone());
                            } else if resp.right_clicked {
                                app.open_sidebar_menu(&bookmark.path, true, resp.rect);
                            }
                            if let Some(drop) = frame.dnd_drop_target(&b_id, 1) {
                                if let Some(payload) = drop.payload {
                                    app.state.drop_into(&payload, &bookmark.path);
                                }
                            }
                            app.drop_targets.push(crate::app::DropTargetZone {
                                rect: resp.rect,
                                target_path: bookmark.path.clone(),
                                is_bookmark_zone: false,
                            });
                            frame.pop_id();
                        }

                        let drop_id = "sidebar-bookmark-drop-zone";
                        let drop_info = frame.dnd_drop_target(drop_id, 1);
                        if let Some(drop) = &drop_info {
                            if let Some(payload) = &drop.payload {
                                app.state.add_bookmark(payload);
                            }
                        }

                        let is_drag_active = app.active_drag.as_ref().is_some_and(|d| d.started);
                        let is_target_hovered = drop_info.as_ref().map(|d| d.is_hovered).unwrap_or(false)
                            || (is_drag_active
                                && app.bookmark_drop_rect.is_some_and(|r| crate::app::rect_contains(&r, app.cursor_pos)));

                        let (resp, ()) = frame
                            .row()
                            .id(drop_id)
                            .with_opts(&LayoutOpts {
                                height: 28.0,
                                pad: 4.0,
                                gap: 6.0,
                                cross: Align::Center,
                                bg: if is_target_hovered { tones.selected } else { iris::Color::TRANSPARENT },
                                radius: 6.0,
                                ..Default::default()
                            })
                            .show(|frame| {
                                icons::icon(frame, ids::Plus, 13.0);
                                theme::label_colored_sized(frame, "Drop to bookmark", 11.0, tones.muted);
                            });

                        app.bookmark_drop_rect = Some(resp.rect);
                        app.drop_targets.push(crate::app::DropTargetZone {
                            rect: resp.rect,
                            target_path: String::new(),
                            is_bookmark_zone: true,
                        });

                        if let Some(path) = target {
                            app.state.navigate(&path);
                        }
                    },
                );
            });
        },
    );

    // Interactive draggable split divider
    let split_opts = SplitOpts {
        min_size: 160.0,
        max_size: 380.0,
        handle_width: 1.0,
        hit_expand: 4.0,
    };
    split_handle_v(frame, "##sidebar-split", &mut app.sidebar_width, &split_opts);
}

fn section_label(frame: &mut Frame, tones: &Tones, title: &str) {
    frame.size_next(0.0, 20.0);
    frame.row_ex(
        &LayoutOpts {
            height: 20.0,
            pad: 4.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            theme::label_colored_sized(frame, title, 10.5, tones.muted);
        },
    );
}
