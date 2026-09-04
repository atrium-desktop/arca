//! The iris-owned flux device, captured once from the start callback.
//!
//! Both the icon glyph store and the thumbnail store upload textures through
//! it. `None` in headless tests, where upload paths are skipped and widgets
//! fall back to same-sized empty boxes.

use std::sync::OnceLock;

struct DevicePtr(*mut lens_sys::flux_device);
// lens/flux calls all happen on iris's single UI thread.
unsafe impl Send for DevicePtr {}
unsafe impl Sync for DevicePtr {}

static DEVICE: OnceLock<DevicePtr> = OnceLock::new();

/// Called from the iris start callback (see `lib.rs::run`).
pub(crate) fn set_device(device: *mut std::ffi::c_void) {
    let _ = DEVICE.set(DevicePtr(device as *mut lens_sys::flux_device));
}

pub(crate) fn device() -> Option<*mut lens_sys::flux_device> {
    DEVICE.get().map(|d| d.0)
}
