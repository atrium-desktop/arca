//! GPU-side thumbnail cache for grid cards and the preview overlay.
//!
//! Decoding happens off-thread in `arca_engine::thumbs::ThumbService`; this
//! store drains finished decodes once per frame, premultiplies the pixels
//! (optics-gfx blends SRC_OVER) and uploads them as `optics_gfx_image`s. Only
//! cards that are actually built (the virtualized visible window) ever request
//! a thumbnail, so scrolling a large folder decodes lazily.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use arca_engine::thumbs::{Thumb, ThumbService};

use crate::device;

/// Mirrors the service-side cache cap: beyond this, oldest uploads are
/// released. `get`-based re-upload below covers any eviction-order skew
/// between the two stores.
const MAX_UPLOADS: usize = 512;

/// GPU upload time allowed per frame. Each upload is ~1 ms; entering a
/// cached folder can finish dozens of disk-cache hits at once, and uploading
/// them all in one frame produced 50-80 ms frames. The rest wait for the
/// next frame (newest first, as those are the rows just scrolled into view).
const UPLOAD_BUDGET: Duration = Duration::from_millis(4);

/// Extensions the core decoder can produce pixels for (embedded covers or
/// the image file itself).
pub(crate) fn is_thumbable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    ext != lower && matches!(ext, "flac" | "mp3" | "png" | "jpg" | "jpeg")
}

pub(crate) struct ThumbStore {
    service: ThumbService,
    images: HashMap<String, *mut optics_gfx_sys::optics_gfx_image>,
    order: VecDeque<String>,
    cwd: String,
    /// Mirrors `AppState::show_thumbnails`; when off, no decodes are
    /// requested and cards/overlays fall back to type glyphs.
    enabled: bool,
    /// Decoded thumbs waiting for an upload slot, oldest first.
    backlog: VecDeque<(String, Thumb)>,
    /// Start of this frame's upload budget (set by `drain_uploads`).
    budget_start: Instant,
}

impl ThumbStore {
    /// Production store backed by the FreeDesktop thumbnail cache.
    pub fn new() -> ThumbStore {
        Self::from_service(ThumbService::new())
    }

    /// Store backed by an explicit cache directory.
    ///
    /// Embedders and tests pass a scratch path: `ThumbService::new` starts a
    /// janitor that prunes the user's real `~/.cache/thumbnails`, which a test
    /// process must never do.
    pub fn with_cache_dir(cache_dir: std::path::PathBuf) -> ThumbStore {
        Self::from_service(ThumbService::with_cache_dir(cache_dir))
    }

    fn from_service(service: ThumbService) -> ThumbStore {
        ThumbStore {
            service,
            images: HashMap::new(),
            order: VecDeque::new(),
            cwd: String::new(),
            enabled: true,
            backlog: VecDeque::new(),
            budget_start: Instant::now(),
        }
    }

    /// See [`ThumbService::set_wake_fn`].
    pub fn set_wake_fn(&mut self, wake: Option<arca_engine::watch::WakeFn>) {
        self.service.set_wake_fn(wake);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Directory changes drop queued decodes (the visible window moved);
    /// decoded caches survive — re-entering a folder is instant.
    pub fn sync_cwd(&mut self, cwd: &str) {
        if self.cwd != cwd {
            self.cwd.clear();
            self.cwd.push_str(cwd);
            self.service.clear_pending();
        }
    }

    /// Move finished decodes onto the GPU within this frame's
    /// [`UPLOAD_BUDGET`], newest first; the rest stay queued and another
    /// frame is requested. Called once per frame, before the build. Without a
    /// device (headless) nothing was ever requested, so this is a no-op.
    pub fn drain_uploads(&mut self) {
        if device::device().is_none() {
            return;
        }
        self.budget_start = Instant::now();
        self.backlog.extend(self.service.drain());
        while !self.budget_spent() {
            let Some((path, thumb)) = self.backlog.pop_back() else {
                break;
            };
            if let Some(image) = upload(&thumb) {
                self.insert(path, image);
            }
        }
        if !self.backlog.is_empty() {
            arca_engine::telemetry::counter("ui.thumb_upload_deferred", self.backlog.len() as u64);
            optics_window::request_animation_frame();
        }
    }

    fn budget_spent(&self) -> bool {
        self.budget_start.elapsed() >= UPLOAD_BUDGET
    }

    /// The uploaded image for `path`, requesting a decode on miss. Returns
    /// None headless, while decoding, for files without artwork, or when
    /// thumbnails are disabled.
    pub fn image_for(&mut self, path: &str) -> Option<*mut optics_gfx_sys::optics_gfx_image> {
        if !self.enabled {
            return None;
        }
        device::device()?;
        if let Some(&image) = self.images.get(path) {
            return Some(image);
        }
        // Decoded earlier but evicted from the GPU side: re-upload from the
        // service cache (also touches its LRU recency).
        if self.service.get(path).is_some() && self.budget_spent() {
            // Re-uploads share the frame budget; retry next frame.
            optics_window::request_animation_frame();
            return None;
        }
        if let Some(thumb) = self.service.get(path) {
            let thumb = thumb.clone();
            if let Some(image) = upload(&thumb) {
                self.insert(path.to_string(), image);
                return self.images.get(path).copied();
            }
            return None;
        }
        self.service.request(path);
        None
    }

    fn insert(&mut self, path: String, image: *mut optics_gfx_sys::optics_gfx_image) {
        if self.images.insert(path.clone(), image).is_some() {
            self.order.retain(|p| p != &path);
        }
        self.order.push_back(path);
        while self.order.len() > MAX_UPLOADS {
            if let Some(oldest) = self.order.pop_front()
                && let Some(image) = self.images.remove(&oldest)
            {
                // SAFETY: the device is alive during the run.
                unsafe { optics_gfx_sys::optics_gfx_image_release(image) };
            }
        }
    }

    /// Explicitly release all uploaded GPU textures. Called on shutdown via the
    /// optics-app lifecycle stop hook before device teardown (ADR-0045).
    pub fn release_all(&mut self) {
        for (_, image) in self.images.drain() {
            // SAFETY: called while device is alive.
            unsafe { optics_gfx_sys::optics_gfx_image_release(image) };
        }
        self.order.clear();
    }
}

impl Drop for ThumbStore {
    fn drop(&mut self) {
        if device::device().is_some() {
            self.release_all();
        }
    }
}

/// Upload one decode using the optics-gfx premultiplied region upload.
fn upload(thumb: &Thumb) -> Option<*mut optics_gfx_sys::optics_gfx_image> {
    let device = device::device()?;
    let _span = arca_engine::telemetry::span("ui.thumb_upload");
    let desc = optics_gfx_sys::optics_gfx_image_desc {
        type_: optics_gfx_sys::optics_gfx_struct_type::OPTICS_GFX_TYPE_IMAGE_DESC,
        next: std::ptr::null(),
        width: thumb.width,
        height: thumb.height,
        format: optics_gfx_sys::optics_gfx_format::OPTICS_GFX_FORMAT_RGBA8_UNORM,
        initial_data: std::ptr::null(),
        usage: optics_gfx_sys::optics_gfx_image_usage::OPTICS_GFX_IMAGE_USAGE_SAMPLED,
    };
    let mut out = std::ptr::null_mut();
    // SAFETY: device is the live optics-window device.
    let rc = unsafe { optics_gfx_sys::optics_gfx_image_create(device, &desc, &mut out) };
    if rc != optics_gfx_sys::optics_gfx_result::OPTICS_GFX_OK || out.is_null() {
        return None;
    }
    let update = optics_gfx_sys::optics_gfx_image_update_region_opts {
        x: 0,
        y: 0,
        width: thumb.width,
        height: thumb.height,
        data: thumb.rgba.as_ptr().cast(),
        row_bytes: 0,
        bytes: thumb.rgba.len(),
        premultiply: true,
    };
    // SAFETY: out is valid and newly allocated; thumb.rgba holds straight RGBA8 pixels.
    let rc = unsafe { optics_gfx_sys::optics_gfx_image_update_region(out, &update) };
    if rc != optics_gfx_sys::optics_gfx_result::OPTICS_GFX_OK {
        // SAFETY: device is alive during the run.
        unsafe { optics_gfx_sys::optics_gfx_image_release(out) };
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dispatch list must stay in sync with the engine decoder, which
    /// only compiles PNG and JPEG support (`arca-engine`'s `image` features).
    #[test]
    fn thumbable_matches_the_decoder_feature_set() {
        for name in [
            "cover.flac",
            "track.mp3",
            "photo.png",
            "photo.jpg",
            "photo.jpeg",
            // Case-insensitive, like the decoder's extension check.
            "PHOTO.PNG",
            "Cover.FLAC",
        ] {
            assert!(is_thumbable(name), "{name} must be decodable");
        }

        for name in [
            // Formats the `image` crate could decode but that are not compiled
            // in, so they must not be requested.
            "photo.gif",
            "photo.webp",
            "photo.bmp",
            "photo.tiff",
            "document.pdf",
            "notes.txt",
            // No extension at all (also guards the empty-extension case).
            "README",
            "archive.tar.gz",
            // A leading dot is not an extension.
            ".hidden",
        ] {
            assert!(!is_thumbable(name), "{name} must not be requested");
        }
    }

    /// A directory change drops queued work but keeps decoded pixels, so
    /// returning to a folder is instant.
    #[test]
    fn sync_cwd_drops_pending_work_only() {
        let dir = crate::test_support::ScratchDir::new("arca-thumb-store");
        let mut store = ThumbStore::with_cache_dir(dir.path().join("cache"));
        store.sync_cwd("/a");
        store.sync_cwd("/a");
        assert_eq!(store.cwd, "/a");

        store.sync_cwd("/b");
        assert_eq!(store.cwd, "/b");
    }

    /// Without a GPU device (headless tests), requesting a thumbnail must be
    /// a harmless no-op rather than a null-pointer upload.
    #[test]
    fn image_for_without_a_device_returns_none() {
        let dir = crate::test_support::ScratchDir::new("arca-thumb-none");
        let mut store = ThumbStore::with_cache_dir(dir.path().join("cache"));
        store.sync_cwd("/tmp");
        assert!(store.image_for("/tmp/anything.png").is_none());
    }
}
