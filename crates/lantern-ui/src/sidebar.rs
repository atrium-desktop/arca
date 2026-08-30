//! Persistent places sidebar.

use iris::{Align, Frame, LayoutOpts};
use lantern_core::bookmarks::BookmarkKind;

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::{self, Tones};

pub(crate) fn build_sidebar(app: &mut UiApp, frame: &mut Frame, tones: &Tones) {
    frame.size_next(212.0, 0.0);
    frame.column_ex(
        &LayoutOpts {
            width: 212.0,
            gap: 3.0,
            pad: 10.0,
            cross: Align::Stretch,
            bg: tones.sidebar,
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
                frame.size_next(0.0, 12.0);
                frame.spacer(12.0);
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
}

fn section_label(frame: &mut Frame, tones: &Tones, label: &str) {
    frame.size_next(0.0, 26.0);
    frame.row_ex(
        &LayoutOpts {
            height: 26.0,
            pad: 5.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| theme::label_colored_sized(frame, label, 10.5, tones.muted),
    );
}
