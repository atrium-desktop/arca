//! `AppState`: the whole non-GUI state of the file manager.
//!
//! The UI crate owns one of these, renders it every frame, and calls its
//! methods in response to input. The engine exposes a synchronous API for
//! tests, embedders, and the CLI/chooser paths, and an asynchronous
//! navigation API (`begin_navigate` / `poll_navigation`) for the interactive
//! window: a directory read must never block the frame that received the
//! click.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bookmarks::{self, Bookmark};
use crate::chooser::FileFilter;
use crate::config::{self, Config, ThemeMode, ViewMode};
use crate::entry::{self, Entry, SortKey};
use crate::history::History;
use crate::ops;
use crate::path;
use crate::trash;
use crate::watch::{FsWatcher, WakeFn};

/// The engine's clipboard is the XDG transfer payload itself: one type for
/// `Ctrl+C`/`Ctrl+X` staging and for drop payloads, so the two paths can
/// never drift apart again.
pub use arca_xdg::clipboard::{FileClipboard as Clipboard, TransferAction};

/// A directory read in flight. Each request owns its receiver; a newer request
/// replaces the previous one and the stale worker's send fails harmlessly.
struct PendingNavigation {
    tab_id: u64,
    target: String,
    history: HistoryUpdate,
    started: std::time::Instant,
    result: std::sync::mpsc::Receiver<std::io::Result<Vec<Entry>>>,
}

/// How a navigation moves its tab's history — applied only once the read
/// succeeded, so a failed or superseded request leaves the cursor alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryUpdate {
    /// A new location: push it.
    Push,
    /// Back/forward: move the cursor by this many steps.
    Step(isize),
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

    /// Lazily built `.trashinfo` index for the current Trash listing.
    ///
    /// Enumerating the trash root costs ~2 ms for 500 items, so looking a row
    /// up by name per frame would blow the frame budget. Built once per
    /// listing and invalidated whenever the directory changes.
    trash_index: Option<std::collections::HashMap<String, trash::TrashItem>>,

    /// Optional explicit Trash root. `None` uses the FreeDesktop home/per-mount
    /// location; embedders and tests pass a scratch root so they never move the
    /// user's files into the real Trash.
    trash_root: Option<PathBuf>,

    /// Latest asynchronous navigation request, if any.
    pending_navigation: Option<PendingNavigation>,
    /// Monotonic request id. A worker that lost the race skips its read
    /// instead of competing for the disk after the user has moved on.
    nav_generation: Arc<AtomicU64>,
    /// Listing for the Miller preview (right) column; see [`Self::request_miller_preview`].
    miller_preview: crate::miller_preview::PreviewLoader,
}

/// One independent browsing session. Navigation, selection and filtering are
/// deliberately tab-local; preferences, bookmarks and the clipboard remain
/// application-wide.
///
/// # Selection identity
///
/// Selection is modelled by [`Entry`] *name*, never by visible index. The
/// visible list is rebuilt by every directory read, sort toggle, hidden-file
/// toggle and filter edit, so an index kept across a rebuild would silently
/// address a different file — and the next `Delete` would then trash the
/// wrong entry. The `*_indices` fields are a cache derived from the names by
/// `TabState::resync_selection` and are never written from outside it.
///
/// Selection is always a subset of the visible set: names hidden by the
/// filter are dropped, because every file operation resolves through
/// `visible()` and acting on an invisible entry is not what the user sees.
#[derive(Clone, Debug)]
pub struct TabState {
    pub id: u64,
    pub cwd: String,
    entries: Vec<Entry>,
    pub read_error: Option<String>,
    history: History,

    /// Authoritative multi-selection, by entry name.
    selected_names: BTreeSet<String>,
    /// Authoritative primary cursor (the row that owns focus), by entry name.
    cursor_name: Option<String>,
    /// Authoritative range-selection anchor, by entry name.
    anchor_name: Option<String>,

    /// Derived: visible index of `cursor_name`.
    selected: Option<usize>,
    /// Derived: visible indices of `selected_names`.
    selected_indices: BTreeSet<usize>,
    /// Derived: visible index of `anchor_name`.
    anchor: Option<usize>,

    pub filter: String,
    miller_columns: Vec<MillerColumn>,
}

impl TabState {
    /// Recompute the derived index caches from the authoritative names.
    ///
    /// Must run after any change to `entries`, `filter`, `file_filter`, the
    /// sort order, or the selection itself. Names that are no longer visible
    /// are dropped, keeping `selected_names` a subset of the visible set.
    /// Selection is always a subset of the visible set, and the cursor is
    /// always a member of the selection: names hidden by the filter are
    /// dropped, because every file operation resolves through `visible()`
    /// and acting on an invisible entry is not what the user sees.
    fn resync_selection(&mut self, visible: &[usize]) {
        // Drop names that left the visible set (deleted, filtered out).
        let visible_names: BTreeSet<&str> = visible
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| e.name.as_str())
            .collect();
        self.selected_names
            .retain(|n| visible_names.contains(n.as_str()));
        if let Some(cursor) = &self.cursor_name
            && !visible_names.contains(cursor.as_str())
        {
            self.cursor_name = None;
        }
        if let Some(anchor) = &self.anchor_name
            && !visible_names.contains(anchor.as_str())
        {
            self.anchor_name = None;
        }
        // Keep the cursor/selection invariant: a cursor whose row is not
        // selected would make `selected()` and `selected_entries()` disagree.
        if let Some(cursor) = self.cursor_name.clone() {
            self.selected_names.insert(cursor);
        }

        // Re-derive indices in visible order.
        let mut selected_indices = BTreeSet::new();
        let mut selected = None;
        let mut anchor = None;
        for (visible_idx, &entry_idx) in visible.iter().enumerate() {
            let Some(entry) = self.entries.get(entry_idx) else {
                continue;
            };
            if self.selected_names.contains(&entry.name) {
                selected_indices.insert(visible_idx);
            }
            if self.cursor_name.as_deref() == Some(entry.name.as_str()) {
                selected = Some(visible_idx);
            }
            if self.anchor_name.as_deref() == Some(entry.name.as_str()) {
                anchor = Some(visible_idx);
            }
        }
        self.selected = selected;
        self.selected_indices = selected_indices;
        self.anchor = anchor;
    }

    /// Assert the derived index caches agree with the authoritative names.
    ///
    /// The invariant: every `selected_indices` entry resolves to a member of
    /// `selected_names`, `selected` resolves to `cursor_name`, and the cursor
    /// is itself selected. Any violation means an entries write skipped
    /// `resync_selection`.
    #[cfg(test)]
    fn debug_assert_selection_coherent(&self, visible: &[usize]) {
        let name_at = |visible_index: usize| {
            visible
                .get(visible_index)
                .and_then(|&entry_index| self.entries.get(entry_index))
                .map(|entry| entry.name.as_str())
        };

        for visible_index in &self.selected_indices {
            let name = name_at(*visible_index)
                .unwrap_or_else(|| panic!("selected index {visible_index} is out of range"));
            assert!(
                self.selected_names.contains(name),
                "selected index {visible_index} resolves to {name:?}, which is not in selected_names"
            );
        }
        assert_eq!(
            self.selected_indices.len(),
            self.selected_names.len(),
            "every selected name must have exactly one visible index"
        );
        if let Some(cursor_name) = &self.cursor_name {
            let resolved = self.selected.and_then(name_at);
            assert_eq!(
                resolved,
                Some(cursor_name.as_str()),
                "cursor index must resolve to cursor_name"
            );
            assert!(
                self.selected_names.contains(cursor_name),
                "the cursor must always be a member of the selection"
            );
        }
    }

    /// Drop every selection name (derived caches are refreshed by
    /// `TabState::resync_selection`).
    fn clear_selection_names(&mut self) {
        self.selected_names.clear();
        self.cursor_name = None;
        self.anchor_name = None;
    }
}

/// A directory presented in Miller view. The model holds at most two
/// columns: the parent (when the active directory has one) and, always last,
/// the active directory. The UI centres the active column and fills the
/// right-hand slot from [`AppState::miller_preview`].
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
            selected_names: BTreeSet::new(),
            cursor_name: None,
            anchor_name: None,
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
            trash_index: None,
            trash_root: None,
            pending_navigation: None,
            nav_generation: Arc::new(AtomicU64::new(0)),
            miller_preview: Default::default(),
        };
        state.rebuild_bookmarks(&cfg.bookmarks);
        state.navigate(&path::home_dir());
        state.sync_fs_watches();
        state
    }

    /// Construct with explicit inputs (tests, embedding).
    pub fn with_config(config_file: String, cfg: Config, start_dir: &str) -> AppState {
        Self::with_config_and_trash_root(config_file, cfg, start_dir, None)
    }

    /// Construct with an explicit Trash root.
    ///
    /// Embedders and tests use this to keep destructive operations inside a
    /// scratch tree instead of the user's real home/per-mount Trash. `None`
    /// preserves the FreeDesktop default.
    pub fn with_config_and_trash_root(
        config_file: String,
        cfg: Config,
        start_dir: &str,
        trash_root: Option<PathBuf>,
    ) -> AppState {
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
            trash_index: None,
            trash_root,
            pending_navigation: None,
            nav_generation: Arc::new(AtomicU64::new(0)),
            miller_preview: Default::default(),
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
            if !self.bookmarks.iter().any(|b| b.path == bookmark.path)
                && entry::dir_exists(&bookmark.path)
            {
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

    /// Reorder tabs by moving the tab at `from` to the slot `to`, expressed
    /// in post-removal coordinates (remove `from` first, then insert at
    /// `to`) — the same convention the optics-ui tab strip reports, so the
    /// drag action applies without index translation. The active tab follows its
    /// document, never its old position.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        // Track where the active tab landed after removal/insertion.
        if self.active_tab == from {
            self.active_tab = to;
        } else if from < self.active_tab && to >= self.active_tab {
            self.active_tab -= 1;
        } else if to <= self.active_tab && from > self.active_tab {
            self.active_tab += 1;
        }
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

    /// Navigate to `path` (tilde/relative segments and `file://` URIs resolved).
    /// Records history on success; reports and stays put on failure.
    pub fn navigate(&mut self, raw: &str) {
        let decoded_uri = raw
            .starts_with("file://")
            .then(|| crate::xdg::decode_file_uri(raw))
            .flatten()
            .map(|p| p.to_string_lossy().into_owned());
        let target = decoded_uri.as_deref().unwrap_or(raw);
        let mut resolved =
            if target.starts_with('/') || target.starts_with('~') || self.cwd().is_empty() {
                path::normalize(target)
            } else {
                path::resolve(self.cwd(), target)
            };
        if !entry::dir_exists(&resolved) {
            // A `file://` URI (e.g. `OpenURI` for `x-scheme-handler/file`) may
            // point to a regular file; reveal its parent directory instead of
            // failing with "Not a directory".
            if decoded_uri.is_some()
                && std::path::Path::new(&resolved).is_file()
                && let Some(parent) = std::path::Path::new(&resolved)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .filter(|p| !p.is_empty() && entry::dir_exists(p))
            {
                resolved = parent;
            }
        }
        if !entry::dir_exists(&resolved) {
            self.set_status(format!("Not a directory: {resolved}"));
            return;
        }
        self.tabs[self.active_tab].history.push(&resolved);
        self.set_dir(&resolved);
    }

    fn set_dir(&mut self, path: &str) {
        // A synchronous navigation supersedes any in-flight asynchronous one;
        // bumping the generation also tells the worker to skip its read.
        self.cancel_navigation();
        let listing = entry::read_dir(path, self.show_hidden, self.sort_key, self.sort_ascending);
        self.apply_listing(self.active_tab, path, listing);
    }

    /// Apply a directory listing to one tab, whether it came from a
    /// synchronous read or an asynchronous worker.
    fn apply_listing(
        &mut self,
        tab_index: usize,
        path: &str,
        listing: std::io::Result<Vec<Entry>>,
    ) {
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        tab.cwd = path.to_string();
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
        // Entering a directory is a new listing: nothing is selected. A
        // *re-read* of the same directory goes through `refresh` instead,
        // which preserves selection by name.
        tab.clear_selection_names();
        let visible = self.visible_in_tab(tab_index);
        self.tabs[tab_index].resync_selection(&visible);

        if tab_index == self.active_tab {
            self.rebuild_miller_columns();
            self.rebuild_trash_index();
        }
        if self
            .status
            .as_deref()
            .is_some_and(|status| status.starts_with("Loading "))
        {
            self.status = None;
        }
        self.sync_fs_watches();
    }

    // ---- asynchronous navigation ---------------------------------------

    /// Start reading a directory off the UI thread.
    ///
    /// Returns `true` once the request is accepted. The caller keeps painting
    /// the previous listing and calls [`Self::poll_navigation`] each frame;
    /// [`Self::is_navigating`] tells the host to request another frame. A
    /// newer request replaces the older one, so only the last navigation can
    /// publish a listing.
    pub fn begin_navigate(&mut self, raw: &str) -> bool {
        let resolved = if raw.starts_with('/') || raw.starts_with('~') || self.cwd().is_empty() {
            path::normalize(raw)
        } else {
            path::resolve(self.cwd(), raw)
        };
        // The directory probe and read happen in the worker: even a `stat`
        // can block on an unresponsive mount. History is recorded only after
        // the read succeeds.
        self.begin_set_dir(resolved, HistoryUpdate::Push)
    }

    /// Asynchronous [`Self::go_back`].
    pub fn begin_go_back(&mut self) -> bool {
        self.begin_history_step(-1)
    }

    /// Asynchronous [`Self::go_forward`].
    pub fn begin_go_forward(&mut self) -> bool {
        self.begin_history_step(1)
    }

    /// Peek `direction` steps from the cursor — counted from any step still
    /// in flight for this tab, so rapid Back clicks walk further back instead
    /// of re-requesting the same entry — without moving the cursor yet.
    fn begin_history_step(&mut self, direction: isize) -> bool {
        let tab_id = self.tabs[self.active_tab].id;
        let in_flight = match &self.pending_navigation {
            Some(pending) if pending.tab_id == tab_id => match pending.history {
                HistoryUpdate::Step(steps) => steps,
                HistoryUpdate::Push => 0,
            },
            _ => 0,
        };
        let steps = in_flight + direction;
        let Some(path) = self.tabs[self.active_tab]
            .history
            .peek(steps)
            .map(str::to_string)
        else {
            return false;
        };
        self.begin_set_dir(path, HistoryUpdate::Step(steps))
    }

    fn apply_history(&mut self, tab_index: usize, update: HistoryUpdate, target: &str) {
        let history = &mut self.tabs[tab_index].history;
        match update {
            HistoryUpdate::Push => history.push(target),
            HistoryUpdate::Step(steps) => {
                history.step(steps);
            }
        }
    }

    /// Asynchronous [`Self::go_up`].
    pub fn begin_go_up(&mut self) -> bool {
        let parent = path::parent(self.cwd());
        if parent == self.cwd() {
            return false;
        }
        self.begin_navigate(&parent)
    }

    fn cancel_navigation(&mut self) {
        self.nav_generation.fetch_add(1, Ordering::Relaxed);
        self.pending_navigation = None;
    }

    fn begin_set_dir(&mut self, path: String, history: HistoryUpdate) -> bool {
        let tab_index = self.active_tab;
        crate::telemetry::counter("nav.started", 1);
        if self.pending_navigation.is_some() {
            crate::telemetry::counter("nav.superseded", 1);
        }
        log::debug!(target: "arca_engine::nav", "navigate {path} ({history:?})");
        let tab_id = self.tabs[tab_index].id;
        let generation = self.nav_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let generation_handle = Arc::clone(&self.nav_generation);
        let (tx, rx) = std::sync::mpsc::channel();
        let thread_path = path.clone();
        let show_hidden = self.show_hidden;
        let sort_key = self.sort_key;
        let sort_ascending = self.sort_ascending;
        let spawned = std::thread::Builder::new()
            .name("arca-navigation".into())
            .spawn(move || {
                if generation_handle.load(Ordering::Relaxed) != generation {
                    return;
                }
                let listing = entry::read_dir(&thread_path, show_hidden, sort_key, sort_ascending);
                if generation_handle.load(Ordering::Relaxed) == generation {
                    let _ = tx.send(listing);
                }
            });

        if spawned.is_err() {
            self.pending_navigation = None;
            let listing = entry::read_dir(&path, show_hidden, sort_key, sort_ascending);
            if let Ok(entries) = listing {
                self.apply_history(tab_index, history, &path);
                self.apply_listing(tab_index, &path, Ok(entries));
            } else {
                self.set_status(format!("Cannot open {path}"));
            }
            return true;
        }

        self.pending_navigation = Some(PendingNavigation {
            tab_id,
            target: path.clone(),
            history,
            started: std::time::Instant::now(),
            result: rx,
        });
        self.set_status(format!("Loading {path}…"));
        true
    }

    /// Whether a directory read is in flight.
    #[must_use]
    pub fn is_navigating(&self) -> bool {
        self.pending_navigation.is_some()
    }

    /// Poll the in-flight navigation without blocking. Returns true when the
    /// listing changed (or the worker failed), so the host can invalidate.
    pub fn poll_navigation(&mut self) -> bool {
        let Some(pending) = self.pending_navigation.take() else {
            return false;
        };
        let outcome = pending.result.try_recv();
        if !matches!(outcome, Err(std::sync::mpsc::TryRecvError::Empty)) {
            let elapsed = pending.started.elapsed();
            crate::telemetry::observe("nav.latency", elapsed);
            let ok = matches!(outcome, Ok(Ok(_)));
            crate::telemetry::counter(if ok { "nav.completed" } else { "nav.failed" }, 1);
            log::debug!(
                target: "arca_engine::nav",
                "navigation to {} {} in {} ms",
                pending.target,
                if ok { "published" } else { "failed" },
                elapsed.as_millis()
            );
        }
        match outcome {
            Ok(Ok(entries)) => {
                let Some(index) = self.tabs.iter().position(|tab| tab.id == pending.tab_id) else {
                    return false;
                };
                self.apply_history(index, pending.history, &pending.target);
                self.apply_listing(index, &pending.target, Ok(entries));
                true
            }
            Ok(Err(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                self.set_status(format!("Not a directory: {}", pending.target));
                true
            }
            Ok(Err(error)) => {
                self.set_status(format!("Cannot open {}: {error}", pending.target));
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pending_navigation = Some(pending);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.set_status("Navigation failed".into());
                true
            }
        }
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

    /// Re-read the current directory in place.
    ///
    /// Selection is carried across by name (not by index), so an external
    /// change or `Ctrl+R` cannot shift the highlight onto a different file.
    /// Entries that genuinely disappeared are dropped from the selection.
    pub fn refresh(&mut self) {
        let _span = crate::telemetry::span("state.refresh");
        let cwd = self.cwd().to_string();
        let listing = entry::read_dir(&cwd, self.show_hidden, self.sort_key, self.sort_ascending);
        let tab = &mut self.tabs[self.active_tab];
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
        let visible = self.visible();
        let tab = &mut self.tabs[self.active_tab];
        tab.resync_selection(&visible);
        self.rebuild_miller_columns();
        self.sync_miller_selection();
        self.rebuild_trash_index();
    }

    /// Toggle dotfile visibility across every tab.
    ///
    /// Refreshes each tab in place through an index loop rather than by
    /// temporarily reassigning `active_tab`, so an early return can never
    /// leave the active-tab pointer on the wrong tab.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        for index in 0..self.tabs.len() {
            let cwd = self.tabs[index].cwd.clone();
            let listing =
                entry::read_dir(&cwd, self.show_hidden, self.sort_key, self.sort_ascending);
            let tab = &mut self.tabs[index];
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
        }
        for index in 0..self.tabs.len() {
            let visible = self.visible_in_tab(index);
            self.tabs[index].resync_selection(&visible);
        }
        self.rebuild_miller_columns();
        self.sync_fs_watches();
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
        crate::telemetry::counter("fs.watch.events", changed_paths.len() as u64);
        log::debug!(target: "arca_engine::watch", "changed: {changed_paths:?}");

        let mut refreshed_active = false;
        let active_cwd = PathBuf::from(self.cwd());

        for path in changed_paths {
            if path == active_cwd {
                self.refresh();
                refreshed_active = true;
            } else {
                for index in 0..self.tabs.len() {
                    if Path::new(&self.tabs[index].cwd) == path {
                        self.reload_tab(index);
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

    /// Re-read one tab's directory in place, refreshing its derived
    /// selection caches.
    ///
    /// `entries` is exactly what the index caches address, so a re-read that
    /// skips `TabState::resync_selection` leaves `selected_indices`
    /// pointing at whatever entry now occupies those indices — and the next
    /// `Delete` in that tab would act on the wrong file. Every entries write
    /// must therefore go through here or through an equivalent resync.
    fn reload_tab(&mut self, index: usize) {
        let Some(cwd) = self.tabs.get(index).map(|tab| tab.cwd.clone()) else {
            return;
        };
        let listing = entry::read_dir(&cwd, self.show_hidden, self.sort_key, self.sort_ascending);
        let tab = &mut self.tabs[index];
        match listing {
            Ok(entries) => {
                tab.entries = entries;
                tab.read_error = None;
            }
            Err(error) => {
                tab.entries.clear();
                tab.read_error = Some(error.to_string());
            }
        }
        let visible = self.visible_in_tab(index);
        self.tabs[index].resync_selection(&visible);
        // The index describes the *active* listing only; rebuilding it for a
        // background tab would enumerate the trash root for nothing.
        if index == self.active_tab {
            self.rebuild_trash_index();
        }
    }

    /// (Re)build the trash metadata index for the active listing.
    ///
    /// Only a Trash `files/` listing needs it; anywhere else the map is
    /// dropped so no stale records survive navigation.
    fn rebuild_trash_index(&mut self) {
        let _span = crate::telemetry::span("state.trash_index");
        self.trash_index = if self.is_showing_trash() {
            Some(
                self.trash_items()
                    .into_iter()
                    .map(|item| (item.id.clone(), item))
                    .collect(),
            )
        } else {
            None
        };
    }

    // ---- listing --------------------------------------------------------

    /// All entries (unfiltered). Read-only; mutations go through methods.
    pub fn entries(&self) -> &[Entry] {
        &self.active_tab().entries
    }

    /// Indices of entries passing the current text filter and file filter.
    ///
    /// Recomputed per call: the filter combines a substring match with
    /// per-file MIME classification, which requires filesystem access and
    /// cannot be memoized without invalidation bugs.
    pub fn visible(&self) -> Vec<usize> {
        self.visible_in_tab(self.active_tab)
    }

    /// [`Self::visible`] for an arbitrary tab index.
    ///
    /// Pure in-memory work: the substring test uses the entry name and the
    /// file filter uses the MIME type the directory read already resolved.
    /// (Deriving MIME from a path here cost a `open`+`read` per entry per
    /// call — ~12 ms for 2400 files, three times per frame.)
    fn visible_in_tab(&self, tab_index: usize) -> Vec<usize> {
        let Some(tab) = self.tabs.get(tab_index) else {
            return Vec::new();
        };
        let query = tab.filter.trim().to_lowercase();
        tab.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if !query.is_empty() && !entry.name.to_lowercase().contains(&query) {
                    return false;
                }
                if let Some(filter) = &self.file_filter {
                    // Directories stay navigable regardless of the filter.
                    if !entry.navigable && !filter.allows_mime(&entry.name, &entry.mime_type) {
                        return false;
                    }
                }
                true
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Set an active file filter (e.g. for portal file chooser mode).
    pub fn set_file_filter(&mut self, filter: Option<FileFilter>) {
        self.file_filter = filter;
        let visible = self.visible();
        let tab = &mut self.tabs[self.active_tab];
        tab.resync_selection(&visible);
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

    /// Absolute paths of all selected entries, in visible order.
    ///
    /// This is the single source every destructive operation resolves
    /// through, so it must agree with what the listing highlights. The
    /// cursor is not a fallback here: an unselected cursor must not silently
    /// become the operand of `Delete`.
    pub fn selected_paths(&self) -> Vec<String> {
        let cwd = self.cwd();
        self.selected_entries()
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
        let visible = self.visible();
        let name = visible
            .get(index)
            .and_then(|&entry_idx| self.entries().get(entry_idx))
            .map(|entry| entry.name.clone());
        let tab = &mut self.tabs[self.active_tab];
        match name {
            Some(name) => {
                tab.selected_names = BTreeSet::from([name.clone()]);
                tab.cursor_name = Some(name.clone());
                tab.anchor_name = Some(name);
            }
            None => tab.clear_selection_names(),
        }
        tab.resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Toggle selection of a visible item (Ctrl+Click).
    pub fn toggle_select_visible(&mut self, index: usize) {
        let visible = self.visible();
        let Some(name) = visible
            .get(index)
            .and_then(|&entry_idx| self.entries().get(entry_idx))
            .map(|entry| entry.name.clone())
        else {
            return;
        };
        let tab = &mut self.tabs[self.active_tab];
        if tab.selected_names.contains(&name) {
            tab.selected_names.remove(&name);
            if tab.cursor_name.as_deref() == Some(name.as_str()) {
                // Cursor falls back to the first remaining selected name;
                // order is re-derived from the visible list below.
                tab.cursor_name = tab.selected_names.iter().next().cloned();
            }
        } else {
            tab.selected_names.insert(name.clone());
            tab.cursor_name = Some(name.clone());
            tab.anchor_name = Some(name);
        }
        tab.resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Range selection from anchor to index (Shift+Click or Shift+Arrows).
    pub fn select_range_visible(&mut self, index: usize) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let target = index.min(visible.len() - 1);
        // Snapshot names up front: the closure runs while `tab` is borrowed
        // mutably below.
        let names_at: Vec<Option<String>> = visible
            .iter()
            .map(|&entry_idx| {
                self.tabs[self.active_tab]
                    .entries
                    .get(entry_idx)
                    .map(|entry| entry.name.clone())
            })
            .collect();
        let name_at = |v: usize| names_at.get(v).cloned().flatten();

        let tab = &mut self.tabs[self.active_tab];
        // Resolve the anchor by name first: it survives re-sorts, and falls
        // back to the cursor (or the first row) when there is none.
        let anchor_visible = tab
            .anchor_name
            .as_ref()
            .and_then(|anchor| {
                names_at
                    .iter()
                    .position(|name| name.as_deref() == Some(anchor.as_str()))
            })
            .or(tab.selected)
            .unwrap_or(0)
            .min(visible.len() - 1);

        let start = anchor_visible.min(target);
        let end = anchor_visible.max(target);
        let names: BTreeSet<String> = (start..=end).filter_map(name_at).collect();
        // Anchor keeps its original row so the range can be re-extended.
        tab.anchor_name = name_at(anchor_visible);
        tab.cursor_name = name_at(target);
        tab.selected_names = names;
        tab.resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Select all visible items (Ctrl+A).
    pub fn select_all(&mut self) {
        let visible = self.visible();
        let names: BTreeSet<String> = visible
            .iter()
            .filter_map(|&entry_idx| self.tabs[self.active_tab].entries.get(entry_idx))
            .map(|entry| entry.name.clone())
            .collect();
        let tab = &mut self.tabs[self.active_tab];
        // Select all keeps the existing cursor when it is still visible;
        // otherwise the first row takes focus.
        let cursor = tab
            .cursor_name
            .clone()
            .filter(|name| names.contains(name))
            .or_else(|| names.iter().next().cloned());
        tab.selected_names = names;
        tab.cursor_name = cursor.clone();
        tab.anchor_name = cursor;
        tab.resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Clear all selection.
    pub fn clear_selection(&mut self) {
        let visible = self.visible();
        let tab = &mut self.tabs[self.active_tab];
        tab.clear_selection_names();
        tab.resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Update the filter text. Selection outside the filtered set is dropped
    /// by `resync_selection`, so file operations only ever reach rows the
    /// user can actually see.
    pub fn set_filter(&mut self, filter: String) {
        self.tabs[self.active_tab].filter = filter;
        let visible = self.visible();
        self.tabs[self.active_tab].resync_selection(&visible);
        self.sync_miller_selection();
    }

    /// Change the view mode as a *user action*: applies it and persists it as
    /// the new default for future launches.
    ///
    /// A command-line `--grid`/`--list`/`--miller` is a one-shot override, not
    /// a preference, so it must use [`Self::override_view_mode`] instead —
    /// persisting it would silently rewrite the user's configuration every
    /// time the binary is invoked with a flag.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.apply_view_mode(mode) {
            self.save_config();
        }
    }

    /// Change the view mode for this session only.
    pub fn override_view_mode(&mut self, mode: ViewMode) {
        self.apply_view_mode(mode);
    }

    /// Apply `mode` to the live state. Returns whether anything changed.
    fn apply_view_mode(&mut self, mode: ViewMode) -> bool {
        if self.view_mode == mode {
            return false;
        }
        self.view_mode = mode;
        if mode == ViewMode::Miller {
            self.rebuild_miller_columns();
        } else {
            // Drop the stale model rather than leaving it readable: it would
            // be silently out of date after any navigation in this mode.
            let tab = &mut self.tabs[self.active_tab];
            tab.miller_columns.clear();
        }
        self.sync_fs_watches();
        true
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
        // Selection and cursor are name-based, so a re-sort keeps them on the
        // same files instead of shifting the highlight to whatever index the
        // new order happens to place there.
        let visible = self.visible();
        let tab = &mut self.tabs[self.active_tab];
        tab.resync_selection(&visible);
        self.rebuild_miller_columns();
        self.sync_miller_selection();
        self.save_config();
    }

    /// Rebuild the Miller column model: parent, then the active directory.
    ///
    /// Only meaningful in Miller view: building it otherwise re-reads the
    /// parent directory on every navigation, refresh and sort for a model
    /// nothing renders. Older layouts kept up to three ancestors; with the
    /// active directory centred only the parent is on screen.
    fn rebuild_miller_columns(&mut self) {
        const MAX_COLUMNS: usize = 2;
        if self.view_mode != ViewMode::Miller {
            return;
        }
        let _span = crate::telemetry::span("state.miller_rebuild");
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

    /// Key for the Miller preview column: the directory under the cursor in
    /// the active column, or `None` when the cursor is on a file, nothing is
    /// selected, or the view is not Miller.
    #[must_use]
    pub fn miller_preview_key(&self) -> Option<crate::miller_preview::PreviewKey> {
        if self.view_mode != ViewMode::Miller {
            return None;
        }
        let entry = self.selected_entry()?;
        if !entry.navigable {
            return None;
        }
        Some(crate::miller_preview::PreviewKey {
            path: entry::child_path(self.cwd(), &entry.name),
            show_hidden: self.show_hidden,
            sort_key: self.sort_key,
            sort_ascending: self.sort_ascending,
        })
    }

    /// Make the preview column's listing current for the cursor. Call once
    /// per frame in Miller view; it is a no-op while the cursor stays put.
    /// `asynchronous` reads off-thread (interactive window); otherwise the
    /// read is inline (tests, embedders without a frame loop).
    pub fn request_miller_preview(&mut self, asynchronous: bool) {
        match self.miller_preview_key() {
            Some(key) => self.miller_preview.request(key, asynchronous),
            None => self.miller_preview.clear(),
        }
    }

    /// Publish a finished preview read. Returns true when it changed.
    pub fn poll_miller_preview(&mut self) -> bool {
        self.miller_preview.poll()
    }

    /// Whether a preview read is in flight (the host keeps painting).
    #[must_use]
    pub fn is_loading_miller_preview(&self) -> bool {
        self.miller_preview.is_loading()
    }

    /// The preview listing for the current cursor, once it is ready.
    #[must_use]
    pub fn miller_preview(&self) -> Option<&crate::miller_preview::MillerPreview> {
        self.miller_preview.get(&self.miller_preview_key()?)
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

    /// Open `name` with optional activation token.
    pub fn open_child_with_token(&mut self, name: &str, token: Option<&str>) {
        let target = entry::child_path(self.cwd(), name);
        if entry::dir_exists(&target) {
            self.navigate(&target);
        } else if let Err(e) = ops::open_with_token(&target, token) {
            self.set_status(format!("Cannot open {name}: {e}"));
        }
    }

    /// Open `name` (a child of cwd) — the mouse path of
    /// [`Self::open_selected_with_token`].
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
                self.undo_stack
                    .push(crate::undo::UndoAction::CreateFolder(path));
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
        match trash::trash_paths_record(&path_refs, self.trash_root.as_deref()) {
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
    /// Stage the selection on the clipboard.
    pub fn yank_selected(&mut self, action: TransferAction) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        self.clipboard = Some(Clipboard::new(path_bufs, action));
        self.set_status(match action {
            TransferAction::Move => format!("Cut {count} item(s) — paste with Ctrl+V"),
            TransferAction::Copy => format!("Copied {count} item(s) — paste with Ctrl+V"),
        });
    }

    /// Transfer `sources` into `destination`, recording one undo transaction
    /// for everything that succeeded.
    ///
    /// Shared by paste and drag-and-drop so both obey identical collision,
    /// self-containment, and undo rules. Returns the number of items moved.
    fn transfer_into(
        &mut self,
        sources: &[PathBuf],
        destination: &Path,
        action: TransferAction,
    ) -> usize {
        let mut created = Vec::new();
        let mut errors = Vec::new();
        for src in sources {
            let result = match action {
                TransferAction::Move => ops::move_into(src, destination),
                TransferAction::Copy => ops::copy_into(src, destination),
            };
            match result {
                Ok(dst) => created.push(dst),
                Err(error) => errors.push(format!(
                    "{}: {error}",
                    src.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }

        if !created.is_empty() {
            match action {
                TransferAction::Move => self.undo_stack.push(crate::undo::UndoAction::Move {
                    sources: sources.to_vec(),
                    destinations: created.clone(),
                }),
                TransferAction::Copy => self.undo_stack.push(crate::undo::UndoAction::Copy {
                    destinations: created.clone(),
                }),
            }
        }

        let done = created.len();
        let verb = match action {
            TransferAction::Move => "Moved",
            TransferAction::Copy => "Copied",
        };
        match (done, errors.first()) {
            (0, Some(error)) => self.set_status(format!("Cannot transfer — {error}")),
            (0, None) => {}
            (n, Some(error)) => self.set_status(format!(
                "{verb} {n} item{}, {} failed ({error})",
                if n == 1 { "" } else { "s" },
                errors.len()
            )),
            (n, None) => {
                self.set_status(format!("{verb} {n} item{}", if n == 1 { "" } else { "s" }))
            }
        }
        done
    }

    /// Paste the clipboard into cwd.
    pub fn paste(&mut self) {
        let Some(clipboard) = self.clipboard.clone() else {
            return;
        };
        if self.is_transferring() {
            self.set_status("A transfer is already running".into());
            return;
        }
        // Large transfers go to the IO worker so the window keeps painting
        // and can report progress; small ones finish within a frame.
        if Self::total_transfer_bytes(&clipboard.paths) >= Self::ASYNC_TRANSFER_THRESHOLD {
            // The clipboard stays staged until the worker reports success:
            // clearing it up front would lose the source list if the job is
            // cancelled or fails. `poll_async_jobs` clears it on success.
            self.paste_async(&clipboard);
            return;
        }
        let destination = PathBuf::from(self.cwd());
        let total = clipboard.paths.len();
        let done = self.transfer_into(&clipboard.paths, &destination, clipboard.action);
        // A cut clipboard is consumed only once every item actually moved;
        // a partial failure leaves it staged for a retry elsewhere.
        if clipboard.action == TransferAction::Move && done == total {
            self.clipboard = None;
        }
        if done > 0 {
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

    /// Whether a background transfer is currently running.
    pub fn is_transferring(&self) -> bool {
        self.io_engine.is_busy()
    }

    /// Cancel the in-flight background transfer, if any.
    pub fn cancel_transfer(&mut self) {
        self.io_engine.cancel_active();
    }

    /// Total bytes of `paths`, for deciding whether a transfer needs a worker.
    fn total_transfer_bytes(paths: &[PathBuf]) -> u64 {
        fn measure(path: &Path) -> u64 {
            // `symlink_metadata` keeps the walk out of symlinked directories.
            match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_dir() => std::fs::read_dir(path)
                    .map(|entries| entries.flatten().map(|e| measure(&e.path())).sum())
                    .unwrap_or(0),
                Ok(meta) => meta.len(),
                Err(_) => 0,
            }
        }
        paths.iter().map(|path| measure(path)).sum()
    }

    /// Transfers at or above this size run on the IO worker.
    ///
    /// Below it the synchronous path is faster (no thread hand-off, no
    /// progress round-trips) and finishes inside a frame.
    const ASYNC_TRANSFER_THRESHOLD: u64 = 8 * 1024 * 1024;

    /// Paste the clipboard into cwd, off the UI thread when it is large.
    fn paste_async(&mut self, clipboard: &Clipboard) {
        let destination = PathBuf::from(self.cwd());
        self.io_engine
            .submit(clipboard.action, clipboard.paths.clone(), destination);
        self.set_status(format!(
            "{} {} item(s)…",
            match clipboard.action {
                TransferAction::Move => "Moving",
                TransferAction::Copy => "Copying",
            },
            clipboard.paths.len()
        ));
    }

    /// Poll for async job progress and completion, committing finished jobs to
    /// the UndoStack. Returns `true` when the listing changed.
    pub fn poll_async_jobs(&mut self) -> bool {
        let mut completed_any = false;
        while let Some(completed) = self.io_engine.poll() {
            completed_any = true;
            let moved = matches!(completed.kind, crate::worker::JobKind::Move { .. });
            let created = completed.created_destinations.clone();
            if !created.is_empty() {
                match &completed.kind {
                    crate::worker::JobKind::Copy { .. } => {
                        self.undo_stack.push(crate::undo::UndoAction::Copy {
                            destinations: created,
                        })
                    }
                    crate::worker::JobKind::Move { sources, .. } => {
                        self.undo_stack.push(crate::undo::UndoAction::Move {
                            sources: sources.clone(),
                            destinations: created,
                        })
                    }
                }
            }
            match (&completed.error, completed.path_count()) {
                (Some(error), _) => self.set_status(format!("Transfer failed — {error}")),
                (None, Some(count)) => {
                    // A completed move consumes the staged cut; clearing it
                    // earlier would lose the sources on cancellation.
                    if moved {
                        self.clipboard = None;
                    }
                    self.set_status(format!(
                        "{} {count} item{}",
                        if moved { "Moved" } else { "Copied" },
                        if count == 1 { "" } else { "s" }
                    ));
                }
                (None, None) => {}
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
    /// Handle a drop payload produced by the drag source.
    ///
    /// The payload is an interchange format (`text/uri-list`, gnome-copied-
    /// files, or bare paths) — never a single raw path — because that is what
    /// both the in-app drag source and every external file manager publish.
    /// `action` is the fallback for formats that cannot encode one.
    ///
    /// Returns `true` when at least one item was transferred.
    pub fn drop_into(&mut self, payload: &str, dst_dir: &str, action: TransferAction) -> bool {
        let Some(transfer) = Clipboard::parse(payload, action, false) else {
            self.set_status("Dropped data carried no usable file paths".into());
            return false;
        };
        let destination = PathBuf::from(dst_dir);
        // A drop onto the directory the items already live in is a no-op, not
        // an error: the pointer simply never left the source row.
        let sources: Vec<PathBuf> = transfer
            .paths
            .into_iter()
            .filter(|src| src.parent() != Some(destination.as_path()))
            .collect();
        if sources.is_empty() {
            return false;
        }
        let moved = self.transfer_into(&sources, &destination, transfer.action) > 0;
        if moved {
            self.refresh();
        }
        moved
    }

    // ---- Trash -----------------------------------------------------------

    /// Every item currently in the user's home Trash, newest first.
    ///
    /// The sidebar opens `…/Trash/files`, so the listing itself only holds
    /// bare file names. The original location and deletion date live in the
    /// companion `.trashinfo` records and are surfaced from here.
    pub fn trash_items(&self) -> Vec<trash::TrashItem> {
        match &self.trash_root {
            Some(root) => trash::list_trash_in_root(root).unwrap_or_default(),
            None => trash::list_trash().unwrap_or_default(),
        }
    }

    /// The `.trashinfo` record backing a trash listing row, if `name` is a
    /// trashed item id in the current directory.
    pub fn trash_record(&self, name: &str) -> Option<trash::TrashItem> {
        if !self.is_showing_trash() {
            return None;
        }
        self.trash_index
            .as_ref()
            .and_then(|index| index.get(name))
            .cloned()
    }

    /// Whether the active tab is showing a Trash listing.
    pub fn is_showing_trash(&self) -> bool {
        trash::is_trash_files_dir(Path::new(self.cwd()))
    }

    /// Restore the given trash ids. Returns how many were restored.
    pub fn restore_trash_ids(&mut self, ids: &[String]) -> usize {
        let restored = self
            .trash_items()
            .into_iter()
            .filter(|item| ids.contains(&item.id))
            .filter(|item| trash::restore_trash_item(item).is_ok())
            .count();
        if restored > 0 {
            self.set_status(format!(
                "Restored {restored} item{}",
                if restored == 1 { "" } else { "s" }
            ));
            self.refresh();
        } else {
            self.set_status("Could not restore the selection".into());
        }
        restored
    }

    /// Permanently delete everything in the home Trash. Returns the count.
    pub fn empty_trash(&mut self) -> usize {
        let count = match &self.trash_root {
            Some(_) => {
                let items = self.trash_items();
                let count = items.len();
                for item in &items {
                    let _ = trash::purge_trash_item(item);
                }
                count
            }
            None => trash::empty_trash().unwrap_or(0),
        };
        self.set_status(format!("Emptied Trash ({count} items deleted)"));
        self.refresh();
        count
    }

    /// Whether `path` is the Trash configured for this state (root or
    /// `files/`) — the sidebar offers "Empty Trash" there.
    #[must_use]
    pub fn is_trash_location(&self, path: &str) -> bool {
        let path = Path::new(path);
        if let Some(root) = &self.trash_root {
            return path == root || path == root.join("files");
        }
        trash::is_trash_files_dir(path)
            || path == Path::new(&trash::home_trash_root())
            || path == Path::new(&trash::home_trash_files_dir())
    }

    /// Permanently delete the given trash ids. Returns how many were purged.
    pub fn purge_trash_ids(&mut self, ids: &[String]) -> usize {
        let purged = self
            .trash_items()
            .into_iter()
            .filter(|item| ids.contains(&item.id))
            .filter(|item| trash::purge_trash_item(item).is_ok())
            .count();
        if purged > 0 {
            self.set_status(format!("Permanently deleted {purged} item(s)"));
            self.refresh();
        }
        purged
    }

    /// Restore every selected item when the tab shows the Trash.
    pub fn restore_selected(&mut self) -> usize {
        let names = self.selected_entry_names();
        if names.is_empty() {
            self.set_status("Select an item to restore".into());
            return 0;
        }
        self.restore_trash_ids(&names)
    }

    /// The ids of the trash items backing the current selection.
    ///
    /// Trash ids are the stored file names (`Trash/files/<id>`), which is
    /// also what the listing shows — so the selection names *are* the ids.
    pub fn selected_entry_names(&self) -> Vec<String> {
        self.selected_entries()
            .into_iter()
            .map(|entry| entry.name.clone())
            .collect()
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
        // The status bar is the user's feedback channel; the log keeps the
        // same line so a report can be reconstructed after the fact.
        log::info!(target: "arca::status", "{msg}");
        self.status = Some(msg);
    }

    /// Select exactly the entry named `name` (cursor + selection).
    ///
    /// The cursor is always a member of the selection — there is no
    /// "focused but unselected" state to reason about, and no operation can
    /// depend on a cursor whose row is not selected. The name is
    /// authoritative, so the selection survives later re-sorts and re-reads.
    fn select_by_name(&mut self, name: &str) {
        let visible = self.visible();
        let tab = &mut self.tabs[self.active_tab];
        tab.selected_names = BTreeSet::from([name.to_string()]);
        tab.cursor_name = Some(name.to_string());
        tab.anchor_name = Some(name.to_string());
        tab.resync_selection(&visible);
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

    fn wait_for_navigation(state: &mut AppState) {
        let start = std::time::Instant::now();
        while state.is_navigating() {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(5),
                "navigation did not finish"
            );
            state.poll_navigation();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // Drain the frame that published the listing.
        state.poll_navigation();
    }

    #[test]
    fn async_navigation_publishes_the_listing() {
        let (dir, mut state) = fixture();
        let child = dir.join("sub");
        std::fs::write(child.join("inside.txt"), b"x").unwrap();

        assert!(state.begin_navigate(child.to_str().unwrap()));
        assert!(state.is_navigating());
        assert_eq!(
            state.cwd(),
            dir.to_str().unwrap(),
            "old listing stays visible"
        );

        wait_for_navigation(&mut state);
        assert_eq!(state.cwd(), child.to_str().unwrap());
        assert_eq!(
            state
                .entries()
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            ["inside.txt"]
        );
        assert!(!state.is_navigating());
        assert!(state.status.is_none(), "the loading status is cleared");
    }

    #[test]
    fn async_navigation_reports_an_invalid_path_without_moving() {
        let (dir, mut state) = fixture();
        let missing = dir.join("missing");
        assert!(state.begin_navigate(missing.to_str().unwrap()));
        wait_for_navigation(&mut state);

        assert_eq!(state.cwd(), dir.to_str().unwrap());
        assert!(!state.can_go_back(), "a failed read must not enter history");
        assert!(
            state
                .status
                .as_deref()
                .is_some_and(|status| status.starts_with("Not a directory")),
            "status was {:?}",
            state.status
        );
    }

    #[test]
    fn newer_async_navigation_supersedes_an_older_one() {
        let (dir, mut state) = fixture();
        let first = dir.join("sub");
        let second = dir.join("sub2");
        std::fs::create_dir(&second).unwrap();

        assert!(state.begin_navigate(first.to_str().unwrap()));
        assert!(state.begin_navigate(second.to_str().unwrap()));
        wait_for_navigation(&mut state);

        assert_eq!(state.cwd(), second.to_str().unwrap());
    }

    #[test]
    fn sync_navigation_cancels_an_in_flight_request() {
        let (dir, mut state) = fixture();
        let async_target = dir.join("sub");
        assert!(state.begin_navigate(async_target.to_str().unwrap()));
        state.navigate(dir.to_str().unwrap());
        assert!(!state.is_navigating());
        state.poll_navigation();
        assert_eq!(state.cwd(), dir.to_str().unwrap());
    }

    #[test]
    fn async_history_navigation_keeps_the_cursor() {
        let (dir, mut state) = fixture();
        let child = dir.join("sub");
        std::fs::write(child.join("inside.txt"), b"x").unwrap();
        state.navigate(child.to_str().unwrap());
        assert!(state.can_go_back());

        assert!(state.begin_go_back());
        wait_for_navigation(&mut state);
        assert_eq!(state.cwd(), dir.to_str().unwrap());
        assert!(state.can_go_forward());

        assert!(state.begin_go_forward());
        wait_for_navigation(&mut state);
        assert_eq!(state.cwd(), child.to_str().unwrap());
    }

    /// Regression: `begin_go_back` used to move the history cursor before the
    /// read finished, so a Back into a deleted directory left the tab showing
    /// one location while history claimed another.
    #[test]
    fn a_failed_async_back_leaves_the_history_cursor_in_place() {
        let (dir, mut state) = fixture();
        let doomed = dir.join("doomed");
        let current = dir.join("sub");
        std::fs::create_dir(&doomed).unwrap();
        state.navigate(doomed.to_str().unwrap());
        state.navigate(current.to_str().unwrap());
        std::fs::remove_dir(&doomed).unwrap();

        assert!(state.begin_go_back());
        wait_for_navigation(&mut state);

        assert_eq!(state.cwd(), current.to_str().unwrap(), "stays put");
        assert!(!state.can_go_forward(), "the cursor did not move back");
        assert!(state.can_go_back());
        assert!(
            state
                .status
                .as_deref()
                .is_some_and(|s| s.starts_with("Not a directory"))
        );
    }

    #[test]
    fn rapid_async_back_clicks_walk_history_and_publish_once() {
        let (dir, mut state) = fixture();
        let a = dir.join("sub");
        let b = a.join("deeper");
        std::fs::create_dir(&b).unwrap();
        state.navigate(a.to_str().unwrap());
        state.navigate(b.to_str().unwrap());

        assert!(state.begin_go_back());
        assert!(state.begin_go_back(), "second click stacks on the first");
        assert!(!state.begin_go_back(), "nothing further back");
        wait_for_navigation(&mut state);

        assert_eq!(state.cwd(), dir.to_str().unwrap());
        assert!(!state.can_go_back());
        assert!(state.begin_go_forward());
        wait_for_navigation(&mut state);
        assert_eq!(state.cwd(), a.to_str().unwrap());
    }

    /// `[INV-UI-04]` asynchronous navigation performs no blocking directory
    /// read on the UI thread outside Miller view; the synchronous API does.
    #[test]
    fn async_navigation_keeps_directory_reads_off_the_ui_thread() {
        std::thread::spawn(|| {
            let (dir, mut state) = fixture();
            crate::telemetry::span::mark_ui_thread();
            let before = crate::telemetry::span::ui_blocking_io_count();
            assert!(state.begin_navigate(dir.join("sub").to_str().unwrap()));
            wait_for_navigation(&mut state);
            assert!(state.begin_go_back());
            wait_for_navigation(&mut state);
            assert_eq!(
                crate::telemetry::span::ui_blocking_io_count(),
                before,
                "async navigation read a directory on the UI thread"
            );

            state.navigate(dir.join("sub").to_str().unwrap());
            assert!(
                crate::telemetry::span::ui_blocking_io_count() > before,
                "the counter does observe synchronous reads"
            );
        })
        .join()
        .unwrap();
    }

    #[test]
    fn navigation_outcomes_are_measured() {
        let registry = crate::telemetry::metrics::global();
        let completed = registry.counter_value("nav.completed");
        let failed = registry.counter_value("nav.failed");
        let (dir, mut state) = fixture();
        assert!(state.begin_navigate(dir.join("sub").to_str().unwrap()));
        wait_for_navigation(&mut state);
        assert!(state.begin_navigate(dir.join("absent").to_str().unwrap()));
        wait_for_navigation(&mut state);
        assert!(registry.counter_value("nav.completed") > completed);
        assert!(registry.counter_value("nav.failed") > failed);
        assert!(
            registry
                .histogram("nav.latency")
                .is_some_and(|h| h.count() >= 2)
        );
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
        s.yank_selected(TransferAction::Copy);
        s.navigate("dst");
        s.paste();
        assert!(dir.join("dst/a.txt").exists(), "status: {:?}", s.status);
        assert!(dir.join("a.txt").exists(), "copy keeps the source");

        // Cut b.txt, paste into dst.
        s.go_up();
        s.select_by_name("b.txt");
        s.yank_selected(TransferAction::Move);
        s.navigate("dst");
        s.paste();
        assert!(dir.join("dst/b.txt").exists());
        assert!(!dir.join("b.txt").exists(), "cut removes the source");
        assert!(s.clipboard.is_none(), "cut clipboard consumed after paste");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The engine's clipboard type *is* the XDG payload type, so the
    /// gnome-copied-files verb, the URI list, and a paste all round-trip
    /// through one implementation (see `arca_xdg::clipboard` for the format
    /// contract itself).
    #[test]
    fn clipboard_uses_the_xdg_transfer_payload() {
        let (dir, mut s) = fixture();
        let file1 = dir.join("a.txt");
        let file2 = dir.join("b.txt");

        // Directories sort first, so rows 1..=2 are a.txt and b.txt.
        s.select_visible(1);
        s.select_range_visible(2);
        assert_eq!(s.selected_count(), 2);
        s.yank_selected(TransferAction::Copy);
        let clipboard = s.clipboard.clone().expect("staged");
        assert_eq!(clipboard.action, TransferAction::Copy);
        assert_eq!(clipboard.paths, vec![file1.clone(), file2.clone()]);
        assert_eq!(
            s.clipboard_payload().unwrap(),
            clipboard.to_gnome_copied_files()
        );

        // The URI list a drag publishes parses back to the same paths.
        let parsed = Clipboard::parse(&clipboard.to_uri_list(), TransferAction::Move, false)
            .expect("uri-list parses");
        assert_eq!(parsed.paths, vec![file1, file2]);
        assert_eq!(parsed.action, TransferAction::Move);

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
        let file_a = dir.join("a.txt");

        assert!(!s.is_bookmarked(&sub));
        s.add_bookmark(&sub);
        assert!(s.is_bookmarked(&sub));
        s.add_bookmark(&sub);
        assert!(s.is_bookmarked(&sub));

        // A drop arrives as an interchange payload, exactly like an external
        // drag from another file manager.
        let payload =
            arca_xdg::FileClipboard::new(vec![file_a.clone()], TransferAction::Move).to_uri_list();
        assert!(s.drop_into(&payload, &sub, TransferAction::Move));
        assert!(!file_a.exists());
        assert!(dir.join("sub").join("a.txt").exists());

        s.toggle_bookmark(&sub); // clean up gtk bookmark

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A multi-item drag from the listing used to be dropped wholesale: the
    /// raw `text/uri-list` string was handed to `PathBuf::from`, so nothing
    /// transferred and the status line reported a bogus `ENOENT`.
    #[test]
    fn drop_transfers_every_item_in_a_uri_list_payload() {
        let (dir, mut s) = fixture();
        let dst = dir.join("dst");
        std::fs::create_dir_all(&dst).unwrap();
        let sources: Vec<PathBuf> = ["a.txt", "b.txt"]
            .iter()
            .map(|name| dir.join(name))
            .collect();

        let payload =
            arca_xdg::FileClipboard::new(sources.clone(), TransferAction::Move).to_uri_list();
        assert!(s.drop_into(&payload, dst.to_str().unwrap(), TransferAction::Move));

        for source in &sources {
            let name = source.file_name().unwrap();
            assert!(!source.exists(), "source must be consumed on move");
            assert!(dst.join(name).exists(), "destination must receive {name:?}");
        }

        // Undo restores both in one transaction.
        s.undo();
        for source in &sources {
            assert!(source.exists(), "undo must restore every moved item");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A copy-action drop leaves the sources in place.
    #[test]
    fn drop_honours_the_copy_action() {
        let (dir, mut s) = fixture();
        let dst = dir.join("dst");
        std::fs::create_dir_all(&dst).unwrap();
        let source = dir.join("a.txt");
        let payload =
            arca_xdg::FileClipboard::new(vec![source.clone()], TransferAction::Copy).to_uri_list();

        assert!(s.drop_into(&payload, dst.to_str().unwrap(), TransferAction::Copy));
        assert!(source.exists(), "copy keeps the source");
        assert!(dst.join("a.txt").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Dropping a directory onto its own descendant must be refused, not
    /// recursively copied into itself.
    #[test]
    fn drop_refuses_a_directory_into_its_own_descendant() {
        let (dir, mut s) = fixture();
        let payload =
            arca_xdg::FileClipboard::new(vec![dir.clone()], TransferAction::Copy).to_uri_list();

        assert!(!s.drop_into(
            &payload,
            dir.join("sub").to_str().unwrap(),
            TransferAction::Copy
        ));
        assert!(
            s.status
                .as_deref()
                .is_some_and(|status| status.contains("itself")),
            "status must explain the refusal, got {:?}",
            s.status
        );
        assert_eq!(
            std::fs::read_dir(dir.join("sub")).unwrap().count(),
            0,
            "nothing may be created inside the target"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A drop back onto the directory the item already lives in is a no-op.
    #[test]
    fn drop_onto_the_same_directory_is_a_no_op() {
        let (dir, mut s) = fixture();
        let payload = arca_xdg::FileClipboard::new(vec![dir.join("a.txt")], TransferAction::Move)
            .to_uri_list();

        assert!(!s.drop_into(&payload, dir.to_str().unwrap(), TransferAction::Move));
        assert!(dir.join("a.txt").exists());
        assert!(!dir.join("a.txt (copy)").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn drop_of_a_non_file_payload_is_reported() {
        let (dir, mut s) = fixture();
        assert!(!s.drop_into("hello world", dir.to_str().unwrap(), TransferAction::Copy));
        assert!(
            s.status
                .as_deref()
                .is_some_and(|status| status.contains("no usable file paths"))
        );

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
    fn move_tab_reorders_and_follows_active_tab() {
        let (dir, mut s) = fixture();
        s.new_tab(None); // tab 1
        s.new_tab(None); // tab 2
        assert_eq!(s.tabs().len(), 3);
        assert_eq!(s.active_tab_index(), 2);

        // optics-ui reports `to` in post-removal coordinates.
        assert!(s.move_tab(2, 0));
        assert_eq!(s.active_tab_index(), 0);
        let ids: Vec<u64> = s.tabs().iter().map(|t| t.id).collect();
        assert_eq!(ids[0], 3, "dragged tab lands at the insertion slot");

        // Moving a non-active tab leftwards past the active tab shifts the
        // active index up (insertion before it pushes it right).
        s.switch_tab(0);
        let active_id = s.active_tab_id();
        assert!(s.move_tab(2, 0));
        assert_eq!(s.active_tab_id(), active_id, "active follows its tab");

        // Degenerate moves are rejected.
        assert!(!s.move_tab(0, 0));
        assert!(!s.move_tab(0, s.tabs().len()));
        assert!(!s.move_tab(s.tabs().len(), 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn miller_model_is_parent_then_active_directory() {
        let (dir, mut s) = fixture();
        s.set_view_mode(ViewMode::Miller);
        s.navigate("sub");
        let columns = s.miller_columns();
        assert_eq!(columns.len(), 2, "parent + active, no deeper ancestors");
        assert_eq!(columns[1].path, s.cwd());
        s.navigate("/");
        assert_eq!(s.miller_columns().len(), 1, "the root has no parent column");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn miller_preview_follows_the_cursor() {
        let (dir, mut s) = fixture();
        s.set_view_mode(ViewMode::Miller);
        s.select_by_name("sub");
        s.request_miller_preview(false);
        let preview = s
            .miller_preview()
            .expect("directory under cursor is previewed");
        assert_eq!(preview.key.path, entry::child_path(s.cwd(), "sub"));
        assert!(preview.listing.is_ok());

        // A file under the cursor has no directory preview.
        let file = s
            .entries()
            .iter()
            .find(|e| !e.navigable)
            .map(|e| e.name.clone())
            .expect("fixture has a file");
        s.select_by_name(&file);
        s.request_miller_preview(false);
        assert!(s.miller_preview_key().is_none());
        assert!(s.miller_preview().is_none());

        // Outside Miller view nothing is previewed.
        s.select_by_name("sub");
        s.set_view_mode(ViewMode::List);
        assert!(s.miller_preview_key().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn miller_columns_end_at_active_directory() {
        let (dir, mut s) = fixture();
        // The column model is built for Miller view only.
        s.set_view_mode(ViewMode::Miller);
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

    /// Every entries write must keep the derived selection caches coherent.
    ///
    /// The watcher used to assign `tab.entries` for background tabs without
    /// resyncing, so `selected_indices` kept the old row numbers while the
    /// rows now held different files — the next `Delete` in that tab would
    /// act on the wrong entry. `TabState::debug_assert_selection_coherent`
    /// encodes the invariant; this test drives every entries-writing path.
    #[test]
    fn every_listing_reload_keeps_the_selection_coherent() {
        let (dir, mut s) = fixture();

        // Two tabs over the same directory, the second with a selection.
        s.new_tab(Some(dir.to_str().unwrap()));
        s.select_visible(1);
        assert_eq!(s.selected_count(), 1);
        s.tabs()[1].debug_assert_selection_coherent(&s.visible_in_tab(1));

        // Insert entries that sort *before* the selected one, shifting every
        // index. Any path that reloads without resyncing now corrupts state.
        for name in ["0-first.txt", "1-second.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        // 1. Watcher reload of a background tab.
        assert!(s.switch_tab(0));
        s.reload_tab(1);
        s.tabs()[1].debug_assert_selection_coherent(&s.visible_in_tab(1));

        // 2. Explicit refresh of the active tab.
        s.refresh();
        s.tabs()[0].debug_assert_selection_coherent(&s.visible_in_tab(0));

        // 3. Sort toggle.
        s.toggle_sort(SortKey::Size);
        s.tabs()[0].debug_assert_selection_coherent(&s.visible_in_tab(0));

        // 4. Hidden-file toggle (re-reads every tab).
        s.toggle_hidden();
        for index in 0..s.tabs().len() {
            s.tabs()[index].debug_assert_selection_coherent(&s.visible_in_tab(index));
        }

        // 5. Filter edit.
        s.set_filter("txt".into());
        s.tabs()[0].debug_assert_selection_coherent(&s.visible_in_tab(0));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The trash metadata index must reflect each listing, not linger from a
    /// previous one — and it must be empty outside a Trash listing.
    #[test]
    fn trash_index_tracks_the_active_listing() {
        let (dir, mut s) = fixture();
        assert!(s.trash_record("anything").is_none(), "no Trash listing yet");

        // A plain directory is never a Trash listing.
        s.navigate(dir.to_str().unwrap());
        assert!(!s.is_showing_trash());
        assert!(s.trash_record("anything").is_none());

        // Entering the trash files/ directory builds the index.
        let trash_files = arca_xdg::home_trash_files_dir();
        if !trash_files.is_dir() {
            eprintln!("no home trash on this host; skipping");
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }
        s.navigate(trash_files.to_str().unwrap());
        assert!(s.is_showing_trash(), "files/ must be recognised as Trash");
        let index = s
            .trash_index
            .as_ref()
            .expect("the index must be built for a Trash listing");
        for (id, item) in index {
            assert_eq!(id, &item.id, "index keys are trash ids");
        }

        // Leaving drops it.
        s.navigate(dir.to_str().unwrap());
        assert!(
            s.trash_index.is_none(),
            "the index must not survive navigation"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Miller columns are only built in Miller view: building them for a
    /// List/Grid navigation re-reads four ancestor directories that nothing
    /// renders.
    #[test]
    fn miller_columns_are_not_built_outside_miller_view() {
        let (dir, mut s) = fixture();
        s.set_view_mode(ViewMode::List);
        s.navigate("sub");
        assert!(s.miller_columns().is_empty());

        s.set_view_mode(ViewMode::Miller);
        assert!(
            !s.miller_columns().is_empty(),
            "switching in builds the model"
        );

        s.set_view_mode(ViewMode::Grid);
        s.refresh();
        assert!(s.miller_columns().is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `--grid`/`--list`/`--miller` are launch-time overrides. Persisting them
    /// would silently rewrite the user's saved default on every invocation,
    /// so the override must leave the config file untouched.
    #[test]
    fn a_cli_view_override_does_not_rewrite_the_config() {
        let (dir, mut s) = fixture();
        let config = dir.join("cfg/arca.conf");

        // Establish a saved default.
        s.set_view_mode(ViewMode::List);
        let saved = std::fs::read_to_string(&config).unwrap();
        assert!(saved.contains("view = list"), "got {saved:?}");

        // A one-shot override changes the live mode only.
        s.override_view_mode(ViewMode::Miller);
        assert_eq!(s.view_mode, ViewMode::Miller);
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            saved,
            "an override must not touch the config file"
        );

        // Re-opening from that config still yields the saved default.
        let reopened = config::load(config.to_str().unwrap());
        assert_eq!(reopened.view_mode, ViewMode::List);

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
        if s.watcher.is_none() {
            eprintln!("inotify instances exhausted on host; skipping test");
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }
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

        assert!(
            removed_updated,
            "expected drain_fs_events to report refresh after deletion"
        );
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
        s.yank_selected(TransferAction::Copy);
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

    #[test]
    fn navigate_decodes_file_uris_and_reveals_file_parent() {
        let (dir, mut s) = fixture();
        let sub = dir.join("My Folder");
        std::fs::create_dir(&sub).unwrap();
        let file = sub.join("notes.txt");
        std::fs::write(&file, b"hi").unwrap();

        let dir_uri = crate::xdg::encode_file_uri(&sub);
        s.navigate(&dir_uri);
        assert_eq!(s.cwd(), sub.to_string_lossy());

        // Navigating back to dir, then opening a file:// URI pointing at a
        // regular file must land on its parent directory.
        s.navigate(&dir.to_string_lossy());
        assert_eq!(s.cwd(), dir.to_string_lossy());
        let file_uri = crate::xdg::encode_file_uri(&file);
        s.navigate(&file_uri);
        assert_eq!(s.cwd(), sub.to_string_lossy());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
