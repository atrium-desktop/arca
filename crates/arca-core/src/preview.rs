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

        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = kind_for_extension(&extension);
        let mut facts = vec![
            ("Size".into(), format_size(metadata.len())),
            ("Modified".into(), modified),
            (
                "Type".into(),
                if extension.is_empty() {
                    "File".into()
                } else {
                    extension.to_ascii_uppercase()
                },
            ),
        ];

        let bytes = read_prefix(source);
        if kind == PreviewKind::Image {
            if let Some((width, height)) = image_dimensions(&extension, &bytes) {
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

fn kind_for_extension(extension: &str) -> PreviewKind {
    match extension {
        "txt" | "md" | "rst" | "org" | "log" | "json" | "toml" | "yaml" | "yml" | "xml" | "ini"
        | "cfg" | "conf" | "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "tsx"
        | "jsx" | "go" | "java" | "sh" | "css" | "html" => PreviewKind::Text,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" => PreviewKind::Image,
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" => PreviewKind::Audio,
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "webm" => PreviewKind::Video,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => PreviewKind::Archive,
        _ => PreviewKind::Generic,
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

fn image_dimensions(extension: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    match extension {
        "png" if bytes.len() >= 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" => Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        )),
        "gif" if bytes.len() >= 10 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") => {
            Some((
                u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
                u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
            ))
        }
        "bmp" if bytes.len() >= 26 && &bytes[..2] == b"BM" => Some((
            u32::from_le_bytes(bytes[18..22].try_into().ok()?),
            u32::from_le_bytes(bytes[22..26].try_into().ok()?),
        )),
        "jpg" | "jpeg" => jpeg_dimensions(bytes),
        _ => None,
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
