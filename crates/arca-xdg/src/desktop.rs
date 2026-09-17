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

    /// Spawn this application detached with given paths and optional XDG activation token.
    pub fn spawn_with_token(&self, paths: &[&Path], activation_token: Option<&str>) -> std::io::Result<()> {
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
            if let Some(token) = activation_token {
                cmd.env("XDG_ACTIVATION_TOKEN", token);
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
        } else {
            let mut cmd = Command::new(&args[0]);
            if args.len() > 1 {
                cmd.args(&args[1..]);
            }
            if let Some(token) = activation_token {
                cmd.env("XDG_ACTIVATION_TOKEN", token);
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
        }
        Ok(())
    }

    /// Spawn this application detached with given paths.
    pub fn spawn(&self, paths: &[&Path]) -> std::io::Result<()> {
        self.spawn_with_token(paths, None)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MimeappsSection {
    Defaults,
    Added,
    Removed,
}

/// Parsed associations from XDG `mimeapps.list` files.
#[derive(Debug, Default, Clone)]
pub struct MimeAssociations {
    pub defaults: HashMap<String, Vec<String>>,
    pub added: HashMap<String, Vec<String>>,
    pub removed: HashSet<(String, String)>,
}

impl MimeAssociations {
    /// Parse the contents of a `mimeapps.list` file into these associations.
    pub fn parse_content(&mut self, content: &str) {
        let mut section = None;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = match line {
                    "[Default Applications]" => Some(MimeappsSection::Defaults),
                    "[Added Associations]" => Some(MimeappsSection::Added),
                    "[Removed Associations]" => Some(MimeappsSection::Removed),
                    _ => None,
                };
                continue;
            }

            let Some(sec) = section else { continue };
            let Some((m, apps_str)) = line.split_once('=') else {
                continue;
            };
            let m = m.trim().to_string();
            for app_id in apps_str.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                match sec {
                    MimeappsSection::Defaults => {
                        let list = self.defaults.entry(m.clone()).or_default();
                        if !list.iter().any(|existing| existing == app_id) {
                            list.push(app_id.to_string());
                        }
                    }
                    MimeappsSection::Added => {
                        let list = self.added.entry(m.clone()).or_default();
                        if !list.iter().any(|existing| existing == app_id) {
                            list.push(app_id.to_string());
                        }
                    }
                    MimeappsSection::Removed => {
                        self.removed.insert((m.clone(), app_id.to_string()));
                    }
                }
            }
        }
    }
}

/// Find the preferred default application for `mime_type` based on `mimeapps.list`.
#[must_use]
pub fn default_application_for_mime(mime_type: &str) -> Option<DesktopEntry> {
    let apps = discover_applications();
    let associations = load_mimeapps_list();
    resolve_default_application(mime_type, &apps, &associations)
}

/// Pure helper to resolve the default application given applications and associations.
#[must_use]
pub fn resolve_default_application(
    mime_type: &str,
    apps: &HashMap<String, DesktopEntry>,
    associations: &MimeAssociations,
) -> Option<DesktopEntry> {
    // 1. First check [Default Applications] in priority order
    if let Some(default_ids) = associations.defaults.get(mime_type) {
        for id in default_ids {
            if associations
                .removed
                .contains(&(mime_type.to_string(), id.clone()))
            {
                continue;
            }
            if let Some(app) = apps.get(id) {
                if !app.no_display && !app.hidden {
                    return Some(app.clone());
                }
            }
        }
    }

    // 2. Then check [Added Associations]
    if let Some(added_ids) = associations.added.get(mime_type) {
        for id in added_ids {
            if associations
                .removed
                .contains(&(mime_type.to_string(), id.clone()))
            {
                continue;
            }
            if let Some(app) = apps.get(id) {
                if !app.no_display && !app.hidden {
                    return Some(app.clone());
                }
            }
        }
    }

    // 3. Fallback: all discovered apps declaring this MIME type, sorted deterministically
    let mut fallback: Vec<&DesktopEntry> = apps
        .values()
        .filter(|app| {
            !app.no_display
                && !app.hidden
                && !associations
                    .removed
                    .contains(&(mime_type.to_string(), app.id.clone()))
                && app.mime_types.iter().any(|m| m == mime_type)
        })
        .collect();
    fallback.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });

    fallback.first().copied().cloned()
}

/// Return all applications capable of opening `mime_type`.
#[must_use]
pub fn applications_for_mime(mime_type: &str) -> Vec<DesktopEntry> {
    let apps = discover_applications();
    let associations = load_mimeapps_list();
    resolve_applications_for_mime(mime_type, &apps, &associations)
}

/// Pure helper to resolve all capable applications in deterministic order.
#[must_use]
pub fn resolve_applications_for_mime(
    mime_type: &str,
    apps: &HashMap<String, DesktopEntry>,
    associations: &MimeAssociations,
) -> Vec<DesktopEntry> {
    let mut matches = Vec::new();
    let mut seen_ids = HashSet::new();

    // 1. Defaults first
    if let Some(ids) = associations.defaults.get(mime_type) {
        for id in ids {
            if associations
                .removed
                .contains(&(mime_type.to_string(), id.clone()))
            {
                continue;
            }
            if let Some(app) = apps.get(id) {
                if !app.no_display && !app.hidden && seen_ids.insert(app.id.clone()) {
                    matches.push(app.clone());
                }
            }
        }
    }

    // 2. Added associations next
    if let Some(ids) = associations.added.get(mime_type) {
        for id in ids {
            if associations
                .removed
                .contains(&(mime_type.to_string(), id.clone()))
            {
                continue;
            }
            if let Some(app) = apps.get(id) {
                if !app.no_display && !app.hidden && seen_ids.insert(app.id.clone()) {
                    matches.push(app.clone());
                }
            }
        }
    }

    // 3. Fallback: remaining apps advertising this MIME type, sorted deterministically
    let mut remaining: Vec<&DesktopEntry> = apps
        .values()
        .filter(|app| {
            !app.no_display
                && !app.hidden
                && !seen_ids.contains(&app.id)
                && !associations
                    .removed
                    .contains(&(mime_type.to_string(), app.id.clone()))
                && app.mime_types.iter().any(|m| m == mime_type)
        })
        .collect();
    remaining.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });

    for app in remaining {
        seen_ids.insert(app.id.clone());
        matches.push(app.clone());
    }

    matches
}

/// Open a path with the desktop default application, or fall back to `xdg-open`.
pub fn open_path_with_token(path: &Path, token: Option<&str>) -> std::io::Result<()> {
    let mime_type = guess_mime_type(path);
    if let Some(app) = default_application_for_mime(&mime_type) {
        if app.spawn_with_token(&[path], token).is_ok() {
            return Ok(());
        }
    }

    let mut cmd = Command::new("xdg-open");
    cmd.arg(path);
    if let Some(t) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", t);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Open a path with the desktop default application, or fall back to `xdg-open`.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    open_path_with_token(path, None)
}

fn load_mimeapps_list() -> MimeAssociations {
    let mut associations = MimeAssociations::default();
    let mut files = Vec::new();

    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .flat_map(|s| {
            let lower = s.to_ascii_lowercase();
            if lower != s {
                vec![lower, s.to_string()]
            } else {
                vec![lower]
            }
        })
        .collect();

    let mut add_candidates = |dir: &Path| {
        for desktop in &desktops {
            files.push(dir.join(format!("{desktop}-mimeapps.list")));
        }
        files.push(dir.join("mimeapps.list"));
    };

    add_candidates(&config_home());
    for dir in config_dirs() {
        add_candidates(&dir);
    }
    add_candidates(&data_home().join("applications"));
    for dir in data_dirs() {
        add_candidates(&dir.join("applications"));
    }

    for file in files {
        if let Ok(content) = std::fs::read_to_string(&file) {
            associations.parse_content(&content);
        }
    }

    associations
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

    #[test]
    fn test_mimeapps_priority_and_deterministic_order() {
        let mut associations = MimeAssociations::default();
        let content = r#"
[Added Associations]
image/png=viewer-added.desktop;
[Default Applications]
image/png=viewer-default.desktop;
[Removed Associations]
image/png=viewer-banned.desktop;
"#;
        associations.parse_content(content);

        let mut apps = HashMap::new();
        let make_app = |id: &str, name: &str| DesktopEntry {
            id: id.into(),
            path: PathBuf::from(format!("/usr/share/applications/{id}")),
            name: name.into(),
            exec: format!("{name} %f"),
            icon: None,
            mime_types: vec!["image/png".into()],
            terminal: false,
            no_display: false,
            hidden: false,
        };

        apps.insert("viewer-default.desktop".into(), make_app("viewer-default.desktop", "Default Viewer"));
        apps.insert("viewer-added.desktop".into(), make_app("viewer-added.desktop", "Added Viewer"));
        apps.insert("viewer-banned.desktop".into(), make_app("viewer-banned.desktop", "Banned Viewer"));
        apps.insert("zebra.desktop".into(), make_app("zebra.desktop", "Zebra Viewer"));
        apps.insert("alpha.desktop".into(), make_app("alpha.desktop", "Alpha Viewer"));

        // Default app should be viewer-default, despite [Added Associations] appearing first in content
        let default_app = resolve_default_application("image/png", &apps, &associations).unwrap();
        assert_eq!(default_app.id, "viewer-default.desktop");

        // Applications list should order:
        // 1. Defaults (viewer-default)
        // 2. Added (viewer-added)
        // 3. Fallback sorted alphabetically by name: Alpha Viewer ("alpha.desktop"), then Zebra Viewer ("zebra.desktop")
        // viewer-banned must be excluded!
        let capable = resolve_applications_for_mime("image/png", &apps, &associations);
        let ids: Vec<&str> = capable.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "viewer-default.desktop",
                "viewer-added.desktop",
                "alpha.desktop",
                "zebra.desktop"
            ]
        );
    }
}
