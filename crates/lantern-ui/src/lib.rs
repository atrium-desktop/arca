//! UI layer for Lantern, built on the optics stack (`lens` widgets inside
//! an `iris` window). The business logic lives in `lantern-core`; this
//! crate renders [`AppState`] and forwards input events to it.

mod app;
mod content;
mod device;
mod icons;
mod menus;
mod preview;
mod sidebar;
mod statusbar;
mod tabs;
mod theme;
mod thumbs;
mod toolbar;

pub use app::UiApp;

use lantern_core::state::AppState;

/// Open the Lantern window and run the event loop until it closes.
pub fn run(state: AppState) -> Result<(), iris::RunError> {
    let mut app = UiApp::new(state);
    let config = iris::Config::new("Lantern")?
        .app_id("io.lantern.Lantern")?
        .size(1040, 700);
    iris::Application::run_with_start(
        config,
        // Borrow iris's device for the texture uploads (icon glyphs and
        // thumbnails); a paint callback would also hand it over but disables
        // the idle frame-skip.
        |host| {
            device::set_device(host.flux_device().as_raw() as *mut std::ffi::c_void);
            true
        },
        move |frame, input| app.build(frame, input),
        None::<fn(iris::PaintHost)>,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lantern_core::config::Config;

    /// A temp directory with a couple of entries, wired to an isolated
    /// config file so tests never touch the real user config.
    fn fixture() -> (std::path::PathBuf, UiApp) {
        let dir = std::env::temp_dir().join(format!(
            "lantern-ui-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::write(dir.join(".hidden"), b"h").unwrap();

        let config_file = dir.join("cfg").join("lantern.conf");
        let state = AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.to_str().unwrap(),
        );
        (dir, UiApp::new(state))
    }

    fn frame(app: &mut UiApp, ui: &mut lens::Ui, input: &lens::Input) {
        ui.frame(input, |f| app.build(f, input));
    }

    #[test]
    fn builds_frames_headless() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        // Several frames so the retained tree reconciles enter/stable/leave.
        for _ in 0..3 {
            frame(&mut app, &mut ui, &input);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_h_toggles_hidden_files() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        assert!(!app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 2);

        let mut input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        input.set_mods(lens::mods::CTRL);
        input.push_key('h' as i32, true, false);
        frame(&mut app, &mut ui, &input);

        assert!(app.state.show_hidden);
        assert_eq!(app.state.entries().len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn arrows_select_and_return_enters_directory() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");

        let mut down = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        down.push_key(lens::key::DOWN, true, false);
        frame(&mut app, &mut ui, &down);
        assert_eq!(
            app.state.selected_entry().map(|e| e.name.as_str()),
            Some("sub"),
            "first entry is the sub directory (dirs sort first)"
        );

        let mut enter = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        enter.push_key(lens::key::RETURN, true, false);
        frame(&mut app, &mut ui, &enter);
        assert!(app.state.cwd().ends_with("/sub"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn filter_field_filters_the_list() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        app.state.set_filter("txt".into());
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);
        assert_eq!(app.state.visible().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn all_three_view_modes_build_headless() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1280.0, 760.0), 1.0 / 60.0);
        for mode in [
            lantern_core::ViewMode::Grid,
            lantern_core::ViewMode::List,
            lantern_core::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            for _ in 0..2 {
                frame(&mut app, &mut ui, &input);
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compact_window_builds_every_view() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((720.0, 520.0), 1.0 / 60.0);
        for mode in [
            lantern_core::ViewMode::Grid,
            lantern_core::ViewMode::List,
            lantern_core::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            frame(&mut app, &mut ui, &input);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn settings_button_toggles_appearance_popup() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(app.settings_menu.is_none());

        // The settings button is the right-most 32px icon in the 54px
        // toolbar, which sits below the 42px tab strip.
        let gear = (1016.0, 69.0);
        let click = |app: &mut UiApp, ui: &mut lens::Ui| {
            let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(gear.0, gear.1);
            press.set_mouse_down(lens::MouseButton::Left, true);
            press.set_mouse_pressed(lens::MouseButton::Left, true);
            frame(app, ui, &press);
            let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(gear.0, gear.1);
            release.set_mouse_released(lens::MouseButton::Left, true);
            frame(app, ui, &release);
        };

        click(&mut app, &mut ui);
        assert!(app.settings_menu.is_some(), "gear click opens the popup");
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        assert!(
            app.settings_menu.is_some(),
            "popup stays open across quiet frames"
        );

        click(&mut app, &mut ui);
        assert!(app.settings_menu.is_none(), "second gear click closes it");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn double_click_into_directory_does_not_panic_on_stale_rows() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }

        // Row 0 is the "sub" directory. Two quick clicks on it trigger the
        // double-click open mid-build; the rows built afterwards in the same
        // frame still reference the old (now stale) entry indices.
        let row0 = (500.0, 219.0);
        for _ in 0..2 {
            let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            press.set_cursor(row0.0, row0.1);
            press.set_mouse_down(lens::MouseButton::Left, true);
            press.set_mouse_pressed(lens::MouseButton::Left, true);
            frame(&mut app, &mut ui, &press);
            let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
            release.set_cursor(row0.0, row0.1);
            release.set_mouse_released(lens::MouseButton::Left, true);
            frame(&mut app, &mut ui, &release);
        }
        assert!(app.state.cwd().ends_with("/sub"));
        for _ in 0..3 {
            frame(&mut app, &mut ui, &settle);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_t_opens_tab_and_space_preview_animates() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");

        let mut new_tab = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        new_tab.set_mods(lens::mods::CTRL);
        new_tab.push_key('t' as i32, true, false);
        frame(&mut app, &mut ui, &new_tab);
        assert_eq!(app.state.tabs().len(), 2);

        app.state.move_selection(1);
        let mut space = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        space.push_key(' ' as i32, true, false);
        frame(&mut app, &mut ui, &space);
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..8 {
            frame(&mut app, &mut ui, &settle);
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A listing of a few thousand entries once overflowed lens's 1 MiB
    /// per-frame arena: draw calls past the limit are dropped silently, which
    /// blanked every grid icon. The virtualized listings must stay far below
    /// the budget no matter how large the directory.
    #[test]
    fn large_directory_does_not_overflow_frame_arena() {
        let dir = std::env::temp_dir().join(format!(
            "lantern-ui-big-{}-{}",
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
        let config_file = dir.join("cfg").join("lantern.conf");
        let state = AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.to_str().unwrap(),
        );
        let mut app = UiApp::new(state);
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1280.0, 760.0), 1.0 / 60.0);

        for mode in [
            lantern_core::ViewMode::Grid,
            lantern_core::ViewMode::List,
            lantern_core::ViewMode::Miller,
        ] {
            app.state.set_view_mode(mode);
            let mut overflowed = false;
            for _ in 0..3 {
                ui.frame(&input, |f| {
                    app.build(f, &input);
                    // SAFETY: the frame is live inside the build callback.
                    overflowed |= unsafe { lens_sys::lens_overflowed(f.as_raw()) };
                });
            }
            assert!(!overflowed, "{mode:?} overflowed the lens frame arena");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The window close button sits at the right edge of the 42px tab
    /// strip; `iris_window_close` outside an active app run is a no-op.
    #[test]
    fn window_close_button_is_clickable() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let settle = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        let close = (1019.0, 21.0);
        let mut press = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        press.set_cursor(close.0, close.1);
        press.set_mouse_down(lens::MouseButton::Left, true);
        press.set_mouse_pressed(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &press);
        let mut release = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        release.set_cursor(close.0, close.1);
        release.set_mouse_released(lens::MouseButton::Left, true);
        frame(&mut app, &mut ui, &release);
        for _ in 0..2 {
            frame(&mut app, &mut ui, &settle);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn centered_label_breaks_and_ellipsizes() {
        let (dir, mut app) = fixture();
        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        ui.frame(&input, |f| {
            app.build(f, &input);
            // A short name stays one line.
            let lines = theme::break_lines(f, "a.txt", 12.5, 114.0, 2);
            assert_eq!(lines, ["a.txt"]);
            // A long CJK name clamps to two lines; the last is ellipsized
            // and every line fits the card's text width.
            let long = "龍猛寺寛度 - 武家坡2021 (Official Music Video) extra long suffix.flac";
            let lines = theme::break_lines(f, long, 12.5, 114.0, 2);
            assert_eq!(lines.len(), 2, "clamped to two lines: {lines:?}");
            assert!(lines[1].ends_with('…'), "second line ellipsized: {lines:?}");
            for line in &lines {
                assert!(
                    f.measure_text(line, 12.5).width <= 114.0,
                    "line fits: {line:?}"
                );
            }
            // A single unbreakable token hard-cuts without panicking.
            let token = "supercalifragilisticexpialidocious-supercalifragilisticexpialidocious";
            let lines = theme::break_lines(f, token, 12.5, 114.0, 2);
            assert_eq!(lines.len(), 2);
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn all_icon_assets_register() {
        for &asset in icons::AssetId::ALL {
            let id = icons::lens_id(asset);
            assert_ne!(id.0, u32::MAX, "{asset:?} failed to parse/register");
            assert!(id.0 >= lens_sys::lens_icon_id::LENS_ICON_COUNT.0);
        }
    }

    #[test]
    fn context_menu_and_sidebar_bookmark_flow() {
        let (dir, mut app) = fixture();
        let sub_dir = dir.join("subfolder");
        std::fs::create_dir(&sub_dir).unwrap();
        app.state.navigate(dir.to_str().unwrap());

        let mut ui = lens::Ui::headless().expect("headless ui");
        let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
        frame(&mut app, &mut ui, &input);

        // Add subfolder to bookmarks
        let sub_path = sub_dir.to_str().unwrap();
        assert!(!app.state.is_bookmarked(sub_path));
        app.state.toggle_bookmark(sub_path);
        assert!(app.state.is_bookmarked(sub_path));

        // Rebuild frames with bookmark visible
        frame(&mut app, &mut ui, &input);
        assert!(app.state.bookmarks.iter().any(|b| b.path == sub_path));

        // Remove from bookmarks
        app.state.toggle_bookmark(sub_path);
        assert!(!app.state.is_bookmarked(sub_path));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
