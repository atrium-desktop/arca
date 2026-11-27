//! Process-environment isolation for tests.
//!
//! Arca reads XDG variables at call time (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
//! `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR`). A test that points them at a scratch
//! directory must restore the previous values afterwards and must not race a
//! sibling test in the same process.
//!
//! [`lock`] serializes every mutation. Under `cargo test` several tests share
//! one process; under nextest each test gets its own process, where the lock
//! is uncontended.

use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the process-wide environment lock.
///
/// A panicking test can poison the mutex. The guard restore already ran, so
/// recovering the lock is correct and keeps later tests running instead of
/// cascading into spurious failures.
pub fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Set (`Some`) or remove (`None`) one environment variable.
///
/// Every caller holds the [`lock`] guard, so writes made through this module
/// never race each other.
fn write_env(key: &str, value: Option<&OsStr>) {
    // SAFETY: an environment write is sound only while no other thread reads
    // or writes the environment. That includes `std::env::var`,
    // `std::env::temp_dir` (`TMPDIR`) and libc `getenv` in C code. `ENV_LOCK`
    // serialises the writes made through this module, but readers do not
    // take it. Under plain `cargo test`, a sibling test thread can therefore
    // race a write: within this crate, `workspace::target_dir` reads
    // `CARGO_TARGET_DIR` and the `fs` fixtures read `TMPDIR`. This is sound
    // only under a process-per-test runner. ADR-0019 makes nextest that
    // runner, and CI uses it. The precondition is documented, not enforced.
    unsafe {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

/// A restoring environment mutation. The variable is restored when the guard
/// drops, while the environment lock is still held.
pub struct EnvGuard {
    key: String,
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Set `key` to `value` for the guard's lifetime.
    pub fn set(key: impl Into<String>, value: impl AsRef<OsStr>) -> Self {
        let lock = lock();
        let key = key.into();
        let previous = std::env::var_os(&key);
        write_env(&key, Some(value.as_ref()));
        Self {
            key,
            previous,
            _lock: lock,
        }
    }

    /// Remove `key` for the guard's lifetime.
    pub fn unset(key: impl Into<String>) -> Self {
        let lock = lock();
        let key = key.into();
        let previous = std::env::var_os(&key);
        write_env(&key, None);
        Self {
            key,
            previous,
            _lock: lock,
        }
    }

    /// The variable name controlled by this guard.
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => write_env(&self.key, Some(&value)),
            None => write_env(&self.key, None),
        }
    }
}

/// Run `f` with `key` set to `value`, restoring the previous value afterwards.
pub fn with_var<T>(key: impl Into<String>, value: impl AsRef<OsStr>, f: impl FnOnce() -> T) -> T {
    let _guard = EnvGuard::set(key, value);
    f()
}

/// A set of XDG variables pointed at one scratch root.
///
/// Holds the process-environment lock for its whole lifetime, so tests using
/// it are serialized. Build it after creating every scratch subdirectory the
/// test needs; `arca-xdg` reads the variables lazily.
pub struct XdgIsolation {
    previous: Vec<(String, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl XdgIsolation {
    /// Point every XDG base directory at `root`.
    pub fn new(root: &std::path::Path) -> Self {
        Self::install(root, false)
    }

    /// [`Self::new`] plus `HOME = root`.
    ///
    /// Tests that exercise the FreeDesktop trash must move `HOME` onto the same
    /// filesystem as the scratch directory: `trash_root_for` sends a path to
    /// the per-mount `.Trash-<uid>` when its device differs from the home
    /// device. Without this, a scratch directory under `/tmp` (often tmpfs)
    /// bypasses the isolated `XDG_DATA_HOME/Trash` the test inspects.
    pub fn with_home(root: &std::path::Path) -> Self {
        Self::install(root, true)
    }

    fn install(root: &std::path::Path, isolate_home: bool) -> Self {
        let lock = lock();
        let mut previous = Vec::with_capacity(6);
        let mut names = vec![
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "runtime"),
            ("XDG_STATE_HOME", "state"),
        ];
        if isolate_home {
            names.push(("HOME", ""));
        }
        for (name, child) in names {
            let directory = if child.is_empty() {
                root.to_path_buf()
            } else {
                root.join(child)
            };
            std::fs::create_dir_all(&directory).expect("create XDG scratch directory");
            previous.push((name.to_string(), std::env::var_os(name)));
            write_env(name, Some(directory.as_os_str()));
        }
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for XdgIsolation {
    fn drop(&mut self) {
        for (key, previous) in self.previous.drain(..) {
            match previous {
                Some(value) => write_env(&key, Some(&value)),
                None => write_env(&key, None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_restores_previous_value() {
        let key = "ARCA_TEST_SUPPORT_ENV_RESTORE";
        let previous = std::env::var_os(key);
        {
            let _guard = EnvGuard::set(key, "inside");
            assert_eq!(std::env::var(key).as_deref(), Ok("inside"));
        }
        assert_eq!(std::env::var_os(key), previous);
    }

    #[test]
    fn guard_restores_unset_state() {
        let key = "ARCA_TEST_SUPPORT_ENV_UNSET";
        // Start from an unset variable, written under the lock like every
        // other mutation.
        {
            let _lock = lock();
            write_env(key, None);
        }
        {
            let _guard = EnvGuard::unset(key);
            assert!(std::env::var_os(key).is_none());
        }
        assert!(std::env::var_os(key).is_none());
    }
}
