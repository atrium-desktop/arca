//! File operations: open with the desktop default app, create folders,
//! rename, and copy/move for paste. All functions return `io::Result` and
//! leave user-facing wording to the caller.

use std::io;
use std::path::{Path, PathBuf};

/// Open `path` with the desktop's default application (`xdg-open`),
/// detached — Arca never blocks on the child.
pub fn open(path: &str) -> io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
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
pub fn rename_path(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    std::fs::rename(from, to)
}

/// Copy `src` (file or directory, recursively) into `dst_dir`, choosing a
/// non-clobbering name ("name (copy)", "name (copy 2)", …) on collision.
/// Returns the created path.
pub fn copy_into(src: &Path, dst_dir: &Path) -> io::Result<PathBuf> {
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
}
