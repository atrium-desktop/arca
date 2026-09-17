//! `AppState`: the whole non-GUI state of the file manager.
//!
//! The UI crate owns one of these, renders it every frame, and calls its
//! methods in response to input. All methods are synchronous and cheap
//! (directory reads of even large folders are milliseconds).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::bookmarks::{self, Bookmark};
use crate::chooser::FileFilter;
use crate::config::{self, Config, ThemeMode, ViewMode};
use crate::entry::{self, Entry, SortKey};
use crate::history::History;
use crate::ops;
use crate::path;
use crate::trash;
use crate::watch::{FsWatcher, WakeFn};

/// Clipboard supporting standard Linux / XDG file transfer formats
/// (`x-special/gnome-copied-files` and `text/uri-list`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    /// `true` = move on paste (cut), `false` = copy.
    pub cut: bool,
}

impl Clipboard {
    pub fn new(paths: Vec<PathBuf>, cut: bool) -> Self {
        Self { paths, cut }
    }

    /// Format as standard Linux desktop `x-special/gnome-copied-files`.
    pub fn to_gnome_copied_files(&self) -> String {
        let action = if self.cut { "cut" } else { "copy" };
        let mut out = format!("{action}\n");
        for p in &self.paths {
            out.push_str(&crate::xdg::encode_file_uri(p));
            out.push('\n');
        }
        out
    }

    /// Format as standard RFC 2483 / XDG `text/uri-list`.
    pub fn to_uri_list(&self) -> String {
        let mut out = String::new();
        for p in &self.paths {
            out.push_str(&crate::xdg::encode_file_uri(p));
            out.push_str("\r\n");
        }
        out
    }

    /// Parse clipboard payload from system clipboard string.
    /// Handles `x-special/gnome-copied-files`, `text/uri-list`, and raw paths.
    pub fn from_text_payload(text: &str) -> Option<Clipboard> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }

        let mut lines = text.lines();
        let first = lines.next()?.trim();

        let (cut, remaining_lines) = if first == "cut" {
            (true, lines.collect::<Vec<_>>())
        } else if first == "copy" {
            (false, lines.collect::<Vec<_>>())
        } else {
            let mut l = vec![first];
            l.extend(lines);
            (false, l)
        };

        let mut paths = Vec::new();
        for line in remaining_lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(p) = crate::xdg::decode_file_uri(line) {
                if p.exists() {
                    paths.push(p);
                }
            } else if line.starts_with('/') {
                let p = PathBuf::from(line);
                if p.exists() {
                    paths.push(p);
                }
            }
        }

        if paths.is_empty() {
            None
        } else {
            Some(Clipboard { paths, cut })
        }
    }
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
    pub file_filter: Option<FileFilter>,
    pub undo_stack: crate::undo::UndoStack,
    pub io_engine: crate::worker::AsyncIoEngine,

    config_file: String,
    pub watcher: Option<FsWatcher>,
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
    /// Index into this tab's visible (filtered) entries (primary cursor).
    pub selected: Option<usize>,
    /// Set of selected indices for multi-selection.
    pub selected_indices: BTreeSet<usize>,
    /// Anchor index for range selection.
    pub anchor: Option<usize>,
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
            selected_indices: BTreeSet::new(),
            anchor: None,
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
            file_filter: None,
            undo_stack: crate::undo::UndoStack::new(),
            io_engine: crate::worker::AsyncIoEngine::new(),
            config_file,
            watcher: FsWatcher::new().ok(),
        };
        state.rebuild_bookmarks(&cfg.bookmarks);
        state.navigate(&path::home_dir());
        state.sync_fs_watches();
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
            file_filter: None,
            undo_stack: crate::undo::UndoStack::new(),
            io_engine: crate::worker::AsyncIoEngine::new(),
            config_file,
            watcher: FsWatcher::new().ok(),
        };
        state.rebuild_bookmarks(&cfg.bookmarks);
        state.navigate(start_dir);
        state.sync_fs_watches();
        state
    }

    fn rebuild_bookmarks(&mut self, user_paths: &[String]) {
        self.bookmarks = bookmarks::standard_bookmarks();
        let home = path::home_dir();
        for bookmark in bookmarks::read_gtk_bookmarks_file(&home) {
            if !self.bookmarks.iter().any(|b| b.path == bookmark.path) && entry::dir_exists(&bookmark.path) {
                self.bookmarks.push(bookmark);
            }
        }
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
        tab.selected_indices.clear();
        tab.anchor = None;
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
        self.sync_fs_watches();
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

    // ---- filesystem watcher ---------------------------------------------

    /// All directories currently displayed in the UI across tabs and Miller columns.
    pub fn active_view_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for tab in &self.tabs {
            if !tab.cwd.is_empty() {
                let p = PathBuf::from(&tab.cwd);
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
        if self.view_mode == ViewMode::Miller {
            for col in &self.active_tab().miller_columns {
                if !col.path.is_empty() {
                    let p = PathBuf::from(&col.path);
                    if !paths.contains(&p) {
                        paths.push(p);
                    }
                }
            }
        }
        paths
    }

    /// Synchronize the filesystem watcher with the currently visible directory paths.
    pub fn sync_fs_watches(&self) {
        if let Some(watcher) = &self.watcher {
            let paths = self.active_view_paths();
            watcher.set_watched_paths(&paths);
        }
    }

    /// Register a wake callback with the filesystem watcher.
    pub fn set_fs_event_listener(&mut self, wake_fn: Option<WakeFn>) {
        if let Some(watcher) = &self.watcher {
            watcher.set_wake_fn(wake_fn);
        }
    }

    /// Process any pending filesystem events and update directory state accordingly.
    /// Returns true if any directory was refreshed.
    pub fn drain_fs_events(&mut self) -> bool {
        let changed_paths = match &self.watcher {
            Some(w) => w.drain(),
            None => return false,
        };
        if changed_paths.is_empty() {
            return false;
        }

        let mut refreshed_active = false;
        let active_cwd = PathBuf::from(self.cwd());

        for path in changed_paths {
            if path == active_cwd {
                self.refresh();
                refreshed_active = true;
            } else {
                for (idx, tab) in self.tabs.iter_mut().enumerate() {
                    if idx != self.active_tab && Path::new(&tab.cwd) == path {
                        if let Ok(entries) = entry::read_dir(
                            &tab.cwd,
                            self.show_hidden,
                            self.sort_key,
                            self.sort_ascending,
                        ) {
                            tab.entries = entries;
                            tab.read_error = None;
                        }
                    }
                }
                if self.view_mode == ViewMode::Miller && !refreshed_active {
                    let in_miller = self
                        .active_tab()
                        .miller_columns
                        .iter()
                        .any(|col| Path::new(&col.path) == path);
                    if in_miller {
                        self.rebuild_miller_columns();
                    }
                }
            }
        }
        true
    }

    // ---- listing --------------------------------------------------------

    /// All entries (unfiltered). Read-only; mutations go through methods.
    pub fn entries(&self) -> &[Entry] {
        &self.active_tab().entries
    }

    /// Indices of entries passing the current text filter and file filter.
    pub fn visible(&self) -> Vec<usize> {
        let q = self.filter().trim().to_lowercase();
        let cwd = std::path::Path::new(self.cwd());
        self.entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if !q.is_empty() && !e.name.to_lowercase().contains(&q) {
                    return false;
                }
                if let Some(filter) = &self.file_filter {
                    if !e.navigable {
                        let full_path = cwd.join(&e.name);
                        if !filter.allows(&e.name, &full_path) {
                            return false;
                        }
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Set an active file filter (e.g. for portal file chooser mode).
    pub fn set_file_filter(&mut self, filter: Option<FileFilter>) {
        self.file_filter = filter;
        let tab = &mut self.tabs[self.active_tab];
        tab.selected = None;
        tab.selected_indices.clear();
        tab.anchor = None;
        self.sync_miller_selection();
    }

    /// The primary selected entry (active cursor), resolved through the filter.
    pub fn selected_entry(&self) -> Option<&Entry> {
        let visible = self.visible();
        let idx = *visible.get(self.selected()?)?;
        self.entries().get(idx)
    }

    /// Absolute path of the primary selected entry.
    pub fn selected_path(&self) -> Option<String> {
        self.selected_entry()
            .map(|e| entry::child_path(self.cwd(), &e.name))
    }

    /// Check whether a visible entry index is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.active_tab().selected_indices.contains(&index)
    }

    /// Number of currently selected items in the active tab.
    pub fn selected_count(&self) -> usize {
        self.active_tab().selected_indices.len()
    }

    /// Set of selected visible indices.
    pub fn selected_indices(&self) -> &BTreeSet<usize> {
        &self.active_tab().selected_indices
    }

    /// All selected entries, resolved through the visible filter.
    pub fn selected_entries(&self) -> Vec<&Entry> {
        let visible = self.visible();
        let entries = self.entries();
        self.active_tab()
            .selected_indices
            .iter()
            .filter_map(|&v_idx| visible.get(v_idx))
            .filter_map(|&e_idx| entries.get(e_idx))
            .collect()
    }

    /// Absolute paths of all selected entries.
    pub fn selected_paths(&self) -> Vec<String> {
        let cwd = self.cwd();
        let entries = self.selected_entries();
        if entries.is_empty() {
            if let Some(p) = self.selected_path() {
                return vec![p];
            }
        }
        entries
            .into_iter()
            .map(|e| entry::child_path(cwd, &e.name))
            .collect()
    }

    /// Move the selection cursor, optionally extending the selection range (Shift+Arrow).
    pub fn move_selection_ext(&mut self, delta: i64, extend_selection: bool) {
        let count = self.visible().len();
        if count == 0 {
            self.clear_selection();
            return;
        }
        let current = self.selected();
        let next_idx = match current {
            None if delta >= 0 => 0,
            None => count - 1,
            Some(cur) => (cur as i64 + delta).clamp(0, count as i64 - 1) as usize,
        };
        if extend_selection {
            self.select_range_visible(next_idx);
        } else {
            self.select_visible(next_idx);
        }
    }

    /// Move the selection by `delta` rows within the visible list.
    pub fn move_selection(&mut self, delta: i64) {
        self.move_selection_ext(delta, false);
    }

    /// Select a single visible index, clearing others and setting anchor.
    pub fn select_visible(&mut self, index: usize) {
        let count = self.visible().len();
        let tab = &mut self.tabs[self.active_tab];
        if index < count {
            tab.selected = Some(index);
            tab.selected_indices.clear();
            tab.selected_indices.insert(index);
            tab.anchor = Some(index);
        } else {
            tab.selected = None;
            tab.selected_indices.clear();
            tab.anchor = None;
        }
        self.sync_miller_selection();
    }

    /// Toggle selection of a visible item (Ctrl+Click).
    pub fn toggle_select_visible(&mut self, index: usize) {
        let count = self.visible().len();
        if index >= count {
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        if tab.selected_indices.contains(&index) {
            tab.selected_indices.remove(&index);
            if tab.selected == Some(index) {
                tab.selected = tab.selected_indices.iter().copied().next();
            }
        } else {
            tab.selected_indices.insert(index);
            tab.selected = Some(index);
            tab.anchor = Some(index);
        }
        self.sync_miller_selection();
    }

    /// Range selection from anchor to index (Shift+Click or Shift+Arrows).
    pub fn select_range_visible(&mut self, index: usize) {
        let count = self.visible().len();
        if count == 0 {
            return;
        }
        let target = index.min(count - 1);
        let tab = &mut self.tabs[self.active_tab];
        let anchor = tab.anchor.unwrap_or(tab.selected.unwrap_or(0)).min(count - 1);
        let start = anchor.min(target);
        let end = anchor.max(target);

        tab.selected_indices.clear();
        for i in start..=end {
            tab.selected_indices.insert(i);
        }
        tab.selected = Some(target);
        self.sync_miller_selection();
    }

    /// Select all visible items (Ctrl+A).
    pub fn select_all(&mut self) {
        let count = self.visible().len();
        let tab = &mut self.tabs[self.active_tab];
        tab.selected_indices.clear();
        for i in 0..count {
            tab.selected_indices.insert(i);
        }
        tab.selected = (count > 0).then_some(0);
        tab.anchor = (count > 0).then_some(0);
        self.sync_miller_selection();
    }

    /// Clear all selection.
    pub fn clear_selection(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        tab.selected = None;
        tab.selected_indices.clear();
        tab.anchor = None;
        self.sync_miller_selection();
    }

    /// Update the filter text. Any selection is dropped: indices address the
    /// visible list, which the filter just reshaped.
    pub fn set_filter(&mut self, filter: String) {
        let tab = &mut self.tabs[self.active_tab];
        tab.filter = filter;
        tab.selected = None;
        tab.selected_indices.clear();
        tab.anchor = None;
        self.sync_miller_selection();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode != mode {
            self.view_mode = mode;
            if mode == ViewMode::Miller {
                self.rebuild_miller_columns();
            }
            self.sync_fs_watches();
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

    /// Open the selected entry: navigate into directories, `xdg-open` files with optional activation token.
    pub fn open_selected_with_token(&mut self, token: Option<&str>) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let target = entry::child_path(self.cwd(), &entry.name);
        if entry.navigable {
            self.navigate(&target);
        } else if let Err(e) = ops::open_with_token(&target, token) {
            self.set_status(format!("Cannot open {}: {e}", entry.name));
        }
    }

    /// Open the selected entry: navigate into directories, `xdg-open` files.
    pub fn open_selected(&mut self) {
        self.open_selected_with_token(None);
    }

    /// Open `name` with optional activation token.
    pub fn open_child_with_token(&mut self, name: &str, token: Option<&str>) {
        let target = entry::child_path(self.cwd(), name);
        if entry::dir_exists(&target) {
            self.navigate(&target);
        } else if let Err(e) = ops::open_with_token(&target, token) {
            self.set_status(format!("Cannot open {name}: {e}"));
        }
    }

    /// Open `name` (a child of cwd) — the mouse path of [`Self::open_selected`].
    pub fn open_child(&mut self, name: &str) {
        self.open_child_with_token(name, None);
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
                self.undo_stack.push(crate::undo::UndoAction::CreateFolder(path));
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
                self.undo_stack.push(crate::undo::UndoAction::Rename {
                    original: from,
                    new_path: to,
                });
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

    /// Move all selected entries to the trash (atomic batch undo transaction).
    pub fn trash_selected(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        let path_refs: Vec<&Path> = paths.iter().map(|p| Path::new(p.as_str())).collect();
        match trash::trash_paths_record(&path_refs, None) {
            Ok(items) => {
                self.undo_stack.push(crate::undo::UndoAction::Trash(items));
                self.clear_selection();
                self.set_status(if count == 1 {
                    "Moved 1 item to Trash".into()
                } else {
                    format!("Moved {count} items to Trash")
                });
                self.refresh();
            }
            Err(e) => self.set_status(format!("Cannot trash items: {e}")),
        }
    }

    /// Permanently delete all selected entries bypassing Trash.
    pub fn delete_selected_permanently(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let mut deleted = 0;
        for p in &paths {
            if ops::delete_path(Path::new(p)).is_ok() {
                deleted += 1;
            }
        }
        self.clear_selection();
        self.set_status(format!("Permanently deleted {deleted} item(s)"));
        self.refresh();
    }

    /// Undo the most recent reversible file operation.
    pub fn undo(&mut self) {
        match self.undo_stack.undo() {
            Ok(msg) => {
                self.set_status(msg);
                self.refresh();
            }
            Err(err) => self.set_status(err),
        }
    }

    /// Redo the most recently undone file operation.
    pub fn redo(&mut self) {
        match self.undo_stack.redo() {
            Ok(msg) => {
                self.set_status(msg);
                self.refresh();
            }
            Err(err) => self.set_status(err),
        }
    }

    /// Copy (`cut = false`) or cut all selected entries into the clipboard.
    pub fn yank_selected(&mut self, cut: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        self.clipboard = Some(Clipboard {
            paths: path_bufs,
            cut,
        });
        self.set_status(if cut {
            format!("Cut {count} item(s) — paste with Ctrl+V")
        } else {
            format!("Copied {count} item(s) — paste with Ctrl+V")
        });
    }

    /// Paste the clipboard into cwd.
    pub fn paste(&mut self) {
        let Some(clipboard) = self.clipboard.clone() else {
            return;
        };
        let dest_dir = PathBuf::from(self.cwd());
        let mut done = 0usize;
        let mut created_dsts = Vec::new();
        for src in &clipboard.paths {
            let result = if clipboard.cut {
                ops::move_into(src, &dest_dir)
            } else {
                ops::copy_into(src, &dest_dir)
            };
            match result {
                Ok(dst) => {
                    done += 1;
                    created_dsts.push(dst);
                }
                Err(e) => self.set_status(format!(
                    "Cannot paste {}: {e}",
                    src.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        if clipboard.cut {
            self.undo_stack.push(crate::undo::UndoAction::Move {
                sources: clipboard.paths.clone(),
                destinations: created_dsts,
            });
        } else if !created_dsts.is_empty() {
            self.undo_stack.push(crate::undo::UndoAction::Copy {
                destinations: created_dsts,
            });
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

    /// Retrieve the serialized XDG clipboard payload if files are currently in the clipboard.
    pub fn clipboard_payload(&self) -> Option<String> {
        self.clipboard.as_ref().map(|c| c.to_gnome_copied_files())
    }

    /// Paste from an external system clipboard text payload.
    pub fn paste_text(&mut self, text: &str) {
        if let Some(clip) = Clipboard::from_text_payload(text) {
            self.clipboard = Some(clip);
            self.paste();
        }
    }

    /// Submit an asynchronous copy job to the background IO worker.
    pub fn copy_async(
        &mut self,
        sources: Vec<PathBuf>,
        destination: PathBuf,
    ) -> crate::worker::JobId {
        self.io_engine.submit_copy(sources, destination)
    }

    /// Submit an asynchronous move job to the background IO worker.
    pub fn move_async(
        &mut self,
        sources: Vec<PathBuf>,
        destination: PathBuf,
    ) -> crate::worker::JobId {
        self.io_engine.submit_move(sources, destination)
    }

    /// Poll for async job progress and completion, committing finished jobs to the UndoStack.
    pub fn poll_async_jobs(&mut self) -> bool {
        let mut completed_any = false;
        while let Some(completed) = self.io_engine.poll() {
            completed_any = true;
            match completed.kind {
                crate::worker::JobKind::Copy { sources: _, destination: _ } => {
                    if !completed.created_destinations.is_empty() {
                        self.undo_stack.push(crate::undo::UndoAction::Copy {
                            destinations: completed.created_destinations.clone(),
                        });
                        self.set_status(format!(
                            "Copied {} item(s)",
                            completed.created_destinations.len()
                        ));
                    }
                }
                crate::worker::JobKind::Move { sources, destination: _ } => {
                    if !completed.created_destinations.is_empty() {
                        self.undo_stack.push(crate::undo::UndoAction::Move {
                            sources,
                            destinations: completed.created_destinations.clone(),
                        });
                        self.set_status(format!(
                            "Moved {} item(s)",
                            completed.created_destinations.len()
                        ));
                    }
                }
            }
            if let Some(err) = completed.error {
                self.set_status(format!("Operation error: {err}"));
            }
        }
        if completed_any {
            self.refresh();
        }
        completed_any
    }

    // ---- bookmarks & theme ----------------------------------------------

    pub fn is_bookmarked(&self, path: &str) -> bool {
        self.bookmarks.iter().any(|b| b.path == path)
    }

    /// Add or remove a path as a user bookmark. Fixed bookmarks (home, XDG
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
            self.set_status(format!("Added bookmark {path}"));
        }
        let home = path::home_dir();
        let _ = bookmarks::save_gtk_bookmarks_file(&home, &self.bookmarks);
        self.save_config();
    }

    /// Add a path as a user bookmark (if not already bookmarked).
    pub fn add_bookmark(&mut self, path: &str) {
        let path = path::normalize(path);
        if !self.bookmarks.iter().any(|b| b.path == path) {
            self.bookmarks.push(Bookmark::for_path(&path));
            self.set_status(format!("Added bookmark {path}"));
            let home = path::home_dir();
            let _ = bookmarks::save_gtk_bookmarks_file(&home, &self.bookmarks);
            self.save_config();
        }
    }

    /// Move a dropped item into a destination directory.
    pub fn drop_into(&mut self, src_path: &str, dst_dir: &str) {
        let src = std::path::PathBuf::from(src_path);
        let dst = std::path::PathBuf::from(dst_dir);
        if src == dst || src.parent() == Some(&dst) {
            return;
        }
        match ops::move_into(&src, &dst) {
            Ok(_) => {
                let name = src.file_name().unwrap_or_default().to_string_lossy();
                let dst_name = dst.file_name().unwrap_or_default().to_string_lossy();
                self.set_status(format!("Moved {name} into {dst_name}"));
                self.refresh();
            }
            Err(e) => {
                let name = src.file_name().unwrap_or_default().to_string_lossy();
                self.set_status(format!("Cannot move {name}: {e}"));
            }
        }
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
            "arca-state-test-{}-{}",
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

        let config_file = dir.join("cfg").join("arca.conf");
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
        let saved = std::fs::read_to_string(dir.join("cfg").join("arca.conf")).unwrap();
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
    fn file_filter_keeps_directories_and_filters_files() {
        let (dir, mut s) = fixture();
        std::fs::write(dir.join("image.png"), b"png").unwrap();
        s.refresh();
        assert_eq!(s.visible().len(), 4); // sub, a.txt, b.txt, image.png

        let filter = crate::chooser::FileFilter::new(
            "PNG Images",
            vec![crate::chooser::FilterRule {
                kind: crate::chooser::FilterRuleKind::Glob,
                value: "*.png".into(),
            }],
        );
        s.set_file_filter(Some(filter));
        let visible_names: Vec<_> = s
            .visible()
            .iter()
            .map(|&idx| s.entries()[idx].name.clone())
            .collect();
        assert_eq!(visible_names, vec!["sub", "image.png"]);

        s.set_file_filter(None);
        assert_eq!(s.visible().len(), 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hidden_toggle_persists() {
        let (dir, mut s) = fixture();
        assert_eq!(s.entries().len(), 3);
        s.toggle_hidden();
        assert_eq!(s.entries().len(), 4);
        // Config was written and round-trips.
        let cfg = config::load(dir.join("cfg/arca.conf").to_str().unwrap());
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
        crate::trash::trash_paths(&[&victim], Some(&trash_root)).unwrap();
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
    fn clipboard_xdg_payload_roundtrip() {
        let (dir, _) = fixture();
        let file1 = dir.join("a.txt");
        let file2 = dir.join("b.txt");

        let clip = Clipboard::new(vec![file1.clone(), file2.clone()], false);
        let gnome_payload = clip.to_gnome_copied_files();
        assert!(gnome_payload.starts_with("copy\n"));

        let parsed = Clipboard::from_text_payload(&gnome_payload).expect("should parse");
        assert!(!parsed.cut);
        assert_eq!(parsed.paths, vec![file1.clone(), file2.clone()]);

        let cut_clip = Clipboard::new(vec![file1.clone()], true);
        let cut_payload = cut_clip.to_gnome_copied_files();
        assert!(cut_payload.starts_with("cut\n"));
        let parsed_cut = Clipboard::from_text_payload(&cut_payload).expect("should parse");
        assert!(parsed_cut.cut);
        assert_eq!(parsed_cut.paths, vec![file1.clone()]);

        let uri_list = clip.to_uri_list();
        assert!(uri_list.contains("\r\n"));
        let parsed_uri = Clipboard::from_text_payload(&uri_list).expect("should parse uri-list");
        assert_eq!(parsed_uri.paths, vec![file1, file2]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bookmark_toggle_roundtrip() {
        let (dir, mut s) = fixture();
        let target = dir.join("sub").to_str().unwrap().to_string();
        assert!(!s.is_bookmarked(&target));
        s.toggle_bookmark(&target);
        assert!(s.is_bookmarked(&target));

        let cfg = config::load(dir.join("cfg/arca.conf").to_str().unwrap());
        assert!(cfg.bookmarks.contains(&target));

        s.toggle_bookmark(&target);
        assert!(!s.is_bookmarked(&target));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn add_bookmark_and_drop_into() {
        let (dir, mut s) = fixture();
        let sub = dir.join("sub").to_str().unwrap().to_string();
        let file_a = dir.join("a.txt").to_str().unwrap().to_string();

        assert!(!s.is_bookmarked(&sub));
        s.add_bookmark(&sub);
        assert!(s.is_bookmarked(&sub));
        s.add_bookmark(&sub);
        assert!(s.is_bookmarked(&sub));

        s.drop_into(&file_a, &sub);
        assert!(!dir.join("a.txt").exists());
        assert!(dir.join("sub").join("a.txt").exists());

        s.toggle_bookmark(&sub); // clean up gtk bookmark

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
        let cfg = config::load(dir.join("cfg/arca.conf").to_str().unwrap());
        assert_eq!(cfg.view_mode, ViewMode::Miller);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fs_watcher_updates_directory_listing_on_external_mutation() {
        let (dir, mut s) = fixture();
        assert_eq!(s.entries().len(), 3); // sub, a.txt, b.txt

        // Allow watcher thread to register watch
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 1. External download / creation
        let new_file = dir.join("downloaded.zip");
        std::fs::write(&new_file, b"content").unwrap();

        let start = std::time::Instant::now();
        let mut updated = false;
        while start.elapsed() < std::time::Duration::from_secs(2) {
            if s.drain_fs_events() {
                updated = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(updated, "expected drain_fs_events to report refresh");
        assert!(
            s.entries().iter().any(|e| e.name == "downloaded.zip"),
            "new file should appear in active entries"
        );

        // 2. External deletion
        std::fs::remove_file(&new_file).unwrap();
        let start = std::time::Instant::now();
        let mut removed_updated = false;
        while start.elapsed() < std::time::Duration::from_secs(2) {
            if s.drain_fs_events() {
                removed_updated = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(removed_updated, "expected drain_fs_events to report refresh after deletion");
        assert!(
            !s.entries().iter().any(|e| e.name == "downloaded.zip"),
            "deleted file should disappear from active entries"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_and_redo_file_operations() {
        let (dir, mut s) = fixture();

        // 1. Test create folder + undo/redo
        let folder_name = s.create_folder().expect("folder created");
        assert!(dir.join(&folder_name).exists());
        assert!(s.undo_stack.can_undo());

        s.undo();
        assert!(!dir.join(&folder_name).exists());
        assert!(s.undo_stack.can_redo());

        s.redo();
        assert!(dir.join(&folder_name).exists());

        // 2. Test rename + undo/redo
        s.select_by_name(&folder_name);
        assert!(s.rename(&folder_name, "renamed_folder"));
        assert!(dir.join("renamed_folder").exists());
        assert!(!dir.join(&folder_name).exists());

        s.undo();
        assert!(dir.join(&folder_name).exists());
        assert!(!dir.join("renamed_folder").exists());

        s.redo();
        assert!(dir.join("renamed_folder").exists());
        assert!(!dir.join(&folder_name).exists());

        // 3. Test trash + undo/redo
        s.select_by_name("renamed_folder");
        s.trash_selected();
        assert!(!dir.join("renamed_folder").exists());

        s.undo();
        assert!(dir.join("renamed_folder").exists());

        // 4. Test permanent delete
        s.select_by_name("renamed_folder");
        s.delete_selected_permanently();
        assert!(!dir.join("renamed_folder").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multi_selection_and_batch_operations() {
        let (dir, mut s) = fixture();
        assert_eq!(s.entries().len(), 3);

        // 1. Initial single selection
        assert_eq!(s.selected_count(), 0);
        s.select_visible(0);
        assert_eq!(s.selected_count(), 1);
        assert!(s.is_selected(0));
        assert!(!s.is_selected(1));

        // 2. Range selection 0..=2 (selects all 3)
        s.select_range_visible(2);
        assert_eq!(s.selected_count(), 3);
        assert!(s.is_selected(0));
        assert!(s.is_selected(1));
        assert!(s.is_selected(2));
        assert_eq!(s.selected_paths().len(), 3);

        // 3. Toggle selection (Ctrl+Click removes index 1)
        s.toggle_select_visible(1);
        assert_eq!(s.selected_count(), 2);
        assert!(s.is_selected(0));
        assert!(!s.is_selected(1));
        assert!(s.is_selected(2));

        // 4. Batch Yank (Copy)
        s.yank_selected(false);
        assert_eq!(s.clipboard.as_ref().unwrap().paths.len(), 2);

        // 5. Select All (Ctrl+A)
        s.select_all();
        assert_eq!(s.selected_count(), 3);

        // 6. Batch Trash all items
        s.trash_selected();
        assert_eq!(s.entries().len(), 0);
        assert_eq!(s.selected_count(), 0);

        // 7. Undo restores all 3 items at once!
        s.undo();
        assert_eq!(s.entries().len(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
