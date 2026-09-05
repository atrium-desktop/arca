//! XDG Desktop Entry Specification and MIME Applications Association (`mimeapps.list`).
//!
//! Discovers `.desktop` files, parses desktop entries, resolves associations
//! from `mimeapps.list`, formats `Exec` field codes, and spawns applications.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::base::{config_dirs, config_home, data_dirs, data_home};
use crate::mime::guess_mime_type;
use crate::uri::encode_file_uri;

/// A parsed `.desktop` application entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub mime_types: Vec<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
}

impl DesktopEntry {
    /// Format command-line arguments for opening `paths` with this application,
    /// properly expanding Desktop Entry specification field codes (`%f`, `%F`, `%u`, `%U`, etc.).
    #[must_use]
    pub fn format_exec(&self, paths: &[&Path]) -> Vec<String> {
        let mut result = Vec::new();
        let tokens = tokenize_exec(&self.exec);

        let first_path = paths.first().map(|p| p.to_string_lossy().into_owned());
        let all_paths: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let first_uri = paths.first().map(|p| encode_file_uri(p));
        let all_uris: Vec<String> = paths.iter().map(|p| encode_file_uri(p)).collect();

        let mut had_target_specifier = false;

        for token in tokens {
            match token.as_str() {
                "%f" => {
                    had_target_specifier = true;
                    if let Some(p) = &first_path {
                        result.push(p.clone());
                    }
                }
                "%F" => {
                    had_target_specifier = true;
                    result.extend(all_paths.clone());
                }
                "%u" => {
                    had_target_specifier = true;
                    if let Some(u) = &first_uri {
                        result.push(u.clone());
                    }
                }
                "%U" => {
                    had_target_specifier = true;
                    result.extend(all_uris.clone());
                }
                "%i" => {
                    if let Some(icon) = &self.icon {
                        result.push("--icon".into());
                        result.push(icon.clone());
                    }
                }
                "%c" => {
                    result.push(self.name.clone());
                }
                "%k" => {
                    result.push(self.path.to_string_lossy().into_owned());
                }
                t if t.starts_with('%') && t.len() == 2 => {}
                t => result.push(t.to_string()),
            }
        }

        if !had_target_specifier && !paths.is_empty() {
            result.extend(all_paths);
        }

        result
    }

    /// Spawn this application detached with given paths.
    pub fn spawn(&self, paths: &[&Path]) -> std::io::Result<()> {
        let args = self.format_exec(paths);
        if args.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "empty exec command",
            ));
        }

        if self.terminal {
            let term = std::env::var("TERMINAL").unwrap_or_else(|_| "foot".into());
            let mut cmd = Command::new(term);
            cmd.arg("-e");
            cmd.args(&args);
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
        } else {
            let mut cmd = Command::new(&args[0]);
            if args.len() > 1 {
                cmd.args(&args[1..]);
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
        }
        Ok(())
    }
}

fn tokenize_exec(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in exec.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && !in_single {
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if ch.is_whitespace() && !in_single && !in_double {
            if !current.is_empty() {
                tokens.push(current);
                current = String::new();
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parse a single `.desktop` file.
#[must_use]
pub fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let id = path.file_name()?.to_string_lossy().into_owned();

    let mut in_desktop_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut mime_types = Vec::new();
    let mut terminal = false;
    let mut no_display = false;
    let mut hidden = false;
    let mut entry_type = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();

        match key {
            "Type" => entry_type = Some(val.to_string()),
            "Name" if name.is_none() => name = Some(val.to_string()),
            "Exec" => exec = Some(val.to_string()),
            "Icon" => icon = Some(val.to_string()),
            "Terminal" => terminal = val == "true",
            "NoDisplay" => no_display = val == "true",
            "Hidden" => hidden = val == "true",
            "MimeType" => {
                mime_types = val
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
            }
            _ => {}
        }
    }

    if entry_type.as_deref() != Some("Application") {
        return None;
    }
    let name = name?;
    let exec = exec?;

    Some(DesktopEntry {
        id,
        path: path.to_path_buf(),
        name,
        exec,
        icon,
        mime_types,
        terminal,
        no_display,
        hidden,
    })
}

/// Discover all valid, non-hidden desktop applications in the system.
#[must_use]
pub fn discover_applications() -> HashMap<String, DesktopEntry> {
    let mut apps = HashMap::new();

    let mut search_dirs = Vec::new();
    for data_dir in data_dirs() {
        search_dirs.push(data_dir.join("applications"));
    }
    search_dirs.push(data_home().join("applications"));

    for dir in search_dirs {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|ext| ext == "desktop") {
                if let Some(app) = parse_desktop_file(&p) {
                    if !app.hidden {
                        apps.insert(app.id.clone(), app);
                    }
                }
            }
        }
    }
    apps
}

/// Find the preferred default application for `mime_type` based on `mimeapps.list`.
#[must_use]
pub fn default_application_for_mime(mime_type: &str) -> Option<DesktopEntry> {
    let apps = discover_applications();
    let associations = load_mimeapps_list();

    if let Some(default_ids) = associations.get(mime_type) {
        for id in default_ids {
            if let Some(app) = apps.get(id) {
                return Some(app.clone());
            }
        }
    }

    for app in apps.values() {
        if app.mime_types.iter().any(|m| m == mime_type) && !app.no_display {
            return Some(app.clone());
        }
    }

    None
}

/// Return all applications capable of opening `mime_type`.
#[must_use]
pub fn applications_for_mime(mime_type: &str) -> Vec<DesktopEntry> {
    let apps = discover_applications();
    let mut matches = Vec::new();
    let mut seen_ids = HashSet::new();

    let associations = load_mimeapps_list();
    if let Some(ids) = associations.get(mime_type) {
        for id in ids {
            if let Some(app) = apps.get(id) {
                if seen_ids.insert(app.id.clone()) && !app.no_display {
                    matches.push(app.clone());
                }
            }
        }
    }

    for app in apps.values() {
        if app.mime_types.iter().any(|m| m == mime_type)
            && seen_ids.insert(app.id.clone())
            && !app.no_display
        {
            matches.push(app.clone());
        }
    }

    matches
}

/// Open a path with the desktop default application, or fall back to `xdg-open`.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    let mime_type = guess_mime_type(path);
    if let Some(app) = default_application_for_mime(&mime_type) {
        if app.spawn(&[path]).is_ok() {
            return Ok(());
        }
    }

    Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

fn load_mimeapps_list() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    let mut files = Vec::new();
    files.push(config_home().join("mimeapps.list"));
    for dir in config_dirs() {
        files.push(dir.join("mimeapps.list"));
    }
    files.push(data_home().join("applications/mimeapps.list"));
    for dir in data_dirs() {
        files.push(dir.join("applications/mimeapps.list"));
    }

    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut in_defaults = false;
        let mut in_added = false;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if line == "[Default Applications]" {
                in_defaults = true;
                in_added = false;
                continue;
            } else if line == "[Added Associations]" {
                in_defaults = false;
                in_added = true;
                continue;
            } else if line.starts_with('[') {
                in_defaults = false;
                in_added = false;
                continue;
            }

            if in_defaults || in_added {
                if let Some((m, apps_str)) = line.split_once('=') {
                    let m = m.trim().to_string();
                    let list = map.entry(m).or_default();
                    for app_id in apps_str.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                        if !list.iter().any(|existing| existing == app_id) {
                            list.push(app_id.to_string());
                        }
                    }
                }
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_field_code_expansion() {
        let entry = DesktopEntry {
            id: "editor.desktop".into(),
            path: PathBuf::from("/usr/share/applications/editor.desktop"),
            name: "Text Editor".into(),
            exec: "myeditor --title %c %F".into(),
            icon: Some("editor-icon".into()),
            mime_types: vec!["text/plain".into()],
            terminal: false,
            no_display: false,
            hidden: false,
        };

        let file1 = Path::new("/home/user/test1.txt");
        let file2 = Path::new("/home/user/test2.txt");
        let args = entry.format_exec(&[file1, file2]);

        assert_eq!(
            args,
            vec![
                "myeditor",
                "--title",
                "Text Editor",
                "/home/user/test1.txt",
                "/home/user/test2.txt"
            ]
        );
    }
}
