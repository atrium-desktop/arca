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
pub mod ops;
pub mod path;
pub mod preview;
pub mod state;
pub mod thumbs;

// Re-export standard Linux/XDG specifications from arca-xdg
pub use arca_xdg as xdg;
pub use arca_xdg::desktop;
pub use arca_xdg::mime;
pub use arca_xdg::trash;

pub use bookmarks::{Bookmark, BookmarkKind};
pub use chooser::{
    BytePath, Choice, FileChooserMode, FileChooserRequest, FileChooserResponse, FileFilter,
    FilterRule, FilterRuleKind, PromptAccent, PromptAppearance, PromptColorScheme,
    PrompterRequest, PrompterResponse,
};
pub use config::{Config, ThemeMode, ViewMode};
pub use desktop::DesktopEntry;
pub use entry::{Entry, FileType, SortKey};
pub use mime::MimeDatabase;
pub use path::{complete_path, PathCompletion};
pub use preview::{Preview, PreviewKind};
pub use state::{AppState, Clipboard, MillerColumn, TabState};
pub use thumbs::{Thumb, ThumbService};
pub use trash::TrashItem;
