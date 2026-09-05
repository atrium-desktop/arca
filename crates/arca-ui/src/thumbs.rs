//! GPU-side thumbnail cache for grid cards and the preview overlay.
//!
//! Decoding happens off-thread in `arca_engine::thumbs::ThumbService`; this
//! store drains finished decodes once per frame, premultiplies the pixels
//! (flux blends SRC_OVER) and uploads them as `flux_image`s. Only cards that
//! are actually built (the virtualized visible window) ever request a
//! thumbnail, so scrolling a large folder decodes lazily.

use std::collections::{HashMap, VecDeque};

use arca_engine::thumbs::{Thumb, ThumbService};

use crate::device;

/// Mirrors the service-side cache cap: beyond this, oldest uploads are
/// released. `get`-based re-upload below covers any eviction-order skew
/// between the two stores.
const MAX_UPLOADS: usize = 512;

/// Extensions the core decoder can produce pixels for (embedded covers or
/// the image file itself).
pub(crate) fn is_thumbable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    ext != lower && matches!(ext, "flac" | "mp3" | "png" | "jpg" | "jpeg")
}

pub(crate) struct ThumbStore {
    service: ThumbService,
    images: HashMap<String, *mut lens_sys::flux_image>,
    order: VecDeque<String>,
    cwd: String,
    /// Mirrors `AppState::show_thumbnails`; when off, no decodes are
    /// requested and cards/overlays fall back to type glyphs.
    enabled: bool,
}

impl ThumbStore {
    pub fn new() -> ThumbStore {
        ThumbStore {
            service: ThumbService::new(),
            images: HashMap::new(),
            order: VecDeque::new(),
            cwd: String::new(),
            enabled: true,
        }
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

    /// Move finished decodes onto the GPU. Called once per frame; without a
    /// device (headless) decodes are discarded and nothing was ever
    /// requested, so this stays a no-op.
    pub fn drain_uploads(&mut self) {
        if device::device().is_none() {
            return;
        }
        for (path, thumb) in self.service.drain() {
            if let Some(image) = upload(&thumb) {
                self.insert(path, image);
            }
        }
    }

    /// The uploaded image for `path`, requesting a decode on miss. Returns
    /// None headless, while decoding, for files without artwork, or when
    /// thumbnails are disabled.
    pub fn image_for(&mut self, path: &str) -> Option<*mut lens_sys::flux_image> {
        if !self.enabled {
            return None;
        }
        device::device()?;
        if let Some(&image) = self.images.get(path) {
            return Some(image);
        }
        // Decoded earlier but evicted from the GPU side: re-upload from the
        // service cache (also touches its LRU recency).
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

    fn insert(&mut self, path: String, image: *mut lens_sys::flux_image) {
        if self.images.insert(path.clone(), image).is_some() {
            self.order.retain(|p| p != &path);
        }
        self.order.push_back(path);
        while self.order.len() > MAX_UPLOADS {
            if let Some(oldest) = self.order.pop_front() {
                if let Some(image) = self.images.remove(&oldest) {
                    // SAFETY: the device is alive during the run.
                    unsafe { flux_sys::flux_image_release(image) };
                }
            }
        }
    }
    // No Drop: by the time UiApp drops, iris has already torn the device
    // down, so releasing here could crash. The OS reclaims the textures.
}

/// Premultiply and upload one decode.
fn upload(thumb: &Thumb) -> Option<*mut lens_sys::flux_image> {
    let device = device::device()?;
    let mut pixels = thumb.rgba.clone();
    for px in pixels.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        px[0] = ((u32::from(px[0]) * a + 127) / 255) as u8;
        px[1] = ((u32::from(px[1]) * a + 127) / 255) as u8;
        px[2] = ((u32::from(px[2]) * a + 127) / 255) as u8;
    }
    let desc = flux_sys::flux_image_desc {
        type_: flux_sys::flux_struct_type::FLUX_TYPE_IMAGE_DESC,
        next: std::ptr::null(),
        width: thumb.width,
        height: thumb.height,
        format: flux_sys::flux_format::FLUX_FORMAT_RGBA8_UNORM,
        initial_data: pixels.as_ptr().cast(),
    };
    let mut out = std::ptr::null_mut();
    // SAFETY: device is iris's live device; desc points at valid texels.
    let rc = unsafe { flux_sys::flux_image_create(device, &desc, &mut out) };
    (rc == flux_sys::flux_result::FLUX_OK && !out.is_null()).then_some(out)
}
