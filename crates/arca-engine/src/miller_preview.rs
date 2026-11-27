//! Off-thread listing for the Miller preview column.
//!
//! The preview column shows the directory selected in the centre (active)
//! column. Its target changes on every cursor move, so reading it inside the
//! frame would put a `read_dir` per keystroke on the UI thread — the exact
//! stall ADR-0020 removed from navigation. The loader follows the same
//! request/poll contract: one named worker per request, a generation counter
//! so only the newest request can publish, and a synchronous mode for
//! deterministic tests and non-interactive embedders.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::entry::{self, Entry, SortKey};

/// What a listing depends on: a cached listing is reused only while all of
/// these still match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewKey {
    pub path: String,
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_ascending: bool,
}

/// A finished preview listing.
#[derive(Clone, Debug)]
pub struct MillerPreview {
    pub key: PreviewKey,
    /// The listing, or the read error rendered for the user.
    pub listing: Result<Vec<Entry>, String>,
}

struct Pending {
    key: PreviewKey,
    result: Receiver<Result<Vec<Entry>, String>>,
}

/// Request/poll loader holding at most one in-flight read and one result.
#[derive(Default)]
pub struct PreviewLoader {
    generation: Arc<AtomicU64>,
    pending: Option<Pending>,
    ready: Option<MillerPreview>,
}

impl PreviewLoader {
    /// Ensure a listing for `key` is ready or in flight. A request for a
    /// different key supersedes any older one. `asynchronous == false` reads
    /// inline (tests, embedders without a frame loop).
    pub fn request(&mut self, key: PreviewKey, asynchronous: bool) {
        if self.ready.as_ref().is_some_and(|ready| ready.key == key)
            || self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.key == key)
        {
            return;
        }
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending = None;
        if !asynchronous {
            let listing = read(&key);
            self.ready = Some(MillerPreview { key, listing });
            return;
        }
        let (tx, rx) = mpsc::channel();
        let handle = Arc::clone(&self.generation);
        let thread_key = key.clone();
        let spawned = std::thread::Builder::new()
            .name("arca-miller-preview".into())
            .spawn(move || {
                if handle.load(Ordering::Relaxed) != generation {
                    return;
                }
                let listing = {
                    let _span = crate::telemetry::span("miller.preview_read");
                    read(&thread_key)
                };
                if handle.load(Ordering::Relaxed) == generation {
                    let _ = tx.send(listing);
                }
            });
        match spawned {
            Ok(_) => self.pending = Some(Pending { key, result: rx }),
            Err(_) => {
                let listing = read(&key);
                self.ready = Some(MillerPreview { key, listing });
            }
        }
    }

    /// Forget the request and any result, e.g. when nothing navigable is
    /// selected. A worker still running finds its generation stale.
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.pending = None;
        self.ready = None;
    }

    /// Collect a finished read without blocking. Returns true when a new
    /// result was published.
    pub fn poll(&mut self) -> bool {
        let Some(pending) = self.pending.take() else {
            return false;
        };
        match pending.result.try_recv() {
            Ok(listing) => {
                self.ready = Some(MillerPreview {
                    key: pending.key,
                    listing,
                });
                true
            }
            Err(TryRecvError::Empty) => {
                self.pending = Some(pending);
                false
            }
            Err(TryRecvError::Disconnected) => {
                self.ready = Some(MillerPreview {
                    key: pending.key,
                    listing: Err("Preview unavailable".into()),
                });
                true
            }
        }
    }

    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    /// The finished listing for `key`, if it is the one ready.
    #[must_use]
    pub fn get(&self, key: &PreviewKey) -> Option<&MillerPreview> {
        self.ready.as_ref().filter(|ready| &ready.key == key)
    }
}

fn read(key: &PreviewKey) -> Result<Vec<Entry>, String> {
    entry::read_dir(&key.path, key.show_hidden, key.sort_key, key.sort_ascending)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(path: &std::path::Path) -> PreviewKey {
        PreviewKey {
            path: path.to_string_lossy().into_owned(),
            show_hidden: false,
            sort_key: SortKey::Name,
            sort_ascending: true,
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("arca-miller-preview-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a").join("one.txt"), b"1").unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        dir
    }

    #[test]
    fn synchronous_request_is_ready_immediately() {
        let dir = scratch("sync");
        let mut loader = PreviewLoader::default();
        let k = key(&dir.join("a"));
        loader.request(k.clone(), false);
        let names: Vec<_> = loader
            .get(&k)
            .unwrap()
            .listing
            .as_ref()
            .unwrap()
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(names, ["one.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn asynchronous_request_publishes_only_the_newest() {
        let dir = scratch("async");
        let mut loader = PreviewLoader::default();
        let older = key(&dir.join("a"));
        let newer = key(&dir.join("b"));
        loader.request(older.clone(), true);
        loader.request(newer.clone(), true);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !loader.poll() {
            assert!(
                std::time::Instant::now() < deadline,
                "preview never published"
            );
            std::thread::yield_now();
        }
        assert!(loader.get(&older).is_none());
        assert!(
            loader
                .get(&newer)
                .unwrap()
                .listing
                .as_ref()
                .unwrap()
                .is_empty()
        );
        assert!(!loader.is_loading());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changed_options_invalidate_the_listing() {
        let dir = scratch("opts");
        let mut loader = PreviewLoader::default();
        let k = key(&dir.join("a"));
        loader.request(k.clone(), false);
        let hidden = PreviewKey {
            show_hidden: true,
            ..k.clone()
        };
        assert!(loader.get(&hidden).is_none());
        loader.clear();
        assert!(loader.get(&k).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unreadable_directory_reports_an_error() {
        let mut loader = PreviewLoader::default();
        let k = key(std::path::Path::new("/nonexistent/arca/preview"));
        loader.request(k.clone(), false);
        assert!(loader.get(&k).unwrap().listing.is_err());
    }
}
