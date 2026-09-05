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
                            frame.push_id(&format!("place-{index}"));
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
                            frame.pop_id();
                        }

                        let user_bookmarks = bookmarks
                            .iter()
                            .enumerate()
                            .filter(|(_, bookmark)| bookmark.kind == BookmarkKind::Folder)
                            .collect::<Vec<_>>();
                        if !user_bookmarks.is_empty() {
                            frame.size_next(0.0, 10.0);
                            frame.spacer(10.0);
                            section_label(frame, tones, "BOOKMARKS");
                            for (index, bookmark) in user_bookmarks {
                                frame.push_id(&format!("bookmark-{index}"));
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
                                frame.pop_id();
                            }
                        }

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
