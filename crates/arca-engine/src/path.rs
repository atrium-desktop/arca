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
}
