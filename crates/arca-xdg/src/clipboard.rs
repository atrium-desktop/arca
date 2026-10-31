//! XDG and Linux Desktop standard file transfer interchange formats.
//!
//! One payload type serves both the clipboard (`Ctrl+C`/`Ctrl+X`) and
//! drag-and-drop, because both transports carry the same three encodings:
//!
//! - `x-special/gnome-copied-files`: a `copy`/`cut` verb line followed by
//!   one URI per line. The only format that encodes the *action*.
//! - `text/uri-list` (RFC 2483): CRLF-separated URIs, no action.
//! - bare newline-separated filesystem paths (what several toolkits put on
//!   the text selection, and what a same-process drag carries).
//!
//! [`TransferAction`] is a type rather than a `bool` so call sites cannot
//! silently invert copy and move — the destructive direction.

use std::path::PathBuf;

use crate::uri::{decode_file_uri, encode_file_uri};

/// What a transfer does to the source once it reaches the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferAction {
    /// Leave the source in place.
    Copy,
    /// Remove the source after a successful transfer.
    Move,
}

impl TransferAction {
    /// The `x-special/gnome-copied-files` verb for this action.
    #[must_use]
    pub fn verb(self) -> &'static str {
        match self {
            TransferAction::Copy => "copy",
            TransferAction::Move => "cut",
        }
    }

    fn from_verb(verb: &str) -> Option<Self> {
        match verb {
            "copy" => Some(TransferAction::Copy),
            "cut" => Some(TransferAction::Move),
            _ => None,
        }
    }
}

/// A file transfer payload: what to transfer, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub action: TransferAction,
}

impl FileClipboard {
    #[must_use]
    pub fn new(paths: Vec<PathBuf>, action: TransferAction) -> Self {
        Self { paths, action }
    }

    /// Whether the transfer consumes the source.
    #[must_use]
    pub fn is_move(&self) -> bool {
        self.action == TransferAction::Move
    }

    /// Encode as `x-special/gnome-copied-files`, the format that preserves
    /// the action across process boundaries.
    #[must_use]
    pub fn to_gnome_copied_files(&self) -> String {
        let mut out = String::from(self.action.verb());
        out.push('\n');
        for p in &self.paths {
            out.push_str(&encode_file_uri(p));
            out.push('\n');
        }
        out
    }

    /// Encode as RFC 2483 `text/uri-list` (CRLF separated, no action).
    ///
    /// This is the payload handed to the drag source: the Wayland host only
    /// advertises `text/uri-list` for drag text that starts with `/` or with
    /// a `file://` URI, so a gnome-copied-files payload would not reach other
    /// applications at all.
    #[must_use]
    pub fn to_uri_list(&self) -> String {
        let mut out = String::new();
        for p in &self.paths {
            out.push_str(&encode_file_uri(p));
            out.push_str("\r\n");
        }
        out
    }

    /// Parse a clipboard or drop payload.
    ///
    /// `default_action` supplies the action for formats that cannot encode
    /// one (`text/uri-list`, bare paths). `require_existing` filters paths
    /// that no longer resolve, which is right for a *paste* but wrong for a
    /// *drop*: a vanished source should surface an error at transfer time
    /// rather than silently shrink the selection.
    #[must_use]
    pub fn parse(
        text: &str,
        default_action: TransferAction,
        require_existing: bool,
    ) -> Option<Self> {
        let text = text.trim_end_matches(['\r', '\n', ' ', '\t']);
        if text.is_empty() {
            return None;
        }

        let mut lines = text.lines().map(str::trim);
        let first = lines.clone().next()?;

        // A leading `copy`/`cut` line is the gnome-copied-files verb and is
        // authoritative: it is the only in-band action signal we ever get.
        let (action, body) = match TransferAction::from_verb(first) {
            Some(action) => (action, lines.by_ref().skip(1).collect::<Vec<_>>()),
            None => (default_action, lines.collect::<Vec<_>>()),
        };

        let mut paths = Vec::new();
        for line in body {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let decoded = decode_file_uri(line).or_else(|| {
                // Bare absolute path (also what a same-process drag carries).
                line.starts_with('/').then(|| PathBuf::from(line))
            });
            let Some(path) = decoded else { continue };
            if !path.is_absolute() {
                continue;
            }
            if require_existing && !path.exists() {
                continue;
            }
            if !paths.contains(&path) {
                paths.push(path);
            }
        }

        if paths.is_empty() {
            None
        } else {
            Some(Self { paths, action })
        }
    }

    /// Parse a *clipboard* payload: unknown formats mean copy, and paths that
    /// no longer exist are dropped.
    #[must_use]
    pub fn from_text_payload(text: &str) -> Option<Self> {
        Self::parse(text, TransferAction::Copy, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_payload_roundtrip_preserves_action() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_clip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let f1 = tmp.join("file1.txt");
        let f2 = tmp.join("with space.txt");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        let clip = FileClipboard::new(vec![f1.clone(), f2.clone()], TransferAction::Copy);
        let payload = clip.to_gnome_copied_files();
        assert!(payload.starts_with("copy\n"));
        let parsed = FileClipboard::from_text_payload(&payload).unwrap();
        assert_eq!(parsed.action, TransferAction::Copy);
        assert_eq!(parsed.paths, vec![f1.clone(), f2.clone()]);

        let cut = FileClipboard::new(vec![f1.clone()], TransferAction::Move);
        let cut_payload = cut.to_gnome_copied_files();
        assert!(cut_payload.starts_with("cut\n"));
        let parsed_cut = FileClipboard::from_text_payload(&cut_payload).unwrap();
        assert_eq!(parsed_cut.action, TransferAction::Move);
        assert_eq!(parsed_cut.paths, vec![f1]);

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The URI list a drag source publishes: every path, CRLF separated, and
    /// re-parseable without losing entries.
    #[test]
    fn uri_list_roundtrip_keeps_every_path() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_urilist_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let paths: Vec<PathBuf> = (0..5)
            .map(|i| {
                let p = tmp.join(format!("f{i}.txt"));
                std::fs::write(&p, b"x").unwrap();
                p
            })
            .collect();

        let payload = FileClipboard::new(paths.clone(), TransferAction::Move).to_uri_list();
        assert!(payload.contains("\r\n"));

        // A drop has no verb line, so the caller-supplied default applies.
        let parsed = FileClipboard::parse(&payload, TransferAction::Move, false).unwrap();
        assert_eq!(parsed.action, TransferAction::Move);
        assert_eq!(parsed.paths, paths);

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Bare absolute paths (what a same-process drag stages) parse too.
    #[test]
    fn bare_paths_parse_with_default_action() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_bare_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let a = tmp.join("a.txt");
        let b = tmp.join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();

        let payload = format!("{}\n{}\n", a.display(), b.display());
        let parsed = FileClipboard::parse(&payload, TransferAction::Move, false).unwrap();
        assert_eq!(parsed.action, TransferAction::Move);
        assert_eq!(parsed.paths, vec![a.clone(), b.clone()]);

        // Deduplicated, and comment lines skipped.
        let dupes = format!(
            "{}\n# comment\n{}\n{}\n",
            a.display(),
            a.display(),
            b.display()
        );
        let parsed = FileClipboard::parse(&dupes, TransferAction::Copy, false).unwrap();
        assert_eq!(parsed.paths, vec![a, b]);

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A paste must not carry paths that already vanished; a drop keeps them
    /// so the transfer can report the real error.
    #[test]
    fn require_existing_filters_only_for_paste() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_missing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let payload = format!("{}\n", tmp.join("gone.txt").display());

        assert!(FileClipboard::parse(&payload, TransferAction::Copy, true).is_none());
        let dropped = FileClipboard::parse(&payload, TransferAction::Copy, false).unwrap();
        assert_eq!(dropped.paths.len(), 1);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn junk_payloads_are_rejected() {
        assert!(FileClipboard::from_text_payload("").is_none());
        assert!(FileClipboard::from_text_payload("   \n\t\r\n").is_none());
        assert!(FileClipboard::from_text_payload("just some prose").is_none());
        // Relative paths are never transferable.
        assert!(FileClipboard::parse("relative/path.txt", TransferAction::Copy, false).is_none());
    }
}
