//! Shared headless test harness for the `arca-ui` test suites.
//!
//! Every module's `tests` submodule needs the same three things: an isolated
//! `UiApp` over a scratch directory, a way to run one headless frame, and a
//! way to press a key. Keeping one copy here means a change to the harness
//! cannot silently apply to some suites and not others.

use crate::app::UiApp;
use arca_engine::config::Config;
use arca_engine::AppState;

/// A scratch directory removed when dropped.
pub(crate) struct ScratchDir(pub std::path::PathBuf);

impl ScratchDir {
    /// Create `prefix-<pid>-<nanos>/` under the system temp dir.
    pub(crate) fn new(prefix: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.0
    }

    pub(crate) fn str(&self) -> &str {
        self.0.to_str().expect("utf-8 scratch path")
    }
}

impl std::ops::Deref for ScratchDir {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scratch directory with `sub/`, `a.txt`, and `.hidden`, plus an isolated
/// config file so no test touches the real user configuration.
pub(crate) fn fixture() -> (ScratchDir, UiApp) {
    let dir = ScratchDir::new("arca-ui-test");
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
    std::fs::write(dir.path().join(".hidden"), b"h").unwrap();
    let state = state_for(&dir);
    (dir, UiApp::new(state))
}

/// A scratch directory holding `sub/`, `a.txt` and `b.txt`.
pub(crate) fn fixture_two_files() -> (ScratchDir, UiApp) {
    let dir = ScratchDir::new("arca-ui-two");
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
    let state = state_for(&dir);
    (dir, UiApp::new(state))
}

/// An `AppState` rooted at `dir` with an isolated config file.
pub(crate) fn state_for(dir: &ScratchDir) -> AppState {
    let config_file = dir.path().join("cfg").join("arca.conf");
    AppState::with_config(
        config_file.to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    )
}

/// Run one headless frame and activate its snapshot.
pub(crate) fn frame(app: &mut UiApp, ui: &mut lens::Ui, input: &lens::Input) {
    ui.frame(input, |f| app.build(f, input));
    if let Ok(snapshot) = ui.snapshot() {
        let _ = ui.activate(&snapshot);
    }
}

/// A settled (no input) frame at the default window size.
pub(crate) fn settle(app: &mut UiApp, ui: &mut lens::Ui) {
    let input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
    frame(app, ui, &input);
}

/// Run `count` settled frames.
pub(crate) fn settle_for(app: &mut UiApp, ui: &mut lens::Ui, count: usize) {
    for _ in 0..count {
        settle(app, ui);
    }
}

/// Press `key` with `mods` in one frame.
pub(crate) fn press(app: &mut UiApp, ui: &mut lens::Ui, key: i32, mods: u32) {
    let mut input = lens::Input::new((1040.0, 700.0), 1.0 / 60.0);
    input.set_mods(mods);
    input.push_key(key, true, false);
    frame(app, ui, &input);
}

/// Open Quick Look on the second row (`a.txt`; directories sort first) and
/// assert it is visible.
pub(crate) fn open_preview(app: &mut UiApp, ui: &mut lens::Ui) {
    app.state.select_visible(1);
    settle(app, ui);
    press(app, ui, ' ' as i32, 0);
    settle_for(app, ui, 10);
    assert!(app.preview.target_open, "preview must open");
    assert!(app.preview.progress > 0.5, "preview must be visible");
}
