//! Unique scratch directories with RAII cleanup.
//!
//! Tests must never write into the real configuration, cache, trash, or user
//! directories. [`TestDir`] owns a private subtree under the system temporary
//! directory and removes it on drop, including when a test panics.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes scratch directories created in the same clock tick. Test
/// harnesses run suites concurrently, so a nanosecond timestamp alone can
/// collide when two tests start together.
static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

/// A scratch directory removed when dropped.
#[derive(Debug)]
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    /// Create `arca-test-<prefix>-<pid>-<nanos>-<serial>/` under the system
    /// temporary directory.
    pub fn new(prefix: &str) -> Self {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "arca-test-{prefix}-{}-{nanos}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create scratch directory");
        Self { path }
    }

    /// The directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory as a UTF-8 string. Arca's engine API is string-based.
    pub fn str(&self) -> &str {
        self.path.to_str().expect("scratch path is UTF-8")
    }

    /// A path inside the directory (not created).
    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.path.join(relative)
    }

    /// Create a child directory and return its path.
    pub fn subdir(&self, relative: impl AsRef<Path>) -> PathBuf {
        let path = self.join(relative);
        std::fs::create_dir_all(&path).expect("create scratch subdirectory");
        path
    }

    /// Write a regular file, creating its parent directories.
    pub fn write(&self, relative: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create scratch file parent");
        }
        std::fs::write(&path, contents).expect("write scratch file");
        path
    }

    /// Sorted names of the direct children (directories and files).
    pub fn entry_names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.path)
            .expect("read scratch directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Number of direct children.
    pub fn entry_count(&self) -> usize {
        std::fs::read_dir(&self.path)
            .expect("read scratch directory")
            .count()
    }
}

impl Default for TestDir {
    fn default() -> Self {
        Self::new("tmp")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl std::ops::Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_cleans_a_unique_directory() {
        let first = TestDir::new("unique");
        let second = TestDir::new("unique");
        assert_ne!(first.path(), second.path());
        first.write("nested/a.txt", b"a");
        first.subdir("nested/b");
        assert_eq!(first.entry_names(), ["nested"]);
        let path = first.path().to_path_buf();
        drop(first);
        assert!(!path.exists(), "drop removes the scratch tree");
    }
}
