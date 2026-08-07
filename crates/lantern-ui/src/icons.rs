//! Icon glyphs for files, folders and chrome.
//!
//! The safe `lens::Icon` enum only covers a subset of the C icon set, so
//! this module works with `lens_sys::lens_icon_id` directly (the full
//! feather set from `lens/icon.h`) and wraps the raw widget calls.

use std::ffi::CString;

use iris::Frame;
use lantern_core::bookmarks::BookmarkKind;
use lantern_core::entry::{Entry, FileType};

pub use lens_sys::lens_icon_id as ids;

pub type IconId = lens_sys::lens_icon_id;

pub fn icon(f: &mut Frame, id: IconId, size: f32) {
    // SAFETY: the frame is live inside a build callback.
    unsafe { lens_sys::lens_icon(f.as_raw(), id, size) }
}

pub fn icon_button(f: &mut Frame, id: IconId) -> bool {
    // SAFETY: the frame is live inside a build callback.
    unsafe { lens_sys::lens_icon_button(f.as_raw(), id) }
}

pub fn icon_button_active(f: &mut Frame, id: IconId, active: bool) -> bool {
    // SAFETY: the frame is live inside a build callback.
    unsafe { lens_sys::lens_icon_button_active(f.as_raw(), id, active) }
}

/// Icon button that swaps glyph with its checked state (eye/eye-off, star…).
pub fn icon_toggle_button(
    f: &mut Frame,
    unchecked: IconId,
    checked: IconId,
    size: f32,
    is_checked: bool,
) -> bool {
    // SAFETY: the frame is live inside a build callback.
    unsafe { lens_sys::lens_icon_toggle_button(f.as_raw(), unchecked, checked, size, is_checked) }
}

/// Full-width sidebar row with a leading icon.
pub fn selectable_icon(f: &mut Frame, id: IconId, label: &str, selected: bool) -> bool {
    let label = CString::new(label).expect("file names cannot contain NUL on Unix");
    // SAFETY: the frame is live; the label outlives the call.
    unsafe { lens_sys::lens_selectable_icon(f.as_raw(), id, label.as_ptr(), selected) }
}

/// Icon for a directory entry (type glyph + extension mapping).
pub fn entry_icon(e: &Entry) -> IconId {
    match e.file_type {
        FileType::Directory => dir_icon(&e.name),
        FileType::Symlink => {
            if e.navigable {
                dir_icon(&e.name)
            } else {
                ids::LENS_ICON_LINK_2
            }
        }
        _ => file_icon(&e.name),
    }
}

pub fn bookmark_icon(kind: BookmarkKind) -> IconId {
    match kind {
        BookmarkKind::Home => ids::LENS_ICON_HOME,
        BookmarkKind::Desktop => ids::LENS_ICON_MONITOR,
        BookmarkKind::Documents => ids::LENS_ICON_FILE_TEXT,
        BookmarkKind::Downloads => ids::LENS_ICON_DOWNLOAD,
        BookmarkKind::Music => ids::LENS_ICON_MUSIC,
        BookmarkKind::Pictures => ids::LENS_ICON_IMAGE,
        BookmarkKind::Videos => ids::LENS_ICON_FILM,
        BookmarkKind::Folder => ids::LENS_ICON_FOLDER,
    }
}

fn file_icon(name: &str) -> IconId {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let icon = match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => ids::LENS_ICON_IMAGE,
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" => ids::LENS_ICON_MUSIC,
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" => ids::LENS_ICON_FILM,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => ids::LENS_ICON_ARCHIVE,
        "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "rs" | "go" | "java" => {
            ids::LENS_ICON_CODE
        }
        "sh" | "bash" | "zsh" | "fish" => ids::LENS_ICON_TERMINAL,
        "txt" | "md" | "org" | "rst" | "pdf" | "doc" | "docx" | "json" | "xml" | "yaml" | "yml"
        | "toml" | "ini" | "cfg" | "conf" | "log" => ids::LENS_ICON_FILE_TEXT,
        _ => ids::LENS_ICON_FILE,
    };
    // No extension (or the extension is the whole name, e.g. ".gitignore").
    if ext == lower {
        ids::LENS_ICON_FILE
    } else {
        icon
    }
}

fn dir_icon(name: &str) -> IconId {
    match name {
        "Desktop" => ids::LENS_ICON_MONITOR,
        "Documents" => ids::LENS_ICON_FILE_TEXT,
        "Downloads" => ids::LENS_ICON_DOWNLOAD,
        "Music" => ids::LENS_ICON_MUSIC,
        "Pictures" => ids::LENS_ICON_IMAGE,
        "Videos" => ids::LENS_ICON_FILM,
        ".git" => ids::LENS_ICON_GIT_BRANCH,
        ".config" => ids::LENS_ICON_SETTINGS,
        _ => ids::LENS_ICON_FOLDER,
    }
}
