//! `arca-xdg`: Pure-Rust, zero-external-dependency Freedesktop.org (XDG) specifications.
//!
//! Compliant implementations of:
//! - [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/)
//! - [XDG User Directories Specification](https://wiki.freedesktop.org/wiki/Software/xdg-user-dirs/)
//! - [RFC 8089 File URI Specification](https://datatracker.ietf.org/doc/html/rfc8089)
//! - [XDG Shared MIME-info Specification](https://specifications.freedesktop.org/shared-mime-info-spec/latest/)
//! - [XDG Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/)
//! - [XDG MIME Applications Specification](https://specifications.freedesktop.org/mime-apps-spec/latest/)
//! - [XDG Trash Specification v1.0](https://specifications.freedesktop.org/trash-spec/latest/)
//! - [RFC 1321 MD5 Specification](https://datatracker.ietf.org/doc/html/rfc1321) (Thumbnail Standard)
//! - Standard Linux Desktop Clipboard interchange protocols

pub mod base;
pub mod clipboard;
pub mod desktop;
pub mod md5;
pub mod mime;
pub mod trash;
pub mod uri;
pub mod user_dirs;

// Re-exports for concise ergonomic access
pub use base::{
    all_config_dirs, all_data_dirs, cache_home, config_dirs, config_home, data_dirs, data_home,
    home_dir, runtime_dir, state_home,
};
pub use clipboard::FileClipboard;
pub use desktop::{
    applications_for_mime, default_application_for_mime, discover_applications, open_path,
    parse_desktop_file, DesktopEntry,
};
pub use md5::md5_hex;
pub use mime::{default_database, guess_mime_type, MimeDatabase};
pub use trash::{
    empty_trash, home_trash_root, list_trash, list_trash_in_root, purge_trash_item,
    restore_trash_item, trash_paths, trash_root, trash_root_for, TrashItem,
};
pub use uri::{decode_file_uri, encode_file_uri};
pub use user_dirs::{load_user_dirs, user_dir, UserDirKind};
