//! XDG User Directories Specification (`xdg-user-dirs`).
//!
//! Provides resolution for standard user directories defined by freedesktop.org.

use std::path::PathBuf;

use crate::base::{config_home, home_dir};

/// Standard user directories defined in the XDG user-dirs specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserDirKind {
    Desktop,
    Documents,
    Download,
    Music,
    Pictures,
    Videos,
    Templates,
    PublicShare,
}

impl UserDirKind {
    #[must_use]
    pub fn var_name(self) -> &'static str {
        match self {
            UserDirKind::Desktop => "XDG_DESKTOP_DIR",
            UserDirKind::Documents => "XDG_DOCUMENTS_DIR",
            UserDirKind::Download => "XDG_DOWNLOAD_DIR",
            UserDirKind::Music => "XDG_MUSIC_DIR",
            UserDirKind::Pictures => "XDG_PICTURES_DIR",
            UserDirKind::Videos => "XDG_VIDEOS_DIR",
            UserDirKind::Templates => "XDG_TEMPLATES_DIR",
            UserDirKind::PublicShare => "XDG_PUBLICSHARE_DIR",
        }
    }

    #[must_use]
    pub fn default_name(self) -> &'static str {
        match self {
            UserDirKind::Desktop => "Desktop",
            UserDirKind::Documents => "Documents",
            UserDirKind::Download => "Downloads",
            UserDirKind::Music => "Music",
            UserDirKind::Pictures => "Pictures",
            UserDirKind::Videos => "Videos",
            UserDirKind::Templates => "Templates",
            UserDirKind::PublicShare => "Public",
        }
    }

    pub const ALL: &'static [UserDirKind] = &[
        UserDirKind::Desktop,
        UserDirKind::Documents,
        UserDirKind::Download,
        UserDirKind::Music,
        UserDirKind::Pictures,
        UserDirKind::Videos,
        UserDirKind::Templates,
        UserDirKind::PublicShare,
    ];
}

/// Resolve the filesystem path for a given user directory kind.
#[must_use]
pub fn user_dir(kind: UserDirKind) -> Option<PathBuf> {
    let home = home_dir();
    let user_dirs = load_user_dirs();
    if let Some(path) = user_dirs.iter().find(|(k, _)| *k == kind).map(|(_, p)| p) {
        if path.is_dir() && path != &home {
            return Some(path.clone());
        }
    }
    // Fallback: $HOME/<DefaultName>
    let fallback = home.join(kind.default_name());
    if fallback.is_dir() {
        Some(fallback)
    } else {
        None
    }
}

/// Load and parse `$XDG_CONFIG_HOME/user-dirs.dirs`.
#[must_use]
pub fn load_user_dirs() -> Vec<(UserDirKind, PathBuf)> {
    let home = home_dir();
    let home_str = home.to_string_lossy();
    let config_file = config_home().join("user-dirs.dirs");
    let content = match std::fs::read_to_string(&config_file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim().trim_matches('"');
        let resolved = if let Some(rest) = val.strip_prefix("$HOME/") {
            home.join(rest)
        } else if val == "$HOME" {
            home.clone()
        } else if let Some(rest) = val.strip_prefix(&format!("{home_str}/")) {
            home.join(rest)
        } else {
            PathBuf::from(val)
        };

        for &kind in UserDirKind::ALL {
            if kind.var_name() == key {
                out.push((kind, resolved));
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_dirs_parsing() {
        for (_kind, path) in load_user_dirs() {
            assert!(path.is_absolute());
        }
    }
}
