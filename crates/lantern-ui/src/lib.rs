//! UI layer for Lantern, built on the optics stack (`lens` widgets inside
//! an `iris` window). The business logic lives in `lantern-core`; this
//! crate renders [`AppState`] and forwards input events to it.

mod app;
mod content;
mod icons;
mod menus;
mod preview;
mod sidebar;
mod statusbar;
mod tabs;
mod theme;
mod toolbar;

pub use app::UiApp;

use lantern_core::state::AppState;

/// Open the Lantern window and run the event loop until it closes.
pub fn run(state: AppState) -> Result<(), iris::RunError> {
    let mut app = UiApp::new(state);
    let config = iris::Config::new("Lantern")?
        .app_id("io.lantern.Lantern")?
        .size(1040, 700);
    iris::Application::run(
        config,
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
}
