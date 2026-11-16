//! Persistent places sidebar with interactive split dragging.

use arca_engine::bookmarks::BookmarkKind;
use lens::patterns::{split_handle_v, SplitOpts};
use lens::{Align, Frame, LayoutOpts};

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
            pad: 6.0,
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
                                    app.state.drop_into(
                                        &payload,
                                        &bookmark.path,
                                        arca_engine::TransferAction::Move,
                                    );
                                }
                            }
                            app.drop_targets.push(crate::app::DropTargetZone {
                                rect: resp.rect,
                                target_path: bookmark.path.clone(),
                                is_bookmark_zone: false,
                            });
                            crate::content::suppress_item_pointer_cursor(frame);
                            frame.pop_id();
                        }

                        let user_bookmarks = bookmarks
                            .iter()
                            .enumerate()
                            .filter(|(_, bookmark)| bookmark.kind == BookmarkKind::Folder)
                            .collect::<Vec<_>>();

                        frame.size_next(0.0, 6.0);
                        frame.spacer(6.0);
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
                                    app.state.drop_into(
                                        &payload,
                                        &bookmark.path,
                                        arca_engine::TransferAction::Move,
                                    );
                                }
                            }
                            app.drop_targets.push(crate::app::DropTargetZone {
                                rect: resp.rect,
                                target_path: bookmark.path.clone(),
                                is_bookmark_zone: false,
                            });
                            crate::content::suppress_item_pointer_cursor(frame);
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
                        let is_target_hovered = is_drag_active
                            && (drop_info.as_ref().map(|d| d.is_hovered).unwrap_or(false)
                                || app.bookmark_drop_rect.is_some_and(|r| {
                                    crate::app::rect_contains(&r, app.cursor_pos)
                                }));

                        let (resp, ()) = frame
                            .row()
                            .id(drop_id)
                            .with_opts(&LayoutOpts {
                                height: 28.0,
                                pad: 4.0,
                                gap: 6.0,
                                cross: Align::Center,
                                bg: if is_target_hovered {
                                    tones.selected
                                } else {
                                    lens::Color::TRANSPARENT
                                },
                                radius: 6.0,
                                ..Default::default()
                            })
                            .show(|frame| {
                                icons::icon(frame, ids::Plus, 13.0);
                                theme::label_colored_sized(
                                    frame,
                                    "Drop to bookmark",
                                    11.0,
                                    tones.muted,
                                );
                            });
                        crate::content::suppress_item_pointer_cursor(frame);

                        app.bookmark_drop_rect = Some(resp.rect);
                        app.drop_targets.push(crate::app::DropTargetZone {
                            rect: resp.rect,
                            target_path: String::new(),
                            is_bookmark_zone: true,
                        });

                        if let Some(path) = target {
                            app.state.navigate(&path);
                            app.location.set(&path);
                            app.location_focused = false;
                            app.location_focused_now = false;
                            app.location_completion = None;
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
    split_handle_v(
        frame,
        "##sidebar-split",
        &mut app.sidebar_width,
        &split_opts,
    );
}

fn section_label(frame: &mut Frame, tones: &Tones, title: &str) {
    frame.size_next(0.0, 18.0);
    frame.row_ex(
        &LayoutOpts {
            height: 18.0,
            pad: 2.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            theme::label_colored_sized(frame, title, 10.0, tones.muted);
        },
    );
}

#[cfg(test)]
mod tests {
    //! Behaviour tests for this module.
    //!
    //! Migrated out of the former monolithic `lib.rs::tests` block so each
    //! test lives next to the code it constrains.

    use crate::test_support::{fixture, frame};

    #[test]
    fn drag_and_drop_to_bookmark_and_folder() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        let sub_path = dir.join("sub").to_str().unwrap().to_string();
        assert!(!app.state.is_bookmarked(&sub_path));

        // Test interactive drag: press on "sub" row, move past threshold to show floating preview
        let sub_r = app.sub_row_rect.expect("sub row rect captured");
        let mut drag_in = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        drag_in.set_cursor(sub_r.x + 20.0, sub_r.y + 10.0);
        drag_in.set_mouse_down(lens::MouseButton::Left, true);
        drag_in.set_mouse_pressed(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &drag_in);
        assert!(app.active_drag.is_some());

        // Move cursor by 30px to trigger drag preview
        drag_in.set_cursor(sub_r.x + 50.0, sub_r.y + 10.0);
        drag_in.set_mouse_pressed(lens::MouseButton::Left, false);
        frame(&mut app, &mut ui, &drag_in);
        assert!(app.active_drag.as_ref().unwrap().started);

        // Move cursor down-left over the bookmark drop zone and release
        let b_r = app.bookmark_drop_rect.expect("bookmark drop rect captured");
        let mut release_in = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release_in.set_cursor(b_r.x + 10.0, b_r.y + 10.0);
        release_in.set_mouse_down(lens::MouseButton::Left, false);
        release_in.set_mouse_released(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release_in);

        assert!(app.active_drag.is_none());
        assert!(app.state.is_bookmarked(&sub_path));

        // Test dropping a file onto a folder in the listing
        let file_path = dir.join("a.txt").to_str().unwrap().to_string();
        assert!(dir.join("a.txt").exists());
        assert!(!dir.join("sub").join("a.txt").exists());

        let sub_rect = app.sub_row_rect.expect("sub row rect captured");
        let sub_pos = (sub_rect.x + sub_rect.w * 0.5, sub_rect.y + sub_rect.h * 0.5);
        ui.deliver_drop(&file_path, sub_pos);
        frame(&mut app, &mut ui, &input);

        assert!(!dir.join("a.txt").exists());
        assert!(dir.join("sub").join("a.txt").exists());

        app.state.toggle_bookmark(&sub_path); // clean up gtk bookmark
    }

    #[test]
    fn sidebar_hover_retains_desktop_arrow_cursor() {
        let (_dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &input);
        }

        let place = app
            .drop_targets
            .iter()
            .find(|t| !t.is_bookmark_zone && t.rect.x < app.sidebar_width)
            .expect("must have at least one sidebar place");

        let mut hover_in = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        hover_in.set_cursor(place.rect.x + 10.0, place.rect.y + 10.0);
        frame(&mut app, &mut ui, &hover_in);

        assert_eq!(
            ui.cursor_hint(),
            lens::CursorHint::Default,
            "hovering sidebar item must preserve standard desktop arrow cursor (not web pointer hand)"
        );
    }
}
