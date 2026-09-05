//! Lightweight, dependency-free Quick Look data extraction.
//!
//! The UI owns presentation and animation; this module performs bounded file
//! reads and turns a path into safe metadata/text that can be rendered without
//! blocking on an external preview service.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::format::{format_size, format_time};

const READ_LIMIT: u64 = 96 * 1024;
const TEXT_LIMIT: usize = 20 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Directory,
    Text,
    Image,
    Audio,
    Video,
    Archive,
    Generic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub title: String,
    pub path: String,
    pub kind: PreviewKind,
    pub subtitle: String,
    pub facts: Vec<(String, String)>,
    /// Bounded UTF-8 text excerpt for text-like files.
    pub text: Option<String>,
}

impl Preview {
    pub fn load(path: &str) -> Preview {
        let source = Path::new(path);
        let title = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| path.to_string());
        let Ok(metadata) = std::fs::symlink_metadata(source) else {
            return Preview {
                title,
                path: path.into(),
                kind: PreviewKind::Generic,
                subtitle: "Preview unavailable".into(),
                facts: Vec::new(),
                text: None,
            };
        };

        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| format_time(duration.as_secs() as i64))
            .unwrap_or_else(|| "Unknown".into());

        if metadata.is_dir() {
            let count = std::fs::read_dir(source)
                .map(|items| items.take(10_001).filter_map(Result::ok).count())
                .unwrap_or(0);
            let count_label = if count > 10_000 {
                "More than 10,000 items".into()
            } else {
                format!("{count} item{}", if count == 1 { "" } else { "s" })
            };
            return Preview {
                title,
                path: path.into(),
                kind: PreviewKind::Directory,
                subtitle: count_label.clone(),
                facts: vec![
                    ("Contents".into(), count_label),
                    ("Modified".into(), modified),
                ],
                text: None,
            };
        }

        let mime = crate::mime::guess_mime_type(source);
        let kind = kind_for_mime(&mime);
        let mut facts = vec![
            ("Size".into(), format_size(metadata.len())),
            ("Modified".into(), modified),
            ("Type".into(), mime.clone()),
        ];

        let bytes = read_prefix(source);
        if kind == PreviewKind::Image {
            if let Some((width, height)) = image_dimensions(&mime, &bytes) {
                facts.insert(1, ("Dimensions".into(), format!("{width} × {height}")));
            }
        }

        let text = if matches!(kind, PreviewKind::Text) || looks_like_text(&bytes) {
            Some(text_excerpt(&bytes))
        } else {
            None
        };
        let subtitle = match kind {
            PreviewKind::Text => "Text document",
            PreviewKind::Image => "Image",
            PreviewKind::Audio => "Audio",
            PreviewKind::Video => "Video",
            PreviewKind::Archive => "Archive",
            _ if text.is_some() => "Text document",
            _ => "File",
        }
        .into();

        Preview {
            title,
            path: path.into(),
            kind,
            subtitle,
            facts,
            text,
        }
    }
}

fn kind_for_mime(mime: &str) -> PreviewKind {
    if mime.starts_with("text/")
        || mime == "application/json"
        || mime == "application/toml"
        || mime == "application/yaml"
        || mime == "application/xml"
        || mime == "application/javascript"
    {
        PreviewKind::Text
    } else if mime.starts_with("image/") {
        PreviewKind::Image
    } else if mime.starts_with("audio/") {
        PreviewKind::Audio
    } else if mime.starts_with("video/") {
        PreviewKind::Video
    } else if mime == "application/zip"
        || mime == "application/x-tar"
        || mime == "application/gzip"
        || mime == "application/x-xz"
        || mime == "application/x-7z-compressed"
    {
        PreviewKind::Archive
    } else {
        PreviewKind::Generic
    }
}

fn read_prefix(path: &Path) -> Vec<u8> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let _ = file.take(READ_LIMIT).read_to_end(&mut bytes);
    bytes
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    !sample.contains(&0)
        && sample
            .iter()
            .filter(|&&byte| byte < 0x09 || (byte > 0x0d && byte < 0x20))
            .count()
            * 20
            < sample.len()
}

fn text_excerpt(bytes: &[u8]) -> String {
    let end = bytes.len().min(TEXT_LIMIT);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    text = text
        .chars()
        .map(|ch| {
            if ch == '\t' || ch == '\n' || !ch.is_control() {
                ch
            } else {
                '�'
            }
        })
        .collect();
    if bytes.len() > end {
        text.push_str("\n\n… Preview truncated");
    }
    text
}

fn image_dimensions(mime: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    if (mime == "image/png" || bytes.starts_with(b"\x89PNG\r\n\x1a\n")) && bytes.len() >= 24 {
        Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ))
    } else if (mime == "image/gif" || bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"))
        && bytes.len() >= 10
    {
        Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
        ))
    } else if (mime == "image/bmp" || bytes.starts_with(b"BM")) && bytes.len() >= 26 {
        Some((
            u32::from_le_bytes(bytes[18..22].try_into().ok()?),
            u32::from_le_bytes(bytes[22..26].try_into().ok()?),
        ))
    } else if mime == "image/jpeg" || bytes.starts_with(b"\xff\xd8") {
        jpeg_dimensions(bytes)
    } else {
        None
    }
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[..2] != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2usize;
    while offset + 8 < bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if marker == 0xd8 || marker == 0xd9 {
            continue;
        }
        if offset + 2 > bytes.len() {
            return None;
        }
        let length = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) && length >= 7 {
            let height = u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as u32;
            let width = u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as u32;
            return Some((width, height));
        }
        offset += length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_png_dimensions() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&[0; 8]);
        png.extend_from_slice(&1920u32.to_be_bytes());
        png.extend_from_slice(&1080u32.to_be_bytes());
        assert_eq!(image_dimensions("png", &png), Some((1920, 1080)));
    }

    #[test]
    fn text_detection_rejects_binary_data() {
        assert!(looks_like_text(b"hello\nworld"));
        assert!(!looks_like_text(b"abc\0def"));
    }
}
