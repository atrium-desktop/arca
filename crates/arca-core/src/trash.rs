//! Move-to-trash following the freedesktop Trash specification (minimal
//! subset): files go to `$XDG_DATA_HOME/Trash/files/` with a matching
//! `.trashinfo` in `info/` recording the original absolute path and the
//! deletion date, so desktop tools can restore them.
//!
//! `DeletionDate` is written in UTC (the spec wants local time; without a
//! timezone database UTC is the honest approximation, and desktop tools
//! still parse it).

use std::io;
use std::path::{Path, PathBuf};

/// `~/.local/share/Trash` (or `$XDG_DATA_HOME/Trash`).
pub fn trash_root() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        format!("{home}/.local/share")
    });
    Path::new(&base).join("Trash")
}

/// Move every path into the trash under `root`. Returns the number of
/// entries trashed. On the first failure the already-trashed entries stay
/// trashed and the error is reported to the caller.
pub fn trash_paths<P: AsRef<Path>>(paths: &[P], root: &Path) -> io::Result<usize> {
    let files_dir = root.join("files");
    let info_dir = root.join("info");
    std::fs::create_dir_all(&files_dir)?;
    std::fs::create_dir_all(&info_dir)?;

    let mut trashed = 0;
    for path in paths {
        let path = path.as_ref();
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
            .to_string_lossy()
            .into_owned();
        let unique = unique_trash_name(&files_dir, &name);
        let target = files_dir.join(&unique);

        // Write the info file first so a crash never leaves an
        // unrestorable orphan in files/.
        let absolute = std::fs::canonicalize(path)?;
        let info = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            percent_encode(&absolute.to_string_lossy()),
            deletion_date_now()
        );
        std::fs::write(info_dir.join(format!("{unique}.trashinfo")), info)?;

        match std::fs::rename(path, &target) {
            Ok(()) => {}
            Err(_) => {
                // Cross-filesystem: copy then delete.
                if path.is_dir() {
                    copy_dir_recursive(path, &target)?;
                    std::fs::remove_dir_all(path)?;
                } else {
                    std::fs::copy(path, &target)?;
                    std::fs::remove_file(path)?;
                }
            }
        }
        trashed += 1;
    }
    Ok(trashed)
}

/// `name`, or `name.1`, `name.2`, … when taken inside `files_dir`.
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

/// Percent-encode everything except unreserved characters and `/`
/// (freedesktop spec: the Path value is URL-encoded).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SS` in UTC (see module docs).
fn deletion_date_now() -> String {
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
    std::fs::create_dir(dst)?;
    for item in std::fs::read_dir(src)? {
        let item = item?;
        let target = dst.join(item.file_name());
        if item.file_type()?.is_dir() {
            copy_dir_recursive(&item.path(), &target)?;
        } else {
            std::fs::copy(item.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arca-trash-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn trash_moves_file_and_writes_info() {
        let tmp = temp_fixture();
        let root = tmp.join("trash");
        let victim = tmp.join("delete me.txt");
        std::fs::write(&victim, b"bye").unwrap();

        let n = trash_paths(&[&victim], &root).unwrap();
        assert_eq!(n, 1);
        assert!(!victim.exists());

        let trashed = root.join("files/delete me.txt");
        assert_eq!(std::fs::read(&trashed).unwrap(), b"bye");

        let info = std::fs::read_to_string(root.join("info/delete me.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\n"));
        assert!(info.contains("Path="), "info records original path: {info}");
        // Space in the file name must be percent-encoded in the Path value.
        assert!(info.contains("delete%20me.txt"), "encoded: {info}");
        assert!(info.contains("DeletionDate=20"), "has a date: {info}");

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn trash_name_collisions_get_suffixes() {
        let tmp = temp_fixture();
        let root = tmp.join("trash");

        let a_dir = tmp.join("a");
        let b_dir = tmp.join("b");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let f1 = a_dir.join("same.txt");
        let f2 = b_dir.join("same.txt");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        trash_paths(&[&f1], &root).unwrap();
        trash_paths(&[&f2], &root).unwrap();

        assert!(root.join("files/same.txt").exists());
        assert!(root.join("files/same.txt.1").exists());
        assert!(root.join("info/same.txt.trashinfo").exists());
        assert!(root.join("info/same.txt.1.trashinfo").exists());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn trash_directory_recursively() {
        let tmp = temp_fixture();
        let root = tmp.join("trash");
        // Same filesystem → rename path; but exercise with a directory.
        let dir = tmp.join("folder");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/f.txt"), b"x").unwrap();

        trash_paths(&[&dir], &root).unwrap();
        assert!(!dir.exists());
        assert!(root.join("files/folder/nested/f.txt").exists());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("/home/u/a b#c.txt"), "/home/u/a%20b%23c.txt");
        assert_eq!(percent_encode("/plain/path.txt"), "/plain/path.txt");
    }
}
