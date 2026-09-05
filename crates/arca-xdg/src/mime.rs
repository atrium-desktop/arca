//! XDG Shared MIME-info Specification implementation.
//!
//! Provides MIME type resolution via system `globs2` database with fast
//! fallback rules and magic-byte sniffing for unextended or ambiguous files.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

use crate::base::all_data_dirs;

/// One parsed glob rule from `globs2`.
#[derive(Debug, Clone)]
struct GlobRule {
    weight: u32,
    mime_type: String,
    pattern: String,
    case_sensitive: bool,
}

/// The Shared MIME database cache.
#[derive(Debug, Default)]
pub struct MimeDatabase {
    rules: Vec<GlobRule>,
    aliases: HashMap<String, String>,
    subclasses: HashMap<String, Vec<String>>,
}

static GLOBAL_MIME_DB: OnceLock<MimeDatabase> = OnceLock::new();

/// Return the process-wide shared MIME database.
#[must_use]
pub fn default_database() -> &'static MimeDatabase {
    GLOBAL_MIME_DB.get_or_init(MimeDatabase::load_system)
}

/// Convenience function: resolve the MIME type for `path`.
/// Reads up to 512 bytes for content sniffing if needed.
#[must_use]
pub fn guess_mime_type(path: &Path) -> String {
    default_database().guess_mime_type(path)
}

impl MimeDatabase {
    /// Load MIME database from `$XDG_DATA_HOME/mime` and `$XDG_DATA_DIRS/mime`.
    #[must_use]
    pub fn load_system() -> Self {
        let mut db = MimeDatabase::default();
        for base in all_data_dirs() {
            let mime_dir = base.join("mime");
            if !mime_dir.exists() {
                continue;
            }
            db.load_globs2(&mime_dir.join("globs2"));
            db.load_aliases(&mime_dir.join("aliases"));
            db.load_subclasses(&mime_dir.join("subclasses"));
        }

        // Sort rules by weight descending
        db.rules.sort_by_key(|b| std::cmp::Reverse(b.weight));
        db
    }

    fn load_globs2(&mut self, path: &Path) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            // Format: weight:mime_type:glob[:flags]
            let mut parts = line.split(':');
            let Some(weight_str) = parts.next() else { continue };
            let Some(mime) = parts.next() else { continue };
            let Some(glob) = parts.next() else { continue };
            let flags = parts.next().unwrap_or("");

            let weight = weight_str.parse::<u32>().unwrap_or(50);
            let case_sensitive = flags.contains("cs");

            self.rules.push(GlobRule {
                weight,
                mime_type: mime.to_string(),
                pattern: glob.to_string(),
                case_sensitive,
            });
        }
    }

    fn load_aliases(&mut self, path: &Path) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((alias, canonical)) = line.split_once(' ') {
                self.aliases.insert(alias.trim().into(), canonical.trim().into());
            }
        }
    }

    fn load_subclasses(&mut self, path: &Path) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((sub, parent)) = line.split_once(' ') {
                self.subclasses
                    .entry(sub.trim().into())
                    .or_default()
                    .push(parent.trim().into());
            }
        }
    }

    /// Resolve the canonical MIME type of `path`.
    #[must_use]
    pub fn guess_mime_type(&self, path: &Path) -> String {
        // 1. Directory check
        if path.is_dir() {
            return "inode/directory".to_string();
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();

        // 2. Glob matching from database
        for rule in &self.rules {
            if glob_match(&rule.pattern, &name, rule.case_sensitive) {
                return self.canonicalize(&rule.mime_type);
            }
        }

        // 3. Built-in fast fallback extensions
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if let Some(mime) = builtin_extension_mime(&ext.to_ascii_lowercase()) {
                return self.canonicalize(mime);
            }
        }

        // 4. Content / Magic sniffing on the first 512 bytes
        if let Some(magic_mime) = sniff_file_magic(path) {
            return self.canonicalize(magic_mime);
        }

        "application/octet-stream".to_string()
    }

    /// Resolve aliases to canonical MIME types.
    #[must_use]
    pub fn canonicalize<'a>(&'a self, mime: &'a str) -> String {
        if let Some(target) = self.aliases.get(mime) {
            target.clone()
        } else {
            mime.to_string()
        }
    }

    /// Check whether `sub` inherits from `parent`.
    #[must_use]
    pub fn is_subclass_of(&self, sub: &str, parent: &str) -> bool {
        if sub == parent {
            return true;
        }
        if let Some(parents) = self.subclasses.get(sub) {
            for p in parents {
                if self.is_subclass_of(p, parent) {
                    return true;
                }
            }
        }
        false
    }
}

/// Match simple globs (supports leading `*`, trailing `*`, and exact literals).
fn glob_match(pattern: &str, text: &str, case_sensitive: bool) -> bool {
    if pattern.starts_with('*') && pattern.len() > 1 && !pattern[1..].contains('*') {
        let suffix = &pattern[1..];
        if case_sensitive {
            text.ends_with(suffix)
        } else {
            text.to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
        }
    } else if pattern.ends_with('*') && pattern.len() > 1 && !pattern[..pattern.len() - 1].contains('*') {
        let prefix = &pattern[..pattern.len() - 1];
        if case_sensitive {
            text.starts_with(prefix)
        } else {
            text.to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
        }
    } else if case_sensitive {
        pattern == text
    } else {
        pattern.eq_ignore_ascii_case(text)
    }
}

fn sniff_file_magic(path: &Path) -> Option<&'static str> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 512];
    let n = f.read(&mut buf).ok()?;
    if n == 0 {
        return Some("inode/x-empty");
    }
    let data = &buf[..n];

    if data.starts_with(b"\x7fELF") {
        return Some("application/x-executable");
    }
    if data.starts_with(b"#!") {
        return Some("text/x-script");
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if data.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if data.starts_with(b"%PDF") {
        return Some("application/pdf");
    }
    if data.starts_with(b"PK\x03\x04") {
        return Some("application/zip");
    }
    if data.starts_with(b"\x1f\x8b") {
        return Some("application/gzip");
    }
    if data.starts_with(b"fLaC") {
        return Some("audio/flac");
    }
    if data.starts_with(b"ID3") {
        return Some("audio/mpeg");
    }
    if data.starts_with(b"OggS") {
        return Some("audio/ogg");
    }
    if data.starts_with(b"<?xml") || data.starts_with(b"<svg") {
        return Some("image/svg+xml");
    }

    // If all bytes look like clean UTF-8 text without control characters
    if std::str::from_utf8(data).is_ok() && !data.contains(&0) {
        return Some("text/plain");
    }

    None
}

/// Fast built-in table for environments without system mime-info.
fn builtin_extension_mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" => "text/x-c++",
        "rs" => "text/rust",
        "py" => "text/x-python",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "xz" => "application/x-xz",
        "7z" => "application/x-7z-compressed",
        "desktop" => "application/x-desktop",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_mimes() {
        assert_eq!(builtin_extension_mime("png"), Some("image/png"));
        assert_eq!(builtin_extension_mime("rs"), Some("text/rust"));
        assert_eq!(builtin_extension_mime("desktop"), Some("application/x-desktop"));
    }

    #[test]
    fn test_glob_matching() {
        assert!(glob_match("*.tar.gz", "archive.tar.gz", false));
        assert!(glob_match("*.PNG", "photo.png", false));
        assert!(!glob_match("*.PNG", "photo.png", true));
        assert!(glob_match("Makefile*", "Makefile", false));
    }

    #[test]
    fn test_magic_sniffing() {
        let elf_header = b"\x7fELF\x02\x01\x01\x00";
        let tmp = std::env::temp_dir().join("arca_xdg_test_magic_elf");
        std::fs::write(&tmp, elf_header).unwrap();
        assert_eq!(sniff_file_magic(&tmp), Some("application/x-executable"));
        std::fs::remove_file(&tmp).ok();
    }
}
