//! `AppState`: the whole non-GUI state of the file manager.
//!
//! The UI crate owns one of these, renders it every frame, and calls its
//! methods in response to input. All methods are synchronous and cheap
//! (directory reads of even large folders are milliseconds).

use std::path::PathBuf;

use crate::bookmarks::{self, Bookmark};
use crate::config::{self, Config, ThemeMode, ViewMode};
use crate::entry::{self, Entry, SortKey};
use crate::history::History;
use crate::ops;
use crate::path;
use crate::trash;

/// In-app clipboard for copy/cut + paste.
#[derive(Clone, Debug)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    /// `true` = move on paste (cut), `false` = copy.
    pub cut: bool,
}

/// The complete application state.
pub struct AppState {
    tabs: Vec<TabState>,
    active_tab: usize,
    next_tab_id: u64,
    pub bookmarks: Vec<Bookmark>,
    pub show_hidden: bool,
    pub show_thumbnails: bool,
    pub sort_key: SortKey,
    pub sort_ascending: bool,
    pub theme: ThemeMode,
    pub view_mode: ViewMode,

    pub clipboard: Option<Clipboard>,
    /// One-line feedback for the status bar ("Moved foo to Trash", …).
    pub status: Option<String>,

    config_file: String,
}

/// One independent browsing session. Navigation, selection and filtering are
/// deliberately tab-local; preferences, bookmarks and the clipboard remain
/// application-wide.
#[derive(Clone, Debug)]
pub struct TabState {
    pub id: u64,
    pub cwd: String,
    entries: Vec<Entry>,
    pub read_error: Option<String>,
    history: History,
    /// Index into this tab's visible (filtered) entries.
    pub selected: Option<usize>,
    pub filter: String,
    miller_columns: Vec<MillerColumn>,
}

/// A directory presented in Miller view. The final column is always the
/// active directory; preceding columns are its nearest ancestors.
#[derive(Clone, Debug)]
pub struct MillerColumn {
    pub path: String,
    pub entries: Vec<Entry>,
    /// Entry leading to the next column, or the active tab selection in the
    /// final column. This addresses `entries` directly (no filter).
    pub selected: Option<usize>,
}

impl TabState {
    fn empty(id: u64) -> TabState {
        TabState {
            id,
            cwd: String::new(),
            entries: Vec::new(),
            read_error: None,
            history: History::new(),
            selected: None,
            filter: String::new(),
            miller_columns: Vec::new(),
        }
    }
}

impl AppState {
    /// Load config, build bookmarks, open the home directory.
    pub fn new() -> AppState {
        let config_file = config::config_path();
        let cfg = config::load(&config_file);
        let mut state = AppState {
            tabs: vec![TabState::empty(1)],
            active_tab: 0,
            next_tab_id: 2,
            bookmarks: Vec::new(),
            show_hidden: cfg.show_hidden,
            show_thumbnails: cfg.show_thumbnails,
            sort_key: cfg.sort_key,
            sort_ascending: cfg.sort_ascending,
            theme: cfg.theme,
            view_mode: cfg.view_mode,
            clipboard: None,
            status: None,
            config_file,
        };
        state.rebuild_bookmarks(&cfg.bookmarks);
        state.navigate(&path::home_dir());
        state
    }

    /// Construct with explicit inputs (tests, embedding).
    pub fn with_config(config_file: String, cfg: Config, start_dir: &str) -> AppState {
        let mut state = AppState {
            tabs: vec![TabState::empty(1)],
            active_tab: 0,
            next_tab_id: 2,
            bookmarks: Vec::new(),
            show_hidden: cfg.show_hidden,
            show_thumbnails: cfg.show_thumbnails,
            sort_key: cfg.sort_key,
            sort_ascending: cfg.sort_ascending,
            theme: cfg.theme,
            view_mode: cfg.view_mode,
            clipboard: None,
            status: None,
            config_file,
        };
        state.rebuild_bookmarks(&cfg.bookmarks);
        state.navigate(start_dir);
        state
    }

    fn rebuild_bookmarks(&mut self, user_paths: &[String]) {
        self.bookmarks = bookmarks::standard_bookmarks();
        for path in user_paths {
            if !self.bookmarks.iter().any(|b| b.path == *path) && entry::dir_exists(path) {
                self.bookmarks.push(Bookmark::for_path(path));
            }
        }
    }

    // ---- navigation -----------------------------------------------------

    pub fn tabs(&self) -> &[TabState] {
        &self.tabs
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub fn active_tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_id(&self) -> u64 {
        self.active_tab().id
    }

    pub fn cwd(&self) -> &str {
        &self.active_tab().cwd
    }

    pub fn read_error(&self) -> Option<&str> {
        self.active_tab().read_error.as_deref()
    }

    pub fn filter(&self) -> &str {
        &self.active_tab().filter
    }

    pub fn selected(&self) -> Option<usize> {
        self.active_tab().selected
    }

    pub fn miller_columns(&self) -> &[MillerColumn] {
        &self.active_tab().miller_columns
    }

    /// Open another independent browser tab at `path`, or duplicate the
    /// current location when no path is supplied.
    pub fn new_tab(&mut self, requested: Option<&str>) {
        let target = match requested {
            None => self.cwd().to_string(),
            Some(raw) if raw.starts_with('/') || raw.starts_with('~') => path::normalize(raw),
            Some(raw) => path::resolve(self.cwd(), raw),
        };
        if !entry::dir_exists(&target) {
            self.set_status(format!("Not a directory: {target}"));
            return;
        }
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(TabState::empty(id));
        self.active_tab = self.tabs.len() - 1;
        self.navigate(&target);
        self.set_status("New tab".into());
    }

    /// Close a tab. The final tab is retained so the window never enters an
    /// invalid no-document state.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() == 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.active_tab > index {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.refresh();
        self.set_status("Tab closed".into());
        true
    }

    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_tab)
    }

    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.active_tab {
            return false;
        }
        self.active_tab = index;
        self.refresh();
        true
    }

    pub fn cycle_tab(&mut self, backwards: bool) {
        if self.tabs.len() < 2 {
            return;
        }
        let target = if backwards {
            (self.active_tab + self.tabs.len() - 1) % self.tabs.len()
        } else {
            (self.active_tab + 1) % self.tabs.len()
        };
        self.switch_tab(target);
    }

    /// Navigate to `path` (tilde/relative segments resolved). Records
    /// history on success; reports and stays put on failure.
    pub fn navigate(&mut self, raw: &str) {
        let resolved = if raw.starts_with('/') || raw.starts_with('~') || self.cwd().is_empty() {
            path::normalize(raw)
        } else {
            path::resolve(self.cwd(), raw)
        };
        if !entry::dir_exists(&resolved) {
            self.set_status(format!("Not a directory: {resolved}"));
            return;
        }
        self.tabs[self.active_tab].history.push(&resolved);
        self.set_dir(&resolved);
    }

    fn set_dir(&mut self, path: &str) {
        let listing = entry::read_dir(path, self.show_hidden, self.sort_key, self.sort_ascending);
        let tab = &mut self.tabs[self.active_tab];
        tab.cwd = path.to_string();
        tab.selected = None;
        tab.filter.clear();
        match listing {
            Ok(entries) => {
                tab.entries = entries;
                tab.read_error = None;
            }
            Err(e) => {
                tab.entries.clear();
                tab.read_error = Some(e.to_string());
            }
        }
        self.rebuild_miller_columns();
    }

    pub fn go_back(&mut self) {
        if let Some(path) = self.tabs[self.active_tab]
            .history
            .go_back()
            .map(str::to_string)
        {
            self.set_dir(&path);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(path) = self.tabs[self.active_tab]
            .history
            .go_forward()
            .map(str::to_string)
        {
            self.set_dir(&path);
        }
    }

    pub fn go_up(&mut self) {
        let parent = path::parent(self.cwd());
        if parent != self.cwd() {
            self.navigate(&parent);
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.active_tab().history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.active_tab().history.can_go_forward()
    }

    pub fn refresh(&mut self) {
        let cwd = self.cwd().to_string();
        let filter = self.filter().to_string();
        let selected = self.selected_entry().map(|item| item.name.clone());
        self.set_dir(&cwd);
        self.tabs[self.active_tab].filter = filter;
        if let Some(name) = selected {
            self.select_by_name(&name);
        }
        self.sync_miller_selection();
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let active = self.active_tab;
        for index in 0..self.tabs.len() {
            self.active_tab = index;
            self.refresh();
        }
        self.active_tab = active;
        self.rebuild_miller_columns();
        self.save_config();
    }

    pub fn toggle_thumbnails(&mut self) {
        self.show_thumbnails = !self.show_thumbnails;
        let state = if self.show_thumbnails { "on" } else { "off" };
        self.set_status(format!("Thumbnails {state}"));
        self.save_config();
    }

    // ---- listing --------------------------------------------------------

    /// All entries (unfiltered). Read-only; mutations go through methods.
    pub fn entries(&self) -> &[Entry] {
        &self.active_tab().entries
    }

    /// Indices of entries passing the current filter.
    pub fn visible(&self) -> Vec<usize> {
        entry::filtered_indices(self.entries(), self.filter())
    }

    /// The selected entry, resolved through the filter.
    pub fn selected_entry(&self) -> Option<&Entry> {
        let visible = self.visible();
        let idx = *visible.get(self.selected()?)?;
        self.entries().get(idx)
    }

    /// Absolute path of the selected entry.
    pub fn selected_path(&self) -> Option<String> {
        self.selected_entry()
            .map(|e| entry::child_path(self.cwd(), &e.name))
    }

    /// Move the selection by `delta` rows within the visible list. With no
    /// current selection, `+1` selects the first row and `-1` the last.
    pub fn move_selection(&mut self, delta: i64) {
        let count = self.visible().len();
        if count == 0 {
            self.tabs[self.active_tab].selected = None;
            return;
        }
        let selected = self.selected();
        self.tabs[self.active_tab].selected = Some(match selected {
            None if delta >= 0 => 0,
            None => count - 1,
            Some(cur) => (cur as i64 + delta).clamp(0, count as i64 - 1) as usize,
        });
        self.sync_miller_selection();
    }

    pub fn select_visible(&mut self, index: usize) {
        self.tabs[self.active_tab].selected = (index < self.visible().len()).then_some(index);
        self.sync_miller_selection();
    }

    /// Update the filter text. Any selection is dropped: indices address the
    /// visible list, which the filter just reshaped.
    pub fn set_filter(&mut self, filter: String) {
        let tab = &mut self.tabs[self.active_tab];
        tab.filter = filter;
        tab.selected = None;
        self.sync_miller_selection();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode != mode {
            self.view_mode = mode;
            self.save_config();
        }
    }

    /// Toggle the sort column; same column flips direction.
    pub fn toggle_sort(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_key = key;
            self.sort_ascending = true;
        }
        for tab in &mut self.tabs {
            entry::sort_entries(&mut tab.entries, self.sort_key, self.sort_ascending);
        }
        self.rebuild_miller_columns();
        self.save_config();
    }

    fn rebuild_miller_columns(&mut self) {
        const MAX_COLUMNS: usize = 4;
        let cwd = self.cwd().to_string();
        if cwd.is_empty() {
            return;
        }
        let mut paths = PathBuf::from(&cwd)
            .ancestors()
            .take(MAX_COLUMNS)
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        paths.reverse();

        let current_entries = self.entries().to_vec();
        let mut columns = Vec::with_capacity(paths.len());
        for (index, dir) in paths.iter().enumerate() {
            let entries = if dir == &cwd {
                current_entries.clone()
            } else {
                entry::read_dir(dir, self.show_hidden, self.sort_key, self.sort_ascending)
                    .unwrap_or_default()
            };
            let selected = if let Some(next) = paths.get(index + 1) {
                PathBuf::from(next)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| entries.iter().position(|item| item.name == name))
            } else {
                self.selected()
                    .and_then(|visible| self.visible().get(visible).copied())
            };
            columns.push(MillerColumn {
                path: dir.clone(),
                entries,
                selected,
            });
        }
        self.tabs[self.active_tab].miller_columns = columns;
    }

    fn sync_miller_selection(&mut self) {
        let selected = self
            .selected()
            .and_then(|visible| self.visible().get(visible).copied());
        if let Some(column) = self.tabs[self.active_tab].miller_columns.last_mut() {
            column.selected = selected;
        }
    }

    // ---- actions --------------------------------------------------------

    /// Open the selected entry: navigate into directories, `xdg-open` files.
    pub fn open_selected(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let target = entry::child_path(self.cwd(), &entry.name);
        if entry.navigable {
            self.navigate(&target);
        } else if let Err(e) = ops::open(&target) {
            self.set_status(format!("Cannot open {}: {e}", entry.name));
        }
    }

    /// Open `name` (a child of cwd) — the mouse path of [`Self::open_selected`].
    pub fn open_child(&mut self, name: &str) {
        let target = entry::child_path(self.cwd(), name);
        if entry::dir_exists(&target) {
            self.navigate(&target);
        } else if let Err(e) = ops::open(&target) {
            self.set_status(format!("Cannot open {name}: {e}"));
        }
    }

    /// Activate an item from any Miller column. Directories become the active
    /// location immediately; a file in an ancestor column first makes that
    /// directory active and selects the file for preview/open actions.
    pub fn activate_miller_item(&mut self, directory: &str, name: &str) {
        let target = entry::child_path(directory, name);
        if entry::dir_exists(&target) {
            self.navigate(&target);
            return;
        }
        if self.cwd() != directory {
            self.navigate(directory);
        }
        self.select_by_name(name);
        self.sync_miller_selection();
    }

    /// Create a folder in cwd, select it, and return its name (the UI then
    /// starts an inline rename on it).
    pub fn create_folder(&mut self) -> Option<String> {
        match ops::create_folder(self.cwd()) {
            Ok(path) => {
                let name = path.file_name()?.to_string_lossy().into_owned();
                self.refresh();
                self.select_by_name(&name);
                self.set_status(format!("Created {name}"));
                Some(name)
            }
            Err(e) => {
                self.set_status(format!("Cannot create folder: {e}"));
                None
            }
        }
    }

    /// Rename the entry `from_name` (child of cwd) to `to_name`.
    pub fn rename(&mut self, from_name: &str, to_name: &str) -> bool {
        let to_name = to_name.trim();
        if to_name.is_empty() || to_name == from_name || to_name.contains('/') {
            return false;
        }
        let from = PathBuf::from(entry::child_path(self.cwd(), from_name));
        let to = PathBuf::from(entry::child_path(self.cwd(), to_name));
        match ops::rename_path(&from, &to) {
            Ok(()) => {
                self.refresh();
                self.select_by_name(to_name);
                self.set_status(format!("Renamed {from_name} → {to_name}"));
                true
            }
            Err(e) => {
                self.set_status(format!("Cannot rename {from_name}: {e}"));
                false
            }
        }
    }

    /// Move the selected entry to the trash.
    pub fn trash_selected(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        let name = self
            .selected_entry()
            .map(|e| e.name.clone())
            .unwrap_or_default();
        match trash::trash_paths(&[&path], &trash::trash_root()) {
            Ok(_) => {
                self.set_status(format!("Moved {name} to Trash"));
                self.refresh();
            }
            Err(e) => self.set_status(format!("Cannot trash {name}: {e}")),
        }
    }

    /// Copy (`cut = false`) or cut the selected entry into the clipboard.
    pub fn yank_selected(&mut self, cut: bool) {
        let Some(path) = self.selected_path() else {
            return;
        };
        self.clipboard = Some(Clipboard {
            paths: vec![PathBuf::from(path)],
            cut,
        });
        self.set_status(if cut {
            "Cut — paste with Ctrl+V".to_string()
        } else {
            "Copied — paste with Ctrl+V".to_string()
        });
    }

    /// Paste the clipboard into cwd.
    pub fn paste(&mut self) {
        let Some(clipboard) = self.clipboard.clone() else {
            return;
        };
        let dest_dir = PathBuf::from(self.cwd());
        let mut done = 0usize;
        for src in &clipboard.paths {
            let result = if clipboard.cut {
                ops::move_into(src, &dest_dir)
            } else {
                ops::copy_into(src, &dest_dir)
            };
            match result {
                Ok(_) => done += 1,
                Err(e) => self.set_status(format!(
                    "Cannot paste {}: {e}",
                    src.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        if clipboard.cut && done == clipboard.paths.len() {
            self.clipboard = None;
        }
        if done > 0 {
            self.set_status(format!(
                "{} {done} item{}",
                if clipboard.cut { "Moved" } else { "Copied" },
                if done == 1 { "" } else { "s" }
            ));
            self.refresh();
        }
    }

    // ---- bookmarks & theme ----------------------------------------------

    pub fn is_bookmarked(&self, path: &str) -> bool {
        self.bookmarks.iter().any(|b| b.path == path)
    }

    /// Pin/unpin a path as a user bookmark. Fixed bookmarks (home, XDG
    /// dirs) cannot be removed.
    pub fn toggle_bookmark(&mut self, path: &str) {
        let path = path::normalize(path);
        if let Some(pos) = self.bookmarks.iter().position(|b| b.path == path) {
            if self.bookmarks[pos].kind == bookmarks::BookmarkKind::Folder {
                self.bookmarks.remove(pos);
                self.set_status(format!("Removed bookmark {path}"));
            }
        } else {
            self.bookmarks.push(Bookmark::for_path(&path));
            self.set_status(format!("Bookmarked {path}"));
        }
        self.save_config();
    }

    /// Set an explicit colour-scheme preference and persist it.
    pub fn set_theme(&mut self, mode: ThemeMode) {
        if self.theme == mode {
            return;
        }
        self.theme = mode;
        self.save_config();
    }

    // ---- misc -----------------------------------------------------------

    pub fn set_status(&mut self, msg: String) {
        self.status = Some(msg);
    }

    fn select_by_name(&mut self, name: &str) {
        let visible = self.visible();
        let selected = visible.iter().position(|&i| self.entries()[i].name == name);
        self.tabs[self.active_tab].selected = selected;
    }

    /// Persist the current settings (hidden/thumbnails/sort/theme/bookmarks).
    pub fn save_config(&self) {
        let user_bookmarks = self
            .bookmarks
            .iter()
            .filter(|b| b.kind == bookmarks::BookmarkKind::Folder)
            .map(|b| b.path.clone())
            .collect();
        let cfg = Config {
            show_hidden: self.show_hidden,
            show_thumbnails: self.show_thumbnails,
            sort_key: self.sort_key,
            sort_ascending: self.sort_ascending,
            theme: self.theme,
            view_mode: self.view_mode,
            bookmarks: user_bookmarks,
        };
        // Config write failure is not worth surfacing in a file manager.
        let _ = config::save(&self.config_file, &cfg);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn fixture() -> (PathBuf, AppState) {
        let dir = std::env::temp_dir().join(format!(
            "lantern-state-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::write(dir.join("b.txt"), b"b").unwrap();
        std::fs::write(dir.join(".hidden"), b"h").unwrap();

        let config_file = dir.join("cfg").join("lantern.conf");
        let state = AppState::with_config(
            config_file.to_str().unwrap().into(),
            Config::default(),
            dir.to_str().unwrap(),
        );
        (dir, state)
    }

    #[test]
    fn set_theme_persists_choice() {
        let (dir, mut s) = fixture();
        assert_eq!(s.theme, ThemeMode::System);

        s.set_theme(ThemeMode::Dark);
        assert_eq!(s.theme, ThemeMode::Dark);
        let saved = std::fs::read_to_string(dir.join("cfg").join("lantern.conf")).unwrap();
        assert!(saved.contains("theme = dark"));

        // Re-selecting the active mode is a no-op.
        s.set_theme(ThemeMode::Dark);
        assert_eq!(s.theme, ThemeMode::Dark);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn navigate_and_history() {
        let (dir, mut s) = fixture();
        assert_eq!(s.cwd(), dir.to_str().unwrap());
        assert_eq!(s.entries().len(), 3); // sub, a.txt, b.txt (hidden off)

        s.navigate("sub");
        assert!(s.cwd().ends_with("/sub"));
        assert!(s.can_go_back());

        s.go_up();
        assert_eq!(s.cwd(), dir.to_str().unwrap());

        s.go_back();
        assert!(s.cwd().ends_with("/sub"));
        s.go_forward();
        assert_eq!(s.cwd(), dir.to_str().unwrap());

        s.navigate("/definitely/not/here");
        assert_eq!(s.cwd(), dir.to_str().unwrap(), "failed nav stays put");
        assert!(s.status.is_some());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn selection_and_filter() {
        let (dir, mut s) = fixture();
        s.move_selection(1);
        assert_eq!(s.selected_entry().map(|e| e.name.as_str()), Some("sub"));
        s.move_selection(1);
        assert_eq!(s.selected_entry().map(|e| e.name.as_str()), Some("a.txt"));
        s.move_selection(-1);
        assert_eq!(s.selected_entry().map(|e| e.name.as_str()), Some("sub"));

        s.set_filter("txt".into());
        assert_eq!(s.visible().len(), 2);
        assert!(s.selected().is_none(), "stale selection dropped by filter");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hidden_toggle_persists() {
        let (dir, mut s) = fixture();
        assert_eq!(s.entries().len(), 3);
        s.toggle_hidden();
        assert_eq!(s.entries().len(), 4);
        // Config was written and round-trips.
        let cfg = config::load(dir.join("cfg/lantern.conf").to_str().unwrap());
        assert!(cfg.show_hidden);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_rename_trash_flow() {
        let (dir, mut s) = fixture();
        let trash_root = dir.join("trash");

        let name = s.create_folder().unwrap();
        assert_eq!(name, "New Folder");
        assert!(dir.join("New Folder").is_dir());
        assert_eq!(
            s.selected_entry().map(|e| e.name.as_str()),
            Some("New Folder")
        );

        assert!(s.rename("New Folder", "Renamed"));
        assert!(dir.join("Renamed").is_dir());

        // Trash directly (state.trash_selected uses the real user trash).
        let victim = dir.join("Renamed");
        crate::trash::trash_paths(&[&victim], &trash_root).unwrap();
        assert!(!victim.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn copy_cut_paste() {
        let (dir, mut s) = fixture();
        std::fs::create_dir_all(dir.join("dst")).unwrap();

        // Select a.txt, copy, go into dst, paste.
        s.select_by_name("a.txt");
        s.yank_selected(false);
        s.navigate("dst");
        s.paste();
        assert!(dir.join("dst/a.txt").exists());
        assert!(dir.join("a.txt").exists(), "copy keeps the source");

        // Cut b.txt, paste into dst.
        s.go_up();
        s.select_by_name("b.txt");
        s.yank_selected(true);
        s.navigate("dst");
        s.paste();
        assert!(dir.join("dst/b.txt").exists());
        assert!(!dir.join("b.txt").exists(), "cut removes the source");
        assert!(s.clipboard.is_none(), "cut clipboard consumed after paste");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bookmark_toggle_roundtrip() {
        let (dir, mut s) = fixture();
        let target = dir.join("sub").to_str().unwrap().to_string();
        assert!(!s.is_bookmarked(&target));
        s.toggle_bookmark(&target);
        assert!(s.is_bookmarked(&target));

        let cfg = config::load(dir.join("cfg/lantern.conf").to_str().unwrap());
        assert_eq!(cfg.bookmarks, std::slice::from_ref(&target));

        s.toggle_bookmark(&target);
        assert!(!s.is_bookmarked(&target));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sort_toggle_flips_direction() {
        let (dir, mut s) = fixture();
        assert_eq!(s.sort_key, SortKey::Name);
        s.toggle_sort(SortKey::Size);
        assert_eq!(s.sort_key, SortKey::Size);
        assert!(s.sort_ascending);
        s.toggle_sort(SortKey::Size);
        assert!(!s.sort_ascending);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tabs_keep_independent_navigation_filter_and_selection() {
        let (dir, mut s) = fixture();
        s.set_filter("txt".into());
        s.move_selection(1);
        let first_id = s.active_tab_id();

        s.new_tab(Some(dir.join("sub").to_str().unwrap()));
        assert_eq!(s.tabs().len(), 2);
        assert_ne!(s.active_tab_id(), first_id);
        assert!(s.cwd().ends_with("/sub"));
        assert!(s.filter().is_empty());

        assert!(s.switch_tab(0));
        assert_eq!(s.active_tab_id(), first_id);
        assert_eq!(s.cwd(), dir.to_str().unwrap());
        assert_eq!(s.filter(), "txt");
        assert_eq!(
            s.selected_entry().map(|entry| entry.name.as_str()),
            Some("a.txt")
        );

        assert!(s.close_active_tab());
        assert_eq!(s.tabs().len(), 1);
        assert!(s.cwd().ends_with("/sub"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn miller_columns_end_at_active_directory() {
        let (dir, mut s) = fixture();
        s.navigate("sub");
        let columns = s.miller_columns();
        assert!(!columns.is_empty());
        assert_eq!(columns.last().unwrap().path, s.cwd());
        if columns.len() > 1 {
            let parent = &columns[columns.len() - 2];
            let selected = parent.selected.expect("parent selects active child");
            assert_eq!(parent.entries[selected].name, "sub");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn view_mode_persists() {
        let (dir, mut s) = fixture();
        s.set_view_mode(ViewMode::Miller);
        let cfg = config::load(dir.join("cfg/lantern.conf").to_str().unwrap());
        assert_eq!(cfg.view_mode, ViewMode::Miller);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
