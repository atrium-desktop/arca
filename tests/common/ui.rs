//! Headless `UiApp` harness for the external integration suite.
//!
//! Crate-local unit tests use `crate::test_support`, which can reach private
//! fields. This harness is deliberately built only from the public API, so an
//! integration test proves the same surface an embedder or a future shell
//! would use: `AppState::with_config` + `UiApp::with_thumb_cache` +
//! `Ui::headless()`.

use arca_engine::config::{Config, ViewMode};
use arca_engine::AppState;
use arca_ui::UiApp;
use optics_ui::{Frame, Input, Ui};
use test_support::fs::TestDir;

pub const VIEWPORT: (f32, f32) = (1040.0, 700.0);
pub const DT: f32 = 1.0 / 60.0;

/// A scratch directory plus a headless application and UI.
pub struct Harness {
    pub dir: TestDir,
    /// The config file and thumbnail cache live in sibling scratch
    /// directories: neither a config write nor a cache prune may appear as an
    /// entry in the directory under inspection, and tests must never touch the
    /// user's real thumbnail cache.
    _config_dir: TestDir,
    _cache_dir: TestDir,
    pub app: UiApp,
    pub ui: Ui,
}

impl Harness {
    /// An isolated application rooted at a scratch directory containing
    /// `sub/`, `a.txt`, and `.hidden`.
    pub fn new(prefix: &str) -> Self {
        let dir = TestDir::new(prefix);
        dir.subdir("sub");
        dir.write("a.txt", b"a");
        dir.write(".hidden", b"h");
        Self::build(dir, prefix)
    }

    /// An isolated application rooted at `dir`, with no fixture files.
    pub fn at(prefix: &str) -> Self {
        let dir = TestDir::new(prefix);
        Self::build(dir, prefix)
    }

    fn build(dir: TestDir, prefix: &str) -> Self {
        let config_dir = TestDir::new(&format!("{prefix}-config"));
        let cache_dir = TestDir::new(&format!("{prefix}-cache"));
        let state = AppState::with_config(
            config_dir.join("arca.conf").to_str().unwrap().into(),
            Config::default(),
            dir.str(),
        );
        let app = UiApp::with_thumb_cache(state, cache_dir.path()).sync_navigation();
        let ui = Ui::headless().expect("headless optics-ui");
        Self {
            dir,
            _config_dir: config_dir,
            _cache_dir: cache_dir,
            app,
            ui,
        }
    }

    pub fn set_view(&mut self, mode: ViewMode) {
        self.app.state.set_view_mode(mode);
    }

    /// Run one frame and activate its snapshot.
    pub fn frame(&mut self, input: &Input) {
        self.ui.frame(input, |frame: &mut Frame| {
            self.app.build(frame, input);
        });
        if let Ok(snapshot) = self.ui.snapshot() {
            let _ = self.ui.activate(&snapshot);
        }
    }

    /// A settled (no input) frame.
    pub fn settle(&mut self) {
        self.frame(&Input::new(VIEWPORT, DT));
    }

    pub fn settle_for(&mut self, frames: usize) {
        for _ in 0..frames {
            self.settle();
        }
    }

    /// Press `key` with `mods` in one frame.
    pub fn press(&mut self, key: i32, mods: u32) {
        let mut input = Input::new(VIEWPORT, DT);
        input.set_mods(mods);
        input.push_key(key, true, false);
        self.frame(&input);
    }

    /// One press/release pair at `(x, y)`.
    pub fn click(&mut self, x: f32, y: f32) {
        let mut press = Input::new(VIEWPORT, DT);
        press.set_cursor(x, y);
        press.set_mouse_down(optics_ui::MouseButton::Left, true);
        press.set_mouse_pressed(optics_ui::MouseButton::Left, true);
        self.frame(&press);

        let mut release = Input::new(VIEWPORT, DT);
        release.set_cursor(x, y);
        release.set_mouse_released(optics_ui::MouseButton::Left, true);
        self.frame(&release);
    }

    /// One right-click pair at `(x, y)`.
    pub fn right_click(&mut self, x: f32, y: f32) {
        let mut press = Input::new(VIEWPORT, DT);
        press.set_cursor(x, y);
        press.set_mouse_down(optics_ui::MouseButton::Right, true);
        press.set_mouse_pressed(optics_ui::MouseButton::Right, true);
        self.frame(&press);

        let mut release = Input::new(VIEWPORT, DT);
        release.set_cursor(x, y);
        release.set_mouse_released(optics_ui::MouseButton::Right, true);
        self.frame(&release);
    }

    /// Two click pairs inside the double-click window.
    pub fn double_click(&mut self, x: f32, y: f32) {
        self.click(x, y);
        self.click(x, y);
    }

    /// Enable the production asynchronous navigation policy for this app.
    pub fn enable_async_navigation(&mut self) {
        self.app.set_async_navigation(true);
    }

    /// Run frames until an asynchronous navigation publishes or fails.
    pub fn settle_navigation(&mut self) {
        for _ in 0..500 {
            if !self.app.state.is_navigating() {
                self.settle();
                return;
            }
            self.settle();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("asynchronous navigation did not settle");
    }

    /// Hover without pressing.
    pub fn hover(&mut self, x: f32, y: f32) {
        let mut input = Input::new(VIEWPORT, DT);
        input.set_cursor(x, y);
        self.frame(&input);
    }
}

/// Coordinates of the first list row at the default viewport.
///
/// The chrome is 38 px tabs + 48 px toolbar; the content column pads 12 px and
/// puts a 32 px header above the virtualized scroll, which pads 2 px. Row 0 is
/// therefore 42 px tall starting near y = 140.
pub const FIRST_LIST_ROW: (f32, f32) = (500.0, 155.0);
pub const LIST_ROW_PITCH: f32 = 44.0;
pub const TOOLBAR_GEAR: (f32, f32) = (1016.0, 69.0);
