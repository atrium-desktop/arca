//! File operations: open with the desktop default app, create folders,
//! rename, and copy/move for paste. All functions return `io::Result` and
//! leave user-facing wording to the caller.

use std::io;
use std::path::{Path, PathBuf};

/// Open `path` with the desktop's default application (`mimeapps.list` / `.desktop` / `xdg-open`),
/// detached — Arca never blocks on the child.
pub fn open_with_token(path: &str, token: Option<&str>) -> io::Result<()> {
    let _span = crate::telemetry::span("launch.open");
    let result = crate::desktop::open_path_with_token(Path::new(path), token);
    match &result {
        Ok(()) => log::info!(target: "arca_engine::ops", "opened {path}"),
        Err(error) => log::warn!(target: "arca_engine::ops", "cannot open {path}: {error}"),
    }
    result
}

/// Open `path` with the desktop's default application.
pub fn open(path: &str) -> io::Result<()> {
    open_with_token(path, None)
}

/// Create `parent/New Folder` (or `"New Folder 2"`, … on collision) and
/// return its path.
pub fn create_folder(parent: &str) -> io::Result<PathBuf> {
    for candidate in
        std::iter::once("New Folder".to_string()).chain((2..).map(|n| format!("New Folder {n}")))
    {
        let path = Path::new(parent).join(&candidate);
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Rename (move) a file or directory. `to` must not exist.
///
/// Existence is probed with [`std::fs::symlink_metadata`]: `Path::exists`
/// follows links and therefore reports `false` for a dangling symlink, which
/// would let `rename` silently replace it.
pub fn rename_path(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(to) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::rename(from, to),
        Err(error) => Err(error),
    }
}

/// Permanently delete a file, directory, or symlink.
///
/// The type check uses [`std::fs::symlink_metadata`] so a symlink is always
/// unlinked as a link: following it would delete the *target* tree instead of
/// the entry the user selected.
pub fn delete_path(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Reject `src` → `dst_dir` transfers that would place a directory inside
/// itself (or "into" itself under another name).
///
/// Without this guard a recursive copy descends into the copy it is still
/// writing: the tree grows until the kernel rejects the path with
/// `ENAMETOOLONG`, leaving hundreds of nested directories on disk. Both
/// endpoints are canonicalized so symlinked routes (and `..` segments) are
/// caught as well. Only directories can contain themselves, so files skip
/// the check entirely.
pub(crate) fn ensure_transfer_allowed(src: &Path, dst_dir: &Path) -> io::Result<()> {
    ensure_not_within(src, dst_dir)
}

fn ensure_not_within(src: &Path, dst_dir: &Path) -> io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let src = canonical(src);
    let dst = canonical(dst_dir);
    if dst == src || dst.starts_with(&src) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot copy or move '{}' inside itself", src.display()),
        ));
    }
    Ok(())
}

/// Copy `src` (file or directory, recursively) into `dst_dir`, choosing a
/// non-clobbering name ("name (copy)", "name (copy 2)", …) on collision.
/// Returns the created path.
pub fn copy_into(src: &Path, dst_dir: &Path) -> io::Result<PathBuf> {
    ensure_not_within(src, dst_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no file name"))?;
    let dst = unique_destination(dst_dir, &name.to_string_lossy());
    if src.is_dir() {
        copy_dir_recursive(src, &dst)?;
    } else {
        std::fs::copy(src, &dst)?;
    }
    Ok(dst)
}

/// Move `src` into `dst_dir`. Falls back to copy+delete across filesystems.
pub fn move_into(src: &Path, dst_dir: &Path) -> io::Result<PathBuf> {
    ensure_not_within(src, dst_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no file name"))?;
    let dst = unique_destination(dst_dir, &name.to_string_lossy());
    match std::fs::rename(src, &dst) {
        Ok(()) => Ok(dst),
        Err(_) => {
            // Cross-device link error (or rename unsupported): copy then remove.
            if src.is_dir() {
                copy_dir_recursive(src, &dst)?;
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::copy(src, &dst)?;
                std::fs::remove_file(src)?;
            }
            Ok(dst)
        }
    }
}

/// `dst_dir/name`, or `dst_dir/name (copy)[ N]` when taken.
pub fn unique_destination(dst_dir: &Path, name: &str) -> PathBuf {
    let first = dst_dir.join(name);
    if !first.exists() {
        return first;
    }
    for n in 1.. {
        let candidate = if n == 1 {
            format!("{name} (copy)")
        } else {
            format!("{name} (copy {n})")
        };
        let path = dst_dir.join(candidate);
        if !path.exists() {
            return path;
        }
    }
    unreachable!()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::create_dir(dst)?;
    for item in std::fs::read_dir(src)? {
        let item = item?;
        let target = dst.join(item.file_name());
        let ty = item.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&item.path(), &target)?;
        } else if ty.is_symlink() {
            let link = std::fs::read_link(item.path())?;
            std::os::unix::fs::symlink(link, &target)?;
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
            "arca-ops-test-{}-{}",
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
    fn create_folder_picks_unique_names() {
        let tmp = temp_fixture();
        let a = create_folder(tmp.to_str().unwrap()).unwrap();
        let b = create_folder(tmp.to_str().unwrap()).unwrap();
        assert_eq!(a.file_name().unwrap(), "New Folder");
        assert_eq!(b.file_name().unwrap(), "New Folder 2");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn rename_refuses_to_clobber() {
        let tmp = temp_fixture();
        let a = tmp.join("a.txt");
        let b = tmp.join("b.txt");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        assert!(rename_path(&a, &b).is_err());
        rename_path(&a, &tmp.join("c.txt")).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(tmp.join("c.txt")).unwrap(), b"a");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn copy_into_handles_collisions_and_dirs() {
        let tmp = temp_fixture();
        let src = tmp.join("data");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("f.txt"), b"hello").unwrap();

        let dst_dir = tmp.join("out");
        std::fs::create_dir(&dst_dir).unwrap();

        let first = copy_into(&src, &dst_dir).unwrap();
        let second = copy_into(&src, &dst_dir).unwrap();
        assert_eq!(first.file_name().unwrap(), "data");
        assert_eq!(second.file_name().unwrap(), "data (copy)");
        assert_eq!(
            std::fs::read(second.join("f.txt")).unwrap(),
            b"hello",
            "recursive copy must carry contents"
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn move_into_moves_within_same_fs() {
        let tmp = temp_fixture();
        let src = tmp.join("m.txt");
        std::fs::write(&src, b"m").unwrap();
        let dst_dir = tmp.join("d");
        std::fs::create_dir(&dst_dir).unwrap();
        let dst = move_into(&src, &dst_dir).unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(dst).unwrap(), b"m");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// Copying or moving a directory into itself used to recurse until the
    /// kernel returned `ENAMETOOLONG`, leaving hundreds of nested copies on
    /// disk. The guard must reject it before any I/O happens.
    #[test]
    fn copy_and_move_reject_self_descendant_targets() {
        let tmp = temp_fixture();
        let src = tmp.join("outer");
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("f.txt"), b"x").unwrap();

        for target in [src.clone(), src.join("inner")] {
            let copy = copy_into(&src, &target);
            assert!(
                copy.is_err(),
                "copy into {target:?} must be rejected, got {copy:?}"
            );
            let moved = move_into(&src, &target);
            assert!(
                moved.is_err(),
                "move into {target:?} must be rejected, got {moved:?}"
            );
        }

        // Nothing was created and the source is untouched.
        assert!(src.join("f.txt").exists());
        assert_eq!(
            std::fs::read_dir(src.join("inner")).unwrap().count(),
            0,
            "no nested copies may be created"
        );

        // A sibling directory is still a legal target.
        let sibling = tmp.join("sibling");
        std::fs::create_dir(&sibling).unwrap();
        assert!(copy_into(&src, &sibling).is_ok());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// `delete_path` must unlink a symlink rather than recursively deleting
    /// the directory it points at.
    #[test]
    fn delete_path_unlinks_symlink_not_its_target() {
        let tmp = temp_fixture();
        let target = tmp.join("real");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();
        let link = tmp.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        delete_path(&link).unwrap();

        assert!(!link.exists(), "the link itself must be gone");
        assert!(
            target.join("keep.txt").exists(),
            "the symlink target must survive"
        );

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// A dangling symlink occupies the destination name, so renaming onto it
    /// must fail instead of silently replacing it.
    #[test]
    fn rename_refuses_dangling_symlink_destination() {
        let tmp = temp_fixture();
        let src = tmp.join("src.txt");
        std::fs::write(&src, b"s").unwrap();
        let dangling = tmp.join("dangling");
        std::os::unix::fs::symlink(tmp.join("nowhere"), &dangling).unwrap();

        assert!(
            rename_path(&src, &dangling).is_err(),
            "a dangling symlink still occupies the name"
        );
        assert!(src.exists(), "the source must be untouched");
        assert!(std::fs::symlink_metadata(&dangling).is_ok());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
