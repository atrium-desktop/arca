//! Sidebar bookmarks: the home directory, the XDG user directories
//! (Desktop, Downloads, …) and user-pinned folders.
//!
//! XDG dirs are discovered from `~/.config/user-dirs.dirs` when present,
//! falling back to probing the well-known names under `$HOME`.

use crate::path;

/// Which fixed icon/label a bookmark maps to in the UI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BookmarkKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    /// User-pinned or otherwise unrecognized folder.
    Folder,
}

impl BookmarkKind {
    /// Classify a directory by its file name.
    pub fn from_path(path: &str) -> BookmarkKind {
        if path == path::home_dir() {
            return BookmarkKind::Home;
        }
        let name = path.rsplit('/').next().unwrap_or("");
        match name {
            "Desktop" => BookmarkKind::Desktop,
            "Documents" => BookmarkKind::Documents,
            "Downloads" => BookmarkKind::Downloads,
            "Music" => BookmarkKind::Music,
            "Pictures" => BookmarkKind::Pictures,
            "Videos" => BookmarkKind::Videos,
            _ => BookmarkKind::Folder,
        }
    }
}

/// One sidebar entry.
#[derive(Clone, Debug)]
pub struct Bookmark {
    pub name: String,
    pub path: String,
    pub kind: BookmarkKind,
    /// XDG/home entries are fixed; user bookmarks can be removed.
    pub pinned: bool,
}

impl Bookmark {
    pub fn for_path(path: &str) -> Bookmark {
        Self::for_path_with_name(path, None)
    }

    pub fn for_path_with_name(path: &str, custom_name: Option<String>) -> Bookmark {
        let kind = BookmarkKind::from_path(path);
        let name = custom_name.unwrap_or_else(|| {
            if kind == BookmarkKind::Home {
                "Home".to_string()
            } else {
                path.rsplit('/').next().unwrap_or(path).to_string()
            }
        });
        Bookmark {
            name,
            path: path.to_string(),
            kind,
            pinned: true,
        }
    }
}

/// Decode a `file://...` URI with percent-encoding into a local file path.
pub fn decode_file_uri(uri: &str) -> Option<String> {
    let raw = uri.strip_prefix("file://")?;
    let mut bytes = Vec::new();
    let chars = raw.as_bytes();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == b'%' && i + 2 < chars.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&chars[i + 1..i + 3]).unwrap_or(""), 16) {
                bytes.push(byte);
                i += 3;
                continue;
            }
        }
        bytes.push(chars[i]);
        i += 1;
    }
    String::from_utf8(bytes).ok()
}

/// Encode a local file path into a standard `file://...` URI.
pub fn encode_file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

/// Parse one line of `~/.config/gtk-3.0/bookmarks` (Freedesktop/XDG standard).
pub fn parse_gtk_bookmark_line(line: &str) -> Option<Bookmark> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (uri, name) = match line.split_once(' ') {
        Some((u, n)) => (u.trim(), Some(n.trim().to_string())),
        None => (line, None),
    };
    let path = decode_file_uri(uri)?;
    Some(Bookmark::for_path_with_name(&path, name))
}

/// Format one `Bookmark` into a standard `~/.config/gtk-3.0/bookmarks` line.
pub fn format_gtk_bookmark_line(bookmark: &Bookmark) -> String {
    let uri = encode_file_uri(&bookmark.path);
    let default_name = bookmark.path.rsplit('/').next().unwrap_or(&bookmark.path);
    if !bookmark.name.is_empty() && bookmark.name != default_name {
        format!("{uri} {}", bookmark.name)
    } else {
        uri
    }
}

/// Load standard XDG/GTK user bookmarks from `$XDG_CONFIG_HOME/gtk-3.0/bookmarks`.
pub fn read_gtk_bookmarks_file(home: &str) -> Vec<Bookmark> {
    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| path::join(home, ".config"));
    let file = path::join(&config_home, "gtk-3.0/bookmarks");
    let Ok(content) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(parse_gtk_bookmark_line)
        .filter(|b| std::path::Path::new(&b.path).is_dir())
        .collect()
}

/// Write pinned user bookmarks out to `$XDG_CONFIG_HOME/gtk-3.0/bookmarks`.
pub fn save_gtk_bookmarks_file(home: &str, bookmarks: &[Bookmark]) -> std::io::Result<()> {
    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| path::join(home, ".config"));
    let dir = path::join(&config_home, "gtk-3.0");
    std::fs::create_dir_all(&dir)?;
    let file = path::join(&dir, "bookmarks");
    let mut content = String::new();
    for bookmark in bookmarks {
        if bookmark.pinned && bookmark.kind == BookmarkKind::Folder {
            content.push_str(&format_gtk_bookmark_line(bookmark));
            content.push('\n');
        }
    }
    std::fs::write(file, content)
}

/// The fixed bookmarks: home + the XDG user dirs that actually exist.
pub fn standard_bookmarks() -> Vec<Bookmark> {
    let home = path::home_dir();
    let mut out = vec![Bookmark {
        name: "Home".into(),
        path: home.clone(),
        kind: BookmarkKind::Home,
        pinned: true,
    }];

    for (path, kind) in xdg_user_dirs(&home) {
        out.push(Bookmark {
            name: format!("{kind:?}"),
            path,
            kind,
            pinned: true,
        });
    }
    out
}

/// XDG user dirs in the canonical order, restricted to the kinds Arca
/// shows. Reads `$XDG_CONFIG_HOME/user-dirs.dirs`; falls back to probing
/// the English names under `home`.
fn xdg_user_dirs(home: &str) -> Vec<(String, BookmarkKind)> {
    const ORDER: [(&str, &str, BookmarkKind); 6] = [
        ("XDG_DESKTOP_DIR", "Desktop", BookmarkKind::Desktop),
        ("XDG_DOCUMENTS_DIR", "Documents", BookmarkKind::Documents),
        ("XDG_DOWNLOAD_DIR", "Downloads", BookmarkKind::Downloads),
        ("XDG_MUSIC_DIR", "Music", BookmarkKind::Music),
        ("XDG_PICTURES_DIR", "Pictures", BookmarkKind::Pictures),
        ("XDG_VIDEOS_DIR", "Videos", BookmarkKind::Videos),
    ];

    let configured = read_user_dirs_file(home);
    let mut out = Vec::new();
    for (var, fallback_name, kind) in ORDER {
        let dir = configured
            .iter()
            .find(|(key, _)| key == var)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| path::join(home, fallback_name));
        if dir != home && std::path::Path::new(&dir).is_dir() && !out.iter().any(|(p, _)| *p == dir)
        {
            out.push((dir, kind));
        }
    }
    out
}

/// Parse `user-dirs.dirs` lines: `XDG_DESKTOP_DIR="$HOME/Desktop"`.
fn read_user_dirs_file(home: &str) -> Vec<(String, String)> {
    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| path::join(home, ".config"));
    let file = path::join(&config_home, "user-dirs.dirs");
    let Ok(content) = std::fs::read_to_string(file) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let value = if let Some(rest) = value.strip_prefix("$HOME/") {
            path::join(home, rest)
        } else if value == "$HOME" {
            home.to_string()
        } else {
            value.to_string()
        };
        out.push((key.trim().to_string(), value));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_classification() {
        let home = path::home_dir();
        assert_eq!(BookmarkKind::from_path(&home), BookmarkKind::Home);
        assert_eq!(
            BookmarkKind::from_path(&path::join(&home, "Downloads")),
            BookmarkKind::Downloads
        );
        assert_eq!(BookmarkKind::from_path("/var/log"), BookmarkKind::Folder);
    }

    #[test]
    fn bookmark_for_path_names() {
        let home = path::home_dir();
        assert_eq!(Bookmark::for_path(&home).name, "Home");
        assert_eq!(Bookmark::for_path("/usr/share").name, "share");
    }

    #[test]
    fn parse_user_dirs_lines() {
        let home = path::home_dir();
        // read_user_dirs_file reads the real user config; just ensure it
        // never panics and returns absolute paths when present.
        for (key, value) in read_user_dirs_file(&home) {
            assert!(key.starts_with("XDG_"));
            assert!(value.starts_with('/'));
        }
    }

    #[test]
    fn standard_bookmarks_starts_with_home() {
        let marks = standard_bookmarks();
        assert_eq!(marks[0].kind, BookmarkKind::Home);
        assert!(marks[0].pinned);
        for mark in &marks {
            assert!(std::path::Path::new(&mark.path).is_dir());
        }
    }

    #[test]
    fn gtk_bookmarks_roundtrip_and_percent_decoding() {
        let line = "file:///home/user/My%20Documents Important Docs";
        let mark = parse_gtk_bookmark_line(line).expect("parse gtk bookmark");
        assert_eq!(mark.path, "/home/user/My Documents");
        assert_eq!(mark.name, "Important Docs");
        assert_eq!(mark.kind, BookmarkKind::Folder);

        let formatted = format_gtk_bookmark_line(&mark);
        assert_eq!(formatted, "file:///home/user/My%20Documents Important Docs");

        let simple_line = "file:///home/user/music";
        let simple_mark = parse_gtk_bookmark_line(simple_line).expect("parse simple mark");
        assert_eq!(simple_mark.path, "/home/user/music");
        assert_eq!(simple_mark.name, "music");
        assert_eq!(format_gtk_bookmark_line(&simple_mark), "file:///home/user/music");
    }
}
