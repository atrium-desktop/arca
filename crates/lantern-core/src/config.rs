//! Persistent configuration.
//!
//! Stored as a minimal `key = value` file at
//! `$XDG_CONFIG_HOME/lantern/lantern.conf` (usually
//! `~/.config/lantern/lantern.conf`). Hand-rolled on purpose — the format
//! is trivial and Lantern stays dependency-free.

use std::io;

use crate::entry::SortKey;
use crate::path;

/// Content presentation used by the active browser tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    /// Large square file tiles with names below their icons.
    Grid,
    /// Dense rows with sortable metadata columns.
    #[default]
    List,
    /// Hierarchical side-by-side directory columns.
    Miller,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::Grid => "grid",
            ViewMode::List => "list",
            ViewMode::Miller => "miller",
        }
    }

    fn from_str(value: &str) -> ViewMode {
        match value {
            "grid" => ViewMode::Grid,
            "miller" => ViewMode::Miller,
            _ => ViewMode::List,
        }
    }
}

/// Colour-scheme preference.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThemeMode {
    /// Follow the desktop (default).
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    fn from_str(s: &str) -> ThemeMode {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

/// Everything remembered between runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_ascending: bool,
    pub theme: ThemeMode,
    pub view_mode: ViewMode,
    /// User-pinned bookmark paths (fixed home/XDG bookmarks are not stored).
    pub bookmarks: Vec<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            show_hidden: false,
            sort_key: SortKey::Name,
            sort_ascending: true,
            theme: ThemeMode::System,
            view_mode: ViewMode::List,
            bookmarks: Vec::new(),
        }
    }
}

/// The configuration file path: `$XDG_CONFIG_HOME/lantern/lantern.conf`.
pub fn config_path() -> String {
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| path::join(&path::home_dir(), ".config"));
    path::join(&path::join(&base, "lantern"), "lantern.conf")
}

/// Load `file`; missing file or unknown keys fall back to defaults.
pub fn load(file: &str) -> Config {
    let mut cfg = Config::default();
    let Ok(content) = std::fs::read_to_string(file) else {
        return cfg;
    };
    cfg.apply_str(&content);
    cfg
}

/// Write `cfg` to `file`, creating the parent directory. Writes via a temp
/// file + rename so a crash can't leave a truncated config.
pub fn save(file: &str, cfg: &Config) -> io::Result<()> {
    if let Some(parent) = std::path::Path::new(file).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = format!("{file}.tmp");
    std::fs::write(&tmp, cfg.to_str())?;
    std::fs::rename(&tmp, file)
}

impl Config {
    fn apply_str(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "show_hidden" => self.show_hidden = value == "true",
                "sort" => {
                    if let Some(k) = SortKey::parse(value) {
                        self.sort_key = k;
                    }
                }
                "sort_ascending" => self.sort_ascending = value == "true",
                "theme" => self.theme = ThemeMode::from_str(value),
                "view" => self.view_mode = ViewMode::from_str(value),
                "bookmark" if !value.is_empty() && !self.bookmarks.iter().any(|b| b == value) => {
                    self.bookmarks.push(value.to_string());
                }
                _ => {} // forward-compatible: ignore unknown keys
            }
        }
    }

    fn to_str(&self) -> String {
        let mut out = String::from("# Lantern configuration\n");
        out.push_str(&format!("show_hidden = {}\n", self.show_hidden));
        out.push_str(&format!("sort = {}\n", self.sort_key.as_str()));
        out.push_str(&format!("sort_ascending = {}\n", self.sort_ascending));
        out.push_str(&format!("theme = {}\n", self.theme.as_str()));
        out.push_str(&format!("view = {}\n", self.view_mode.as_str()));
        for bookmark in &self.bookmarks {
            out.push_str(&format!("bookmark = {bookmark}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> String {
        let dir = std::env::temp_dir().join(format!(
            "lantern-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path::join(dir.to_str().unwrap(), "lantern.conf")
    }

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert!(!cfg.show_hidden);
        assert_eq!(cfg.sort_key, SortKey::Name);
        assert!(cfg.sort_ascending);
        assert_eq!(cfg.theme, ThemeMode::System);
        assert_eq!(cfg.view_mode, ViewMode::List);
        assert!(cfg.bookmarks.is_empty());
    }

    #[test]
    fn roundtrip() {
        let file = temp_file();
        let cfg = Config {
            show_hidden: true,
            sort_key: SortKey::Mtime,
            sort_ascending: false,
            theme: ThemeMode::Dark,
            view_mode: ViewMode::Miller,
            bookmarks: vec!["/tmp".into(), "/var/log".into()],
        };
        save(&file, &cfg).unwrap();
        let back = load(&file);
        assert!(back.show_hidden);
        assert_eq!(back.sort_key, SortKey::Mtime);
        assert!(!back.sort_ascending);
        assert_eq!(back.theme, ThemeMode::Dark);
        assert_eq!(back.view_mode, ViewMode::Miller);
        assert_eq!(back.bookmarks, ["/tmp", "/var/log"]);
        std::fs::remove_dir_all(std::path::Path::new(&file).parent().unwrap()).ok();
    }

    #[test]
    fn unknown_keys_and_duplicates_are_tolerated() {
        let mut cfg = Config::default();
        cfg.apply_str("future_key = 1\nbookmark = /a\nbookmark = /a\n# comment\nbogus\n");
        assert_eq!(cfg.bookmarks, ["/a"]);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = load("/nonexistent/lantern.conf");
        assert_eq!(cfg, Config::default());
    }
}
