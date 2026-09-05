//! XDG Base Directory Specification implementation.
//!
//! Provides validated, absolute paths for all XDG environment variables.
//! Strict adherence to specification: empty or relative variables are treated as unset.

use std::env;
use std::path::PathBuf;

/// Return environment variable if set, non-empty, and containing an absolute path.
/// The XDG specification explicitly dictates:
/// "All paths set in these environment variables must be absolute. If an implementation
/// encounters a relative path it should consider the path invalid and treat it as not set."
fn non_empty_abs_env(var: &str) -> Option<PathBuf> {
    env::var_os(var).and_then(|v| {
        let p = PathBuf::from(v);
        if p.is_absolute() {
            Some(p)
        } else {
            None
        }
    })
}

/// Fallback home directory (`$HOME` or `/`).
#[must_use]
pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `$XDG_DATA_HOME` or `$HOME/.local/share`.
#[must_use]
pub fn data_home() -> PathBuf {
    non_empty_abs_env("XDG_DATA_HOME").unwrap_or_else(|| home_dir().join(".local/share"))
}

/// `$XDG_CONFIG_HOME` or `$HOME/.config`.
#[must_use]
pub fn config_home() -> PathBuf {
    non_empty_abs_env("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config"))
}

/// `$XDG_CACHE_HOME` or `$HOME/.cache`.
#[must_use]
pub fn cache_home() -> PathBuf {
    non_empty_abs_env("XDG_CACHE_HOME").unwrap_or_else(|| home_dir().join(".cache"))
}

/// `$XDG_STATE_HOME` or `$HOME/.local/state`.
#[must_use]
pub fn state_home() -> PathBuf {
    non_empty_abs_env("XDG_STATE_HOME").unwrap_or_else(|| home_dir().join(".local/state"))
}

/// `$XDG_RUNTIME_DIR` (must be absolute, or None if not set).
#[must_use]
pub fn runtime_dir() -> Option<PathBuf> {
    non_empty_abs_env("XDG_RUNTIME_DIR")
}

/// Preference-ordered list of base directories to search for data files.
/// Defaults to `["/usr/local/share", "/usr/share"]`.
#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    if let Some(val) = env::var_os("XDG_DATA_DIRS") {
        let dirs: Vec<PathBuf> = env::split_paths(&val)
            .filter(|p| p.is_absolute())
            .collect();
        if !dirs.is_empty() {
            return dirs;
        }
    }
    vec![
        PathBuf::from("/usr/local/share"),
        PathBuf::from("/usr/share"),
    ]
}

/// Full data search hierarchy: `$XDG_DATA_HOME` followed by `$XDG_DATA_DIRS`.
#[must_use]
pub fn all_data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![data_home()];
    dirs.extend(data_dirs());
    dirs
}

/// Preference-ordered list of base directories to search for configuration files.
/// Defaults to `["/etc/xdg"]`.
#[must_use]
pub fn config_dirs() -> Vec<PathBuf> {
    if let Some(val) = env::var_os("XDG_CONFIG_DIRS") {
        let dirs: Vec<PathBuf> = env::split_paths(&val)
            .filter(|p| p.is_absolute())
            .collect();
        if !dirs.is_empty() {
            return dirs;
        }
    }
    vec![PathBuf::from("/etc/xdg")]
}

/// Full config search hierarchy: `$XDG_CONFIG_HOME` followed by `$XDG_CONFIG_DIRS`.
#[must_use]
pub fn all_config_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![config_home()];
    dirs.extend(config_dirs());
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_directory_defaults() {
        assert!(data_home().is_absolute());
        assert!(config_home().is_absolute());
        assert!(cache_home().is_absolute());
        assert!(state_home().is_absolute());
        assert!(!data_dirs().is_empty());
        assert!(!config_dirs().is_empty());
    }
}
