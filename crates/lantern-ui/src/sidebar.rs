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
                if icons::selectable_icon(
                    frame,
                    icons::bookmark_icon(bookmark.kind),
                    &bookmark.name,
                    app.state.cwd() == bookmark.path,
                ) {
                    target = Some(bookmark.path.clone());
                }
                frame.pop_id();
            }

            let pinned = bookmarks
                .iter()
                .enumerate()
                .filter(|(_, bookmark)| bookmark.kind == BookmarkKind::Folder)
                .collect::<Vec<_>>();
            if !pinned.is_empty() {
                frame.size_next(0.0, 12.0);
                frame.spacer(12.0);
                section_label(frame, tones, "PINNED");
                for (index, bookmark) in pinned {
                    frame.push_id(&format!("pinned-{index}"));
                    if icons::selectable_icon(
                        frame,
                        ids::Folder,
                        &bookmark.name,
                        app.state.cwd() == bookmark.path,
                    ) {
                        target = Some(bookmark.path.clone());
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
