//! XDG and Linux Desktop standard file clipboard interchange formats.
//!
//! Supports:
//! - `x-special/gnome-copied-files`: standard multi-file copy/cut payload.
//! - `text/uri-list`: RFC 2483 CRLF-separated standard URI list.

use std::path::PathBuf;

use crate::uri::{decode_file_uri, encode_file_uri};

/// Standard file clipboard payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

impl FileClipboard {
    #[must_use]
    pub fn new(paths: Vec<PathBuf>, cut: bool) -> Self {
        Self { paths, cut }
    }

    /// Format as standard Linux desktop `x-special/gnome-copied-files`.
    #[must_use]
    pub fn to_gnome_copied_files(&self) -> String {
        let action = if self.cut { "cut" } else { "copy" };
        let mut out = format!("{action}\n");
        for p in &self.paths {
            out.push_str(&encode_file_uri(p));
            out.push('\n');
        }
        out
    }

    /// Format as standard RFC 2483 / XDG `text/uri-list`.
    #[must_use]
    pub fn to_uri_list(&self) -> String {
        let mut out = String::new();
        for p in &self.paths {
            out.push_str(&encode_file_uri(p));
            out.push_str("\r\n");
        }
        out
    }

    /// Parse clipboard payload from system clipboard string.
    /// Handles `x-special/gnome-copied-files`, `text/uri-list`, and raw file paths.
    #[must_use]
    pub fn from_text_payload(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }

        let mut lines = text.lines();
        let first = lines.next()?.trim();

        let (cut, remaining_lines) = if first == "cut" {
            (true, lines.collect::<Vec<_>>())
        } else if first == "copy" {
            (false, lines.collect::<Vec<_>>())
        } else {
            let mut l = vec![first];
            l.extend(lines);
            (false, l)
        };

        let mut paths = Vec::new();
        for line in remaining_lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(p) = decode_file_uri(line) {
                if p.exists() {
                    paths.push(p);
                }
            } else if line.starts_with('/') {
                let p = PathBuf::from(line);
                if p.exists() {
                    paths.push(p);
                }
            }
        }

        if paths.is_empty() {
            None
        } else {
            Some(Self { paths, cut })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_payload_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_clip_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let f1 = tmp.join("file1.txt");
        let f2 = tmp.join("file2.txt");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        let clip = FileClipboard::new(vec![f1.clone(), f2.clone()], false);
        let payload = clip.to_gnome_copied_files();
        assert!(payload.starts_with("copy\n"));

        let parsed = FileClipboard::from_text_payload(&payload).unwrap();
        assert!(!parsed.cut);
        assert_eq!(parsed.paths, vec![f1.clone(), f2.clone()]);

        let cut_clip = FileClipboard::new(vec![f1.clone()], true);
        let cut_payload = cut_clip.to_gnome_copied_files();
        assert!(cut_payload.starts_with("cut\n"));
        let parsed_cut = FileClipboard::from_text_payload(&cut_payload).unwrap();
        assert!(parsed_cut.cut);
        assert_eq!(parsed_cut.paths, vec![f1]);

        std::fs::remove_dir_all(&tmp).ok();
    }
}
