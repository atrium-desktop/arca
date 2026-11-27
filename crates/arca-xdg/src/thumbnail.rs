//! [Thumbnail Managing Standard](https://specifications.freedesktop.org/thumbnail-spec/latest/)
//! (version 0.9.0).
//!
//! The thumbnail cache under `$XDG_CACHE_HOME/thumbnails/` is shared by every
//! desktop application, so arca must name, tag and validate entries exactly
//! as other implementations (GLib/GIO, KIO) do:
//!
//! - **Location**: one directory per size flavour (`normal` 128 px, `large`
//!   256 px, `x-large` 512 px, `xx-large` 1024 px).
//! - **Name**: `MD5(canonical URI) + ".png"`, where the URI is escaped the
//!   way `g_filename_to_uri` escapes it ([`thumbnail_uri`]). Any other
//!   escaping produces a different hash and silently forks the cache.
//! - **Metadata**: every thumbnail carries `tEXt` chunks; `Thumb::URI` and
//!   `Thumb::MTime` are mandatory.
//! - **Validity**: a thumbnail is current only when its `Thumb::URI` equals
//!   the source URI and its `Thumb::MTime` equals the source's modification
//!   time in whole seconds ([`is_current`]). The PNG file's own mtime is
//!   irrelevant.
//! - **Writing**: write to a temporary file in the target directory, then
//!   rename, so readers never see a partial PNG; files are `0600` and
//!   directories `0700`.
//! - **Recursion**: files inside the thumbnail root are never thumbnailed.

use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Mandatory key: the canonical URI of the source file.
pub const KEY_URI: &str = "Thumb::URI";
/// Mandatory key: the source modification time, decimal seconds since epoch.
pub const KEY_MTIME: &str = "Thumb::MTime";
/// Optional key: the source size in bytes.
pub const KEY_SIZE: &str = "Thumb::Size";
/// Optional key: the source MIME type.
pub const KEY_MIMETYPE: &str = "Thumb::Mimetype";
/// Optional key: the program that wrote the thumbnail.
pub const KEY_SOFTWARE: &str = "Software";

/// Largest PNG this module will scan for metadata. The biggest flavour is
/// 1024x1024 RGBA, which is ~4 MiB uncompressed.
const MAX_THUMB_BYTES: usize = 8 * 1024 * 1024;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Thumbnail size flavours and their directories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Normal,
    Large,
    XLarge,
    XxLarge,
}

impl Flavor {
    /// Longest edge, in pixels, a thumbnail of this flavour may have.
    #[must_use]
    pub const fn pixels(self) -> u32 {
        match self {
            Flavor::Normal => 128,
            Flavor::Large => 256,
            Flavor::XLarge => 512,
            Flavor::XxLarge => 1024,
        }
    }

    /// Directory name under the thumbnail root.
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Flavor::Normal => "normal",
            Flavor::Large => "large",
            Flavor::XLarge => "x-large",
            Flavor::XxLarge => "xx-large",
        }
    }
}

/// `$XDG_CACHE_HOME/thumbnails`.
#[must_use]
pub fn thumbnail_root() -> PathBuf {
    crate::base::cache_home().join("thumbnails")
}

/// `$XDG_CACHE_HOME/thumbnails/<flavour>`.
#[must_use]
pub fn flavor_dir(flavor: Flavor) -> PathBuf {
    thumbnail_root().join(flavor.dir_name())
}

/// Canonical `file://` URI for `path`, escaped exactly like GLib's
/// `g_filename_to_uri`: unreserved characters plus `!$&'()*+,:=@/` stay
/// literal, everything else is `%XX` with uppercase hex.
///
/// `path` should already be absolute and canonical (symlinks resolved), as
/// the spec requires the hash input to be the canonical URI.
#[must_use]
pub fn thumbnail_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        let literal = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_'
                    | b'.'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b'/'
                    | b':'
                    | b'='
                    | b'@'
            );
        if literal {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Cache file name for a source URI: `MD5(uri) + ".png"`.
#[must_use]
pub fn file_name_for_uri(uri: &str) -> String {
    format!("{}.png", crate::md5::md5_hex(uri.as_bytes()))
}

/// Source mtime as the spec's `Thumb::MTime` value (whole seconds).
#[must_use]
pub fn mtime_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// True when `path` is inside the thumbnail root; such files must never be
/// thumbnailed themselves.
#[must_use]
pub fn is_inside_thumbnail_root(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

/// `tEXt` key/value pairs of a PNG, in file order. `None` when `png` is not
/// a well-formed PNG chunk stream (bad signature, truncated chunk). Chunks
/// whose CRC does not match are skipped rather than trusted.
#[must_use]
pub fn png_text(png: &[u8]) -> Option<Vec<(String, String)>> {
    if png.len() > MAX_THUMB_BYTES {
        return None;
    }
    let mut rest = png.strip_prefix(&PNG_SIGNATURE[..])?;
    let mut out = Vec::new();
    loop {
        if rest.len() < 12 {
            return None;
        }
        let length = u32::from_be_bytes(rest[0..4].try_into().ok()?) as usize;
        let kind: [u8; 4] = rest[4..8].try_into().ok()?;
        let end = length.checked_add(12)?;
        if rest.len() < end {
            return None;
        }
        let data = &rest[8..8 + length];
        let crc = u32::from_be_bytes(rest[8 + length..end].try_into().ok()?);
        if &kind == b"tEXt"
            && crc32(&rest[4..8 + length]) == crc
            && let Some(nul) = data.iter().position(|&b| b == 0)
        {
            // tEXt is Latin-1; spec keys are ASCII and URIs are
            // escaped ASCII, so a lossless byte→char mapping suffices.
            let key: String = data[..nul].iter().map(|&b| b as char).collect();
            let value: String = data[nul + 1..].iter().map(|&b| b as char).collect();
            out.push((key, value));
        }
        if &kind == b"IEND" {
            return Some(out);
        }
        rest = &rest[end..];
    }
}

/// Insert `tEXt` chunks right after `IHDR` (where readers find them without
/// scanning pixel data). `None` if `png` does not start with a valid `IHDR`
/// or a key is not 1–79 Latin-1 characters without NUL.
#[must_use]
pub fn png_with_text(png: &[u8], pairs: &[(&str, &str)]) -> Option<Vec<u8>> {
    let after_sig = png.strip_prefix(&PNG_SIGNATURE[..])?;
    if after_sig.len() < 12 || &after_sig[4..8] != b"IHDR" {
        return None;
    }
    let ihdr_len = u32::from_be_bytes(after_sig[0..4].try_into().ok()?) as usize;
    let split = PNG_SIGNATURE.len() + ihdr_len.checked_add(12)?;
    if png.len() < split {
        return None;
    }
    let mut out = Vec::with_capacity(png.len() + 256);
    out.extend_from_slice(&png[..split]);
    for (key, value) in pairs {
        if key.is_empty() || key.len() > 79 || key.bytes().any(|b| b == 0) {
            return None;
        }
        let mut body = Vec::with_capacity(4 + key.len() + 1 + value.len());
        body.extend_from_slice(b"tEXt");
        body.extend(latin1(key)?);
        body.push(0);
        body.extend(latin1(value)?);
        let data_len = u32::try_from(body.len() - 4).ok()?;
        out.extend_from_slice(&data_len.to_be_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    out.extend_from_slice(&png[split..]);
    Some(out)
}

fn latin1(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).ok().filter(|&b| b != 0))
        .collect()
}

/// Whether a cached thumbnail's metadata describes the current source:
/// `Thumb::URI` equals `uri` and `Thumb::MTime` equals `mtime` exactly.
#[must_use]
pub fn is_current(text: &[(String, String)], uri: &str, mtime: u64) -> bool {
    let get = |key: &str| text.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    get(KEY_URI) == Some(uri)
        && get(KEY_MTIME).and_then(|v| v.trim().parse::<u64>().ok()) == Some(mtime)
}

/// Write `bytes` to `dir/file_name` atomically: create `dir` (mode `0700`)
/// if missing, write a uniquely named `0600` temporary file beside the
/// target, then rename it into place.
pub fn write_atomic(dir: &Path, file_name: &str, bytes: &[u8]) -> std::io::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_data()?;
        std::fs::rename(&tmp, dir.join(file_name))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// CRC-32 (ISO-HDLC, as used by PNG).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid PNG: 1x1 RGBA, hand-assembled.
    fn tiny_png() -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc32(&body).to_be_bytes());
        }
        let mut png = PNG_SIGNATURE.to_vec();
        chunk(&mut png, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        // zlib stored block of one scanline: filter byte + 4 bytes RGBA.
        chunk(
            &mut png,
            b"IDAT",
            &[
                0x78, 0x01, 0x01, 0x05, 0x00, 0xFA, 0xFF, 0, 0, 0, 0, 0, 0x00, 0x05, 0x00, 0x01,
            ],
        );
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn crc32_matches_reference_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn uri_escaping_matches_glib() {
        // Expected values produced by g_filename_to_uri.
        assert_eq!(
            thumbnail_uri(Path::new("/home/u/Song (Live) & more!.flac")),
            "file:///home/u/Song%20(Live)%20&%20more!.flac"
        );
        assert_eq!(
            thumbnail_uri(Path::new("/a/b+c,d=e@f:g$h*i'j~k")),
            "file:///a/b+c,d=e@f:g$h*i'j~k"
        );
        assert_eq!(
            thumbnail_uri(Path::new("/a/#?%;<>[]{}|\\^`\"")),
            "file:///a/%23%3F%25%3B%3C%3E%5B%5D%7B%7D%7C%5C%5E%60%22"
        );
        assert_eq!(
            thumbnail_uri(Path::new("/音乐/é")),
            "file:///%E9%9F%B3%E4%B9%90/%C3%A9"
        );
    }

    #[test]
    fn file_name_matches_spec_example() {
        // Example from the Thumbnail Managing Standard, "Thumbnail Saving".
        assert_eq!(
            file_name_for_uri("file:///home/jens/photos/me.png"),
            "c6ee772d9e49320e97ec29a7eb5b1697.png"
        );
    }

    #[test]
    fn text_roundtrip_and_validity() {
        let tagged = png_with_text(
            &tiny_png(),
            &[
                (KEY_URI, "file:///x/y.png"),
                (KEY_MTIME, "1700000000"),
                (KEY_SOFTWARE, "arca"),
            ],
        )
        .unwrap();
        let text = png_text(&tagged).unwrap();
        assert_eq!(text.len(), 3);
        assert!(is_current(&text, "file:///x/y.png", 1_700_000_000));
        assert!(!is_current(&text, "file:///x/y.png", 1_700_000_001));
        assert!(!is_current(&text, "file:///x/z.png", 1_700_000_000));
        // Untagged PNGs are never current.
        assert!(!is_current(
            &png_text(&tiny_png()).unwrap(),
            "file:///x/y.png",
            1_700_000_000
        ));
    }

    #[test]
    fn malformed_png_is_rejected() {
        assert!(png_text(b"not a png").is_none());
        let png = tiny_png();
        assert!(png_text(&png[..png.len() - 3]).is_none());
        assert!(png_with_text(b"nope", &[(KEY_URI, "x")]).is_none());
        assert!(png_with_text(&png, &[("", "x")]).is_none());
    }

    #[test]
    fn corrupted_text_chunk_is_ignored() {
        let mut tagged = png_with_text(&tiny_png(), &[(KEY_URI, "file:///a")]).unwrap();
        // Flip a byte inside the tEXt value; its CRC no longer matches.
        let at = tagged.windows(8).position(|w| w == b"file:///").unwrap();
        tagged[at] = b'F';
        assert!(png_text(&tagged).unwrap().is_empty());
    }

    #[test]
    fn atomic_write_sets_private_modes() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("arca-xdg-thumb-{}", std::process::id()));
        let dir = base.join("thumbnails/normal");
        write_atomic(&dir, "a.png", b"data").unwrap();
        assert_eq!(std::fs::read(dir.join("a.png")).unwrap(), b"data");
        let file_mode = std::fs::metadata(dir.join("a.png"))
            .unwrap()
            .permissions()
            .mode();
        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(file_mode & 0o777, 0o600);
        assert_eq!(dir_mode & 0o777, 0o700);
        // No temporaries left behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn thumbnail_root_files_are_excluded() {
        let root = Path::new("/home/u/.cache/thumbnails");
        assert!(is_inside_thumbnail_root(
            Path::new("/home/u/.cache/thumbnails/normal/a.png"),
            root
        ));
        assert!(!is_inside_thumbnail_root(
            Path::new("/home/u/Pictures/a.png"),
            root
        ));
    }
}
