//! FreeDesktop.org (XDG) Trash Specification v1.0 implementation.
//!
//! Supports:
//! - Home trash (`$XDG_DATA_HOME/Trash`)
//! - Per-mount / top-level trash (`$topdir/.Trash-$UID`) avoiding cross-device copies
//! - RFC-compliant `.trashinfo` metadata generation and parsing
//! - Trash enumeration, item restoration, single-item deletion, and trash emptying.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::base::{data_home, home_dir};

/// Information about an item in the trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    pub id: String,
    pub original_name: String,
    pub original_path: PathBuf,
    pub deletion_date: String,
    pub file_path: PathBuf,
    pub info_path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// The home trash root (`$XDG_DATA_HOME/Trash`).
#[must_use]
pub fn home_trash_root() -> PathBuf {
    data_home().join("Trash")
}

/// Compatibility alias for home trash root.
#[must_use]
pub fn trash_root() -> PathBuf {
    home_trash_root()
}

/// Determine the appropriate trash directory for `path` according to XDG Trash Spec v1.0.
#[must_use]
pub fn trash_root_for(path: &Path) -> PathBuf {
    let Ok(target_meta) = fs::metadata(path) else {
        return home_trash_root();
    };
    let target_dev = target_meta.dev();

    let home_trash = home_trash_root();
    let home_dev = fs::metadata(home_dir()).map(|m| m.dev()).unwrap_or(0);

    if target_dev == home_dev {
        return home_trash;
    }

    let mut topdir = path.to_path_buf();
    while let Some(parent) = topdir.parent() {
        if let Ok(m) = fs::metadata(parent) {
            if m.dev() != target_dev {
                break;
            }
            topdir = parent.to_path_buf();
        } else {
            break;
        }
    }

    let uid = unsafe { libc_getuid() };
    let user_trash = topdir.join(format!(".Trash-{uid}"));
    if user_trash.exists() || fs::create_dir_all(&user_trash).is_ok() {
        return user_trash;
    }

    home_trash
}

unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

/// Move every path into the appropriate trash.
pub fn trash_paths<P: AsRef<Path>>(paths: &[P], custom_root: Option<&Path>) -> io::Result<usize> {
    let mut trashed = 0;

    for p in paths {
        let path = p.as_ref();
        let root = custom_root
            .map(Path::to_path_buf)
            .unwrap_or_else(|| trash_root_for(path));

        let files_dir = root.join("files");
        let info_dir = root.join("info");
        fs::create_dir_all(&files_dir)?;
        fs::create_dir_all(&info_dir)?;

        let filename = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no filename"))?
            .to_string_lossy()
            .into_owned();

        let unique = unique_trash_name(&files_dir, &filename);
        let target = files_dir.join(&unique);
        let info_file = info_dir.join(format!("{unique}.trashinfo"));

        let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let info_content = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            percent_encode(&absolute.to_string_lossy()),
            local_deletion_date_now()
        );

        fs::write(&info_file, info_content)?;

        match fs::rename(path, &target) {
            Ok(()) => {}
            Err(_) => {
                if path.is_dir() {
                    copy_dir_recursive(path, &target)?;
                    fs::remove_dir_all(path)?;
                } else {
                    fs::copy(path, &target)?;
                    fs::remove_file(path)?;
                }
            }
        }

        trashed += 1;
    }

    Ok(trashed)
}

/// List all items currently in the user's home trash.
pub fn list_trash() -> io::Result<Vec<TrashItem>> {
    list_trash_in_root(&home_trash_root())
}

/// List items in a specific trash root.
pub fn list_trash_in_root(root: &Path) -> io::Result<Vec<TrashItem>> {
    let info_dir = root.join("info");
    let files_dir = root.join("files");
    let mut items = Vec::new();

    let Ok(entries) = fs::read_dir(info_dir) else {
        return Ok(items);
    };

    for entry in entries.flatten() {
        let info_path = entry.path();
        if !info_path
            .extension()
            .is_some_and(|ext| ext == "trashinfo")
        {
            continue;
        }

        let Some(stem) = info_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let file_path = files_dir.join(stem);
        if !file_path.exists() {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&info_path) {
            if let Some((orig_path, date)) = parse_trashinfo(&content) {
                let meta = fs::symlink_metadata(&file_path).ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let original_name = orig_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| stem.to_string());

                items.push(TrashItem {
                    id: stem.to_string(),
                    original_name,
                    original_path: orig_path,
                    deletion_date: date,
                    file_path,
                    info_path,
                    is_dir,
                    size,
                });
            }
        }
    }

    items.sort_by(|a, b| b.deletion_date.cmp(&a.deletion_date));
    Ok(items)
}

/// Restore a trashed item back to its original location.
pub fn restore_trash_item(item: &TrashItem) -> io::Result<PathBuf> {
    if let Some(parent) = item.original_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let dest = if !item.original_path.exists() {
        item.original_path.clone()
    } else {
        let parent = item.original_path.parent().unwrap_or(Path::new("/"));
        unique_destination(parent, &item.original_name)
    };

    match fs::rename(&item.file_path, &dest) {
        Ok(()) => {}
        Err(_) => {
            if item.is_dir {
                copy_dir_recursive(&item.file_path, &dest)?;
                fs::remove_dir_all(&item.file_path)?;
            } else {
                fs::copy(&item.file_path, &dest)?;
                fs::remove_file(&item.file_path)?;
            }
        }
    }

    let _ = fs::remove_file(&item.info_path);
    Ok(dest)
}

/// Permanently delete a single item from the trash.
pub fn purge_trash_item(item: &TrashItem) -> io::Result<()> {
    if item.is_dir {
        fs::remove_dir_all(&item.file_path)?;
    } else {
        fs::remove_file(&item.file_path)?;
    }
    fs::remove_file(&item.info_path).ok();
    Ok(())
}

/// Empty the entire home trash.
pub fn empty_trash() -> io::Result<usize> {
    let items = list_trash()?;
    let count = items.len();
    for item in items {
        purge_trash_item(&item).ok();
    }
    Ok(count)
}

fn parse_trashinfo(content: &str) -> Option<(PathBuf, String)> {
    let mut path = None;
    let mut date = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(encoded) = line.strip_prefix("Path=") {
            let decoded = percent_decode(encoded)?;
            path = Some(PathBuf::from(decoded));
        } else if let Some(d) = line.strip_prefix("DeletionDate=") {
            date = Some(d.to_string());
        }
    }

    match (path, date) {
        (Some(p), Some(d)) => Some((p, d)),
        _ => None,
    }
}

fn unique_trash_name(files_dir: &Path, name: &str) -> String {
    if !files_dir.join(name).exists() {
        return name.to_string();
    }
    for n in 1.. {
        let candidate = format!("{name}.{n}");
        if !files_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_destination(dst_dir: &Path, name: &str) -> PathBuf {
    let first = dst_dir.join(name);
    if !first.exists() {
        return first;
    }
    for n in 1.. {
        let candidate = if n == 1 {
            format!("{name} (restored)")
        } else {
            format!("{name} (restored {n})")
        };
        let path = dst_dir.join(candidate);
        if !path.exists() {
            return path;
        }
    }
    unreachable!()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
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

fn percent_decode(s: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let chars = s.as_bytes();
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
    String::from_utf8(bytes).ok()
}

fn local_deletion_date_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trash_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("arca_xdg_trash_test_{}", std::process::id()));
        let victim = tmp.join("test file.txt");
        let root = tmp.join("TrashRoot");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(&victim, b"hello trash").unwrap();

        let n = trash_paths(&[&victim], Some(&root)).unwrap();
        assert_eq!(n, 1);
        assert!(!victim.exists());

        let items = list_trash_in_root(&root).unwrap();
        assert_eq!(items.len(), 1);

        let restored = restore_trash_item(&items[0]).unwrap();
        assert_eq!(restored, victim);
        assert!(victim.exists());

        fs::remove_dir_all(&tmp).ok();
    }
}
