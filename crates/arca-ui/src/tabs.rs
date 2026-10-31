//! Browser tab strip using the lens TabStrip pattern.

use iris::{Align, Frame, LayoutOpts};
use lens::patterns::{TabAction, TabItem, TabStrip};

use crate::app::UiApp;
use crate::icons::{self, ids};
use crate::theme::Tones;

fn tab_title(cwd: &str) -> String {
    let name = std::path::Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "/".into());
    name
}

pub(crate) fn build_tabs(app: &mut UiApp, frame: &mut Frame, input: &iris::Input, _tones: &Tones) {
    frame.size_next(0.0, 38.0);
    frame.row_ex(
        &LayoutOpts {
            gap: 6.0,
            pad: 4.0,
            cross: Align::Center,
            ..Default::default()
        },
        |frame| {
            let tabs: Vec<TabItem> = app
                .state
                .tabs()
                .iter()
                .map(|tab| {
                    TabItem::new(tab_title(&tab.cwd))
                        .icon(icons::lens_id(ids::Folder))
                        .closable(app.state.tabs().len() > 1)
                })
                .collect();

            let active_idx = app.state.active_tab_index();

            let strip = TabStrip::new("##main-tabs")
                .height(30.0)
                .min_tab_width(100.0)
                .max_tab_width(180.0)
                .show_new_button(true)
                .close_icon(icons::lens_id(ids::X))
                .new_icon(icons::lens_id(ids::Plus));

            let action = strip.show(frame, &tabs, active_idx);

            match action {
                TabAction::Select(index) => app.switch_tab(index),
                TabAction::Close(index) => app.close_tab(index),
                TabAction::NewTab => app.new_tab(),
                // Post-removal coordinates straight from the lens strip;
                // engine applies remove-then-insert verbatim.
                TabAction::Move(from, to) => app.move_tab(from, to),
                // Press frame inside the strip: no model change; the CSD
                // logic below only reads the flag to suppress the move grab.
                TabAction::PressedOnTab => {}
                TabAction::None => {}
            }

            // Remainder space in the tab strip (pure layout flex spacer, not a widget)
            frame.flex(1.0);
            frame.spacer(0.0);

            // Window close button at the far right
            let close_clicked = icons::icon_button(frame, ids::X, 26.0);
            if close_clicked {
                iris::window_close();
            }

            // Window drag behavior (CSD):
            // The top bar background is purely non-interactive (not a button widget,
            // no hover, no focus outline). When pointer press occurs on the top bar
            // area outside the close button, request interactive move from compositor.
            // A press that landed on the strip itself (tab / close / new button)
            // suppresses the move grab: the strip owns the gesture — a tab drag
            // must reorder tabs, not move the window.
            let raw_in = input.as_raw();
            if raw_in.mouse_pressed[0]
                && !close_clicked
                && !matches!(action, TabAction::PressedOnTab)
            {
                let cursor = raw_in.cursor;
                let display = raw_in.display_size;
                if cursor.y >= 0.0
                    && cursor.y <= 38.0
                    && cursor.x >= 0.0
                    && cursor.x < display.x - 34.0
                {
                    iris::window_start_move();
                }
            }
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

    /// Drag-to-reorder end-to-end through the headless UI: press on the
    /// first tab, move past the 14px threshold onto the second tab's slot,
    /// release. The engine model must reflect the new order and the dragged
    /// tab must stay active. Tab rows live in the 30px strip inside the 38px
    /// bar; with min width 100 + close button, tab centres sit near
    /// y = 21 and x ≈ 58 / 168.
    #[test]
    fn tab_drag_reorder_updates_tab_order() {
        let (_dir, mut app) = fixture();
        let cwd = app.state.cwd().to_string();
        app.new_tab_at(&cwd); // second tab, same directory
        assert_eq!(app.state.tabs().len(), 2);
        let original_ids: Vec<u64> = app.state.tabs().iter().map(|t| t.id).collect();

        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        let mk_input = |x: f32, down: bool, pressed: bool, released: bool| {
            let mut i = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            i.set_cursor(x, 21.0);
            i.set_mouse_down(lens::MouseButton::Left, down);
            i.set_mouse_pressed(lens::MouseButton::Left, pressed);
            i.set_mouse_released(lens::MouseButton::Left, released);
            i
        };

        // Press on tab 0, hold; frames advance the drag state machine.
        let press = mk_input(58.0, true, true, false);
        frame(&mut app, &mut ui, &press);
        // Cross the threshold while held.
        let drag = mk_input(58.0, true, false, false);
        frame(&mut app, &mut ui, &drag);
        // Move into tab 1's slot while still held.
        let drag = mk_input(168.0, true, false, false);
        frame(&mut app, &mut ui, &drag);
        // Release: MOVE(0 -> 1) applies and tab 0 remains active.
        let release = mk_input(168.0, false, false, true);
        frame(&mut app, &mut ui, &release);

        let ids: Vec<u64> = app.state.tabs().iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![original_ids[1], original_ids[0]]);
        // The active tab follows its document: the new tab was active before
        // the drag and simply moved with the reorder (no SELECT fires).
        assert_eq!(app.state.active_tab_id(), original_ids[1]);
    }

    /// Hovering over a tab's close button and clicking it closes that tab.
    #[test]
    fn tab_close_button_closes_tab() {
        let (_dir, mut app) = fixture();
        let cwd = app.state.cwd().to_string();
        app.new_tab_at(&cwd);
        assert_eq!(app.state.tabs().len(), 2);
        let remaining_id = app.state.tabs()[1].id;

        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        // Tab 0 has cwd "arca-ui-test-..." which maxes out at max_tab_width = 180.
        // It starts at x = 7, so it spans x = 7..187; its close button is at x ≈ 165..181, y ≈ 21.
        let close_pt = (173.0, 21.0);
        let mut hover = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        hover.set_cursor(close_pt.0, close_pt.1);
        frame(&mut app, &mut ui, &hover);

        let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(close_pt.0, close_pt.1);
        press.set_mouse_down(lens::MouseButton::Left, true);
        press.set_mouse_pressed(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);

        let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(close_pt.0, close_pt.1);
        release.set_mouse_released(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);

        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        assert_eq!(app.state.tabs().len(), 1);
        assert_eq!(app.state.tabs()[0].id, remaining_id);
    }
}
