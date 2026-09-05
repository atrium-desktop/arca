//! Directory entries: reading, sorting and filtering.

use std::io;
use std::path::Path;

use crate::path;

/// What a directory entry points at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    Directory,
    Regular,
    Symlink,
    Other,
}

/// One row in the file list.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub file_type: FileType,
    pub size: u64,
    /// Seconds since the Unix epoch, 0 when unavailable.
    pub mtime: i64,
    pub hidden: bool,
    /// True for directories and for symlinks that resolve to a directory
    /// (so activating them navigates instead of opening).
    pub navigable: bool,
    /// The XDG MIME type of the entry (e.g. `image/png`, `text/plain`, `inode/directory`).
    pub mime_type: String,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.file_type == FileType::Directory
    }
}

/// Column the list is sorted by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Mtime,
}

impl SortKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Mtime => "mtime",
        }
    }

    pub fn parse(s: &str) -> Option<SortKey> {
        match s {
            "name" => Some(SortKey::Name),
            "size" => Some(SortKey::Size),
            "mtime" => Some(SortKey::Mtime),
            _ => None,
        }
    }
}

/// Read `path` and return its entries, sorted: directories first, then by
/// `key` (`ascending` controls direction). With `show_hidden` off, dotfiles
/// are omitted.
pub fn read_dir(
    path: &str,
    show_hidden: bool,
    key: SortKey,
    ascending: bool,
) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(path)? {
        let item = item?;
        let name: String = item.file_name().to_string_lossy().into();
        if name == "." || name == ".." {
            continue;
        }
        let hidden = name.starts_with('.');
        if hidden && !show_hidden {
            continue;
        }

        // DirEntry::metadata is lstat-like: a symlink reports itself.
        let meta = item.metadata()?;
        let file_type = if meta.is_dir() {
            FileType::Directory
        } else if meta.is_file() {
            FileType::Regular
        } else if meta.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Other
        };
        // One extra stat per symlink: does it resolve to a directory?
        let navigable = file_type == FileType::Directory
            || (file_type == FileType::Symlink
                && std::fs::metadata(item.path())
                    .map(|m| m.is_dir())
                    .unwrap_or(false));

        let mime_type = if navigable {
            "inode/directory".to_string()
        } else {
            crate::mime::guess_mime_type(&item.path())
        };

        entries.push(Entry {
            name,
            file_type,
            size: meta.len(),
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            hidden,
            navigable,
            mime_type,
        });
    }

    sort_entries(&mut entries, key, ascending);
    Ok(entries)
}

/// In-place sort: directories always before files, then the chosen key;
/// ties (and `Name`) fall back to case-insensitive name order.
pub fn sort_entries(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by(|a, b| {
        match (a.navigable, b.navigable) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Mtime => a.mtime.cmp(&b.mtime),
        };
        let ord = if ord == std::cmp::Ordering::Equal && key != SortKey::Name {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            ord
        };
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });
}

/// Indices of entries whose name contains `query` (case-insensitive).
/// An empty query matches everything.
pub fn filtered_indices(entries: &[Entry], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| q.is_empty() || e.name.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

/// Join a child `name` onto directory `path` (delegates to [`path::join`]).
pub fn child_path(dir: &str, name: &str) -> String {
    path::join(dir, name)
}

/// True when `path` exists and is a directory.
pub fn dir_exists(path: &str) -> bool {
    Path::new(path).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, ft: FileType, size: u64, mtime: i64) -> Entry {
        Entry {
            name: name.into(),
            file_type: ft,
            size,
            mtime,
            hidden: name.starts_with('.'),
            navigable: ft == FileType::Directory,
            mime_type: if ft == FileType::Directory {
                "inode/directory".into()
            } else {
                "text/plain".into()
            },
        }
    }

    #[test]
    fn sort_dirs_first_then_name() {
        let mut v = vec![
            entry("zed.txt", FileType::Regular, 1, 0),
            entry("beta", FileType::Directory, 0, 0),
            entry("Alpha", FileType::Directory, 0, 0),
            entry("aardvark.txt", FileType::Regular, 1, 0),
        ];
        sort_entries(&mut v, SortKey::Name, true);
        let names: Vec<&str> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "aardvark.txt", "zed.txt"]);
    }

    #[test]
    fn sort_by_size_descending_keeps_dirs_first() {
        let mut v = vec![
            entry("big", FileType::Regular, 100, 0),
            entry("dir", FileType::Directory, 0, 0),
            entry("small", FileType::Regular, 1, 0),
        ];
        sort_entries(&mut v, SortKey::Size, false);
        let names: Vec<&str> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["dir", "big", "small"]);
    }

    #[test]
    fn sort_by_mtime_ascending() {
        let mut v = vec![
            entry("new", FileType::Regular, 0, 200),
            entry("old", FileType::Regular, 0, 100),
        ];
        sort_entries(&mut v, SortKey::Mtime, true);
        assert_eq!(v[0].name, "old");
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let v = vec![
            entry("Notes.md", FileType::Regular, 0, 0),
            entry("TODO", FileType::Regular, 0, 0),
            entry("src", FileType::Directory, 0, 0),
        ];
        assert_eq!(filtered_indices(&v, "to"), vec![1]);
        assert_eq!(filtered_indices(&v, ""), vec![0, 1, 2]);
        assert_eq!(filtered_indices(&v, "zzz"), Vec::<usize>::new());
    }

    #[test]
    fn read_dir_hides_dotfiles_unless_asked() {
        let tmp = temp_fixture();
        std::fs::write(tmp.join("visible.txt"), b"x").unwrap();
        std::fs::write(tmp.join(".hidden"), b"x").unwrap();
        std::fs::create_dir(tmp.join("sub")).unwrap();

        let plain = read_dir(tmp.to_str().unwrap(), false, SortKey::Name, true).unwrap();
        assert_eq!(
            plain.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["sub", "visible.txt"]
        );

        let all = read_dir(tmp.to_str().unwrap(), true, SortKey::Name, true).unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|e| e.hidden));

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn read_dir_reports_error_for_missing_dir() {
        let missing = temp_fixture().join("nope");
        assert!(read_dir(missing.to_str().unwrap(), false, SortKey::Name, true).is_err());
    }

    #[test]
    fn symlink_to_dir_is_navigable() {
        let tmp = temp_fixture();
        std::fs::create_dir(tmp.join("real")).unwrap();
        std::os::unix::fs::symlink(tmp.join("real"), tmp.join("link")).unwrap();
        std::os::unix::fs::symlink(tmp.join("missing"), tmp.join("dangling")).unwrap();

        let entries = read_dir(tmp.to_str().unwrap(), false, SortKey::Name, true).unwrap();
        let link = entries.iter().find(|e| e.name == "link").unwrap();
        assert_eq!(link.file_type, FileType::Symlink);
        assert!(link.navigable);
        let dangling = entries.iter().find(|e| e.name == "dangling").unwrap();
        assert!(!dangling.navigable);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// Unique temp dir per test (no external crates).
    fn temp_fixture() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arca-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
