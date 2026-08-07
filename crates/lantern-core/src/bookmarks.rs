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
        let kind = BookmarkKind::from_path(path);
        let name = if kind == BookmarkKind::Home {
            "Home".to_string()
        } else {
            path.rsplit('/').next().unwrap_or(path).to_string()
        };
        Bookmark {
            name,
            path: path.to_string(),
            kind,
            pinned: true,
        }
    }
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

/// XDG user dirs in the canonical order, restricted to the kinds Lantern
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
}
