//! Small path helpers used across the core (string-based; the UI deals in
//! displayable path strings rather than `PathBuf`).

/// The user's home directory, `/` as a last resort.
pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

/// Parent of `path`; the parent of `/` is `/` itself.
pub fn parent(path: &str) -> String {
    let trimmed = normalize(path);
    if trimmed == "/" {
        return "/".into();
    }
    match trimmed.rfind('/') {
        Some(0) => "/".into(),
        Some(i) => trimmed[..i].to_string(),
        None => trimmed,
    }
}

/// Join a child name onto a directory path without producing `//`.
pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Expand a leading `~` and tidy up the path: collapse duplicate slashes,
/// resolve `.`/`..` lexically, drop any trailing slash (except for `/`).
/// Relative paths are anchored at `cwd` when given, else left relative.
pub fn normalize(input: &str) -> String {
    let expanded = expand_tilde(input);
    let is_absolute = expanded.starts_with('/');

    let mut parts: Vec<&str> = Vec::new();
    for seg in expanded.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else if !is_absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }

    let body = parts.join("/");
    if is_absolute {
        format!("/{body}")
    } else if body.is_empty() {
        ".".into()
    } else {
        body
    }
}

/// Normalize and, for relative input, resolve against `cwd`.
pub fn resolve(cwd: &str, input: &str) -> String {
    let expanded = expand_tilde(input);
    if expanded.starts_with('/') {
        normalize(&expanded)
    } else {
        normalize(&join(cwd, &expanded))
    }
}

/// Result of a path auto-completion query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCompletion {
    /// The auto-completed path string (with common prefix or single match applied).
    pub completed: String,
    /// Full list of matching candidate paths (with trailing slashes on directories).
    pub candidates: Vec<String>,
}

/// Auto-complete a filesystem path given the active `cwd` and current user `input`.
pub fn complete_path(cwd: &str, input: &str) -> Option<PathCompletion> {
    let raw_input = input.trim();
    if raw_input == "~" {
        return Some(PathCompletion {
            completed: "~/".into(),
            candidates: vec!["~/".into()],
        });
    }

    let is_tilde = raw_input.starts_with("~/");
    let (dir_prefix, file_prefix) = match raw_input.rfind('/') {
        Some(idx) => (&raw_input[..=idx], &raw_input[idx + 1..]),
        None => ("", raw_input),
    };

    let search_dir = if is_tilde {
        let rest = dir_prefix.strip_prefix("~/").unwrap_or("");
        if rest.is_empty() {
            home_dir()
        } else {
            join(&home_dir(), rest.trim_end_matches('/'))
        }
    } else if dir_prefix.starts_with('/') {
        if dir_prefix == "/" {
            "/".into()
        } else {
            dir_prefix.trim_end_matches('/').to_string()
        }
    } else if dir_prefix.is_empty() {
        cwd.to_string()
    } else {
        resolve(cwd, dir_prefix.trim_end_matches('/'))
    };

    let entries = std::fs::read_dir(&search_dir).ok()?;
    let show_hidden = file_prefix.starts_with('.');

    let mut matches: Vec<(String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let matched = if name.starts_with(file_prefix) {
            true
        } else {
            name.to_lowercase().starts_with(&file_prefix.to_lowercase())
        };
        if matched {
            let is_dir = entry.file_type().map(|ft| ft.is_dir() || ft.is_symlink()).unwrap_or(false);
            matches.push((name, is_dir));
        }
    }

    if matches.is_empty() {
        return None;
    }

    matches.sort_by_key(|a| a.0.to_lowercase());

    let candidate_paths: Vec<String> = matches
        .iter()
        .map(|(name, is_dir)| {
            let slash = if *is_dir { "/" } else { "" };
            format!("{dir_prefix}{name}{slash}")
        })
        .collect();

    if matches.len() == 1 {
        return Some(PathCompletion {
            completed: candidate_paths[0].clone(),
            candidates: candidate_paths,
        });
    }

    // Multiple matches: compute longest common prefix
    let first_name = &matches[0].0;
    let mut common_len = first_name.len();
    for (name, _) in &matches[1..] {
        common_len = common_len.min(name.len());
        while common_len > 0 && !name[..common_len].eq_ignore_ascii_case(&first_name[..common_len]) {
            common_len -= 1;
        }
    }

    let common_part = &first_name[..common_len];
    let completed = if common_part.len() > file_prefix.len() {
        format!("{dir_prefix}{common_part}")
    } else {
        raw_input.to_string()
    };

    Some(PathCompletion {
        completed,
        candidates: candidate_paths,
    })
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        home_dir()
    } else if let Some(rest) = input.strip_prefix("~/") {
        join(&home_dir(), rest)
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_of_root_is_root() {
        assert_eq!(parent("/"), "/");
        assert_eq!(parent("/home"), "/");
        assert_eq!(parent("/home/ming"), "/home");
        assert_eq!(parent("/home/ming/"), "/home");
    }

    #[test]
    fn join_avoids_double_slash() {
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/home", "ming"), "/home/ming");
    }

    #[test]
    fn normalize_resolves_dots_and_slashes() {
        assert_eq!(normalize("/a/./b/../c"), "/a/c");
        assert_eq!(normalize("/a//b/"), "/a/b");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(normalize(""), ".");
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = home_dir();
        assert_eq!(normalize("~"), home);
        assert_eq!(normalize("~/docs"), join(&home, "docs"));
    }

    #[test]
    fn resolve_anchors_relative_paths() {
        assert_eq!(resolve("/a/b", "c"), "/a/b/c");
        assert_eq!(resolve("/a/b", "../c"), "/a/c");
        assert_eq!(resolve("/a/b", "/x"), "/x");
        assert_eq!(resolve("/a/b", "~/x"), join(&home_dir(), "x"));
    }

    #[test]
    fn path_completion_single_and_multiple() {
        let dir = std::env::temp_dir().join(format!(
            "arca-path-comp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("documents")).unwrap();
        std::fs::create_dir_all(dir.join("downloads")).unwrap();
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();

        let dir_str = dir.to_str().unwrap();

        // "~" completes to "~/"
        assert_eq!(complete_path(dir_str, "~").unwrap().completed, "~/");

        // Single directory match appends "/"
        let single = complete_path(dir_str, &format!("{dir_str}/mu")).unwrap();
        assert_eq!(single.completed, format!("{dir_str}/music/"));
        assert_eq!(single.candidates, vec![format!("{dir_str}/music/")]);

        // Multiple matches with common prefix "do" -> "doc" and "dow"
        let multi = complete_path(dir_str, &format!("{dir_str}/d")).unwrap();
        assert_eq!(multi.completed, format!("{dir_str}/do"));
        assert_eq!(
            multi.candidates,
            vec![format!("{dir_str}/documents/"), format!("{dir_str}/downloads/")]
        );

        // Single file match does NOT append "/"
        let file = complete_path(dir_str, &format!("{dir_str}/not")).unwrap();
        assert_eq!(file.completed, format!("{dir_str}/notes.txt"));

        // Non-existent path returns None
        assert!(complete_path(dir_str, &format!("{dir_str}/xyz")).is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
