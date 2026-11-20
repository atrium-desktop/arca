//! Shared headless test harness for the `arca-ui` test suites.
//!
//! Every module's `tests` submodule needs the same three things: an isolated
//! `UiApp` over a scratch directory, a way to run one headless frame, and a
//! way to press a key. Keeping one copy here means a change to the harness
//! cannot silently apply to some suites and not others.

use crate::app::UiApp;
use arca_engine::config::Config;
use arca_engine::AppState;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes scratch directories created in the same clock tick. The
/// test harness runs suites concurrently, and a nanosecond timestamp alone
/// can collide when two tests start together.
static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

/// A scratch directory removed when dropped.
///
/// The isolated config file is a sibling of the listing, never a child:
/// a test that persists a setting would otherwise create a `cfg/` directory
/// inside the directory it is inspecting, changing the entry set and every
/// visible index.
pub(crate) struct ScratchDir {
    listing: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl ScratchDir {
    /// Create `prefix-<pid>-<nanos>-<serial>/` under the system temp dir.
    pub(crate) fn new(prefix: &str) -> Self {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let listing = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}-{serial}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&listing).expect("create scratch listing");
        let mut config_name = listing.as_os_str().to_owned();
        config_name.push(".conf");
        Self {
            listing,
            config: std::path::PathBuf::from(config_name),
        }
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.listing
    }

    pub(crate) fn str(&self) -> &str {
        self.listing.to_str().expect("utf-8 scratch path")
    }

    /// Config file outside the listed directory.
    pub(crate) fn config_path(&self) -> &std::path::Path {
        &self.config
    }
}

impl std::ops::Deref for ScratchDir {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.listing
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.listing);
        let _ = std::fs::remove_file(&self.config);
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
    AppState::with_config(
        dir.config_path().to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    )
}

/// Run one headless frame and activate its snapshot.
pub(crate) fn frame(app: &mut UiApp, ui: &mut optics_ui::Ui, input: &optics_ui::Input) {
    ui.frame(input, |f| app.build(f, input));
    if let Ok(snapshot) = ui.snapshot() {
        let _ = ui.activate(&snapshot);
    }
}

/// A settled (no input) frame at the default window size.
pub(crate) fn settle(app: &mut UiApp, ui: &mut optics_ui::Ui) {
    let input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
    frame(app, ui, &input);
}

/// Run `count` settled frames.
pub(crate) fn settle_for(app: &mut UiApp, ui: &mut optics_ui::Ui, count: usize) {
    for _ in 0..count {
        settle(app, ui);
    }
}

/// Press `key` with `mods` in one frame.
pub(crate) fn press(app: &mut UiApp, ui: &mut optics_ui::Ui, key: i32, mods: u32) {
    let mut input = optics_ui::Input::new((1040.0, 700.0), 1.0 / 60.0);
    input.set_mods(mods);
    input.push_key(key, true, false);
    frame(app, ui, &input);
}

/// Open Quick Look on the second row (`a.txt`; directories sort first) and
/// assert it is visible.
pub(crate) fn open_preview(app: &mut UiApp, ui: &mut optics_ui::Ui) {
    app.state.select_visible(1);
    settle(app, ui);
    press(app, ui, ' ' as i32, 0);
    settle_for(app, ui, 10);
    assert!(app.preview.target_open, "preview must open");
    assert!(app.preview.progress > 0.5, "preview must be visible");
}
