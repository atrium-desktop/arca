//! RFC 8089 and FreeDesktop File URI (`file://`) specification.
//!
//! Provides percent-encoding and decoding between native file paths and URIs.
//! Compliant with localhost prefixes and UTF-8 path characters.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// Encode a filesystem path into a canonical `file://...` URI.
#[must_use]
pub fn encode_file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    let bytes = path.as_os_str().as_bytes();
    for &b in bytes {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Decode a `file://` URI into a native `PathBuf`.
/// Correctly handles `file:///path`, `file://localhost/path`, and percent-encoded bytes.
#[must_use]
pub fn decode_file_uri(uri: &str) -> Option<PathBuf> {
    let mut raw = uri.strip_prefix("file://")?;
    if let Some(rest) = raw.strip_prefix("localhost/") {
        raw = rest;
    } else if let Some(rest) = raw.strip_prefix('/') {
        raw = rest;
    } else if raw.is_empty() {
        return Some(PathBuf::from("/"));
    }

    let mut bytes = Vec::with_capacity(raw.len() + 1);
    bytes.push(b'/'); // Restore leading slash for absolute path

    let chars = raw.as_bytes();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == b'%' && i + 2 < chars.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&chars[i + 1..i + 3]).unwrap_or(""), 16) {
                bytes.push(byte);
                i += 3;
                continue;
            }
        }
        bytes.push(chars[i]);
        i += 1;
    }

    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_roundtrip() {
        let p = Path::new("/home/user/My Photos/summer & fall 2024.jpg");
        let uri = encode_file_uri(p);
        assert_eq!(
            uri,
            "file:///home/user/My%20Photos/summer%20%26%20fall%202024.jpg"
        );
        let back = decode_file_uri(&uri).unwrap();
        assert_eq!(back, p);

        let localhost_uri = "file://localhost/home/user/test.txt";
        assert_eq!(
            decode_file_uri(localhost_uri).unwrap(),
            PathBuf::from("/home/user/test.txt")
        );
    }
}
