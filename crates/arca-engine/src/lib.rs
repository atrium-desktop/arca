//! Core business logic for the Arca file manager.
//!
//! This crate is deliberately free of any GUI dependency: it models the
//! directory listing, navigation history, bookmarks, sorting/filtering,
//! file operations and persistent configuration. The UI layer
//! (`arca-ui`) renders an [`state::AppState`] and forwards user events
//! to it. Everything here is unit-testable headlessly.

pub mod bookmarks;
pub mod chooser;
pub mod config;
pub mod entry;
pub mod format;
pub mod history;
pub mod ipc;
pub mod miller_preview;
pub mod ops;
pub mod path;
pub mod preview;
pub mod state;
pub mod telemetry;
pub mod thumbs;
pub mod undo;
pub mod watch;
pub mod worker;

// Re-export standard Linux/XDG specifications from arca-xdg
pub use arca_xdg as xdg;
pub use arca_xdg::desktop;
pub use arca_xdg::mime;
pub use arca_xdg::trash;

pub use bookmarks::{Bookmark, BookmarkKind};
pub use chooser::{
    ARCA_CONTRACT_VERSION, BytePath, Choice, ChooserRequestEnvelope, ChooserResponseEnvelope,
    FileChooserMode, FileChooserRequest, FileChooserResponse, FileFilter, FilterRule,
    FilterRuleKind, read_chooser_request, write_chooser_response,
};
pub use config::{Config, ThemeMode, ViewMode};
pub use desktop::DesktopEntry;
pub use entry::{Entry, FileType, SortKey};
pub use miller_preview::{MillerPreview, PreviewKey};
pub use mime::MimeDatabase;
pub use path::{PathCompletion, complete_path};
pub use preview::{Preview, PreviewKind};
pub use state::{AppState, Clipboard, MillerColumn, TabState, TransferAction};
pub use thumbs::{Thumb, ThumbService};
pub use trash::TrashItem;
pub use undo::{UndoAction, UndoStack};
pub use watch::FsWatcher;
pub use worker::{AsyncIoEngine, JobCompleted, JobId, JobKind, JobProgress};
