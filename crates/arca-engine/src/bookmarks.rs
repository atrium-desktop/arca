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
    Trash,
    /// User-pinned or otherwise unrecognized folder.
    Folder,
}

impl BookmarkKind {
    /// Classify a directory by its file name.
    pub fn from_path(path: &str) -> BookmarkKind {
        if path == path::home_dir() {
            return BookmarkKind::Home;
        }
        if path.ends_with("/Trash") || path.ends_with("/Trash/files") || path == "trash:///" {
            return BookmarkKind::Trash;
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
    crate::xdg::decode_file_uri(uri).map(|p| p.to_string_lossy().into_owned())
}

/// Encode a local file path into a standard `file://...` URI.
pub fn encode_file_uri(path: &str) -> String {
    crate::xdg::encode_file_uri(std::path::Path::new(path))
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
pub fn read_gtk_bookmarks_file(_home: &str) -> Vec<Bookmark> {
    let file = crate::xdg::config_home().join("gtk-3.0/bookmarks");
    let Ok(content) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(parse_gtk_bookmark_line)
        .filter(|b| std::path::Path::new(&b.path).is_dir())
        .collect()
}

/// Write pinned user bookmarks out to `$XDG_CONFIG_HOME/gtk-3.0/bookmarks`.
pub fn save_gtk_bookmarks_file(_home: &str, bookmarks: &[Bookmark]) -> std::io::Result<()> {
    let dir = crate::xdg::config_home().join("gtk-3.0");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("bookmarks");
    let mut content = String::new();
    for bookmark in bookmarks {
        if bookmark.pinned && bookmark.kind == BookmarkKind::Folder {
            content.push_str(&format_gtk_bookmark_line(bookmark));
            content.push('\n');
        }
    }
    std::fs::write(file, content)
}

/// The fixed bookmarks: home + the XDG user dirs that actually exist + Trash.
pub fn standard_bookmarks() -> Vec<Bookmark> {
    let home = path::home_dir();
    let mut out = vec![Bookmark {
        name: "Home".into(),
        path: home.clone(),
        kind: BookmarkKind::Home,
        pinned: true,
    }];

    const USER_DIRS: [(crate::xdg::UserDirKind, BookmarkKind); 6] = [
        (crate::xdg::UserDirKind::Desktop, BookmarkKind::Desktop),
        (crate::xdg::UserDirKind::Documents, BookmarkKind::Documents),
        (crate::xdg::UserDirKind::Download, BookmarkKind::Downloads),
        (crate::xdg::UserDirKind::Music, BookmarkKind::Music),
        (crate::xdg::UserDirKind::Pictures, BookmarkKind::Pictures),
        (crate::xdg::UserDirKind::Videos, BookmarkKind::Videos),
    ];

    for (user_dir_kind, bookmark_kind) in USER_DIRS {
        if let Some(dir_path) = crate::xdg::user_dir(user_dir_kind) {
            let path_str = dir_path.to_string_lossy().into_owned();
            if path_str != home && !out.iter().any(|b| b.path == path_str) {
                out.push(Bookmark {
                    name: format!("{bookmark_kind:?}"),
                    path: path_str,
                    kind: bookmark_kind,
                    pinned: true,
                });
            }
        }
    }

    let trash_path = crate::trash::home_trash_root().to_string_lossy().into_owned();
    out.push(Bookmark {
        name: "Trash".into(),
        path: trash_path,
        kind: BookmarkKind::Trash,
        pinned: true,
    });
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
        // load_user_dirs reads the user config; ensure it returns valid absolute paths.
        for (_kind, path) in crate::xdg::load_user_dirs() {
            assert!(path.is_absolute());
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
