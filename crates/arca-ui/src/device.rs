//! The iris-owned flux device, captured from the lifecycle start callback.
//!
//! Both the icon glyph store and the thumbnail store upload textures through
//! it. `None` in headless tests or after shutdown, where upload paths are skipped
//! and widgets fall back to same-sized empty boxes.

use std::sync::atomic::{AtomicPtr, Ordering};

static DEVICE: AtomicPtr<flux_sys::flux_device> = AtomicPtr::new(std::ptr::null_mut());

/// Called from the iris lifecycle start callback (see `lib.rs::run`).
pub(crate) fn set_device(device: *mut flux_sys::flux_device) {
    DEVICE.store(device, Ordering::Release);
}

/// Called from the iris lifecycle stop callback during teardown.
pub(crate) fn clear_device() {
    DEVICE.store(std::ptr::null_mut(), Ordering::Release);
}

pub(crate) fn device() -> Option<*mut flux_sys::flux_device> {
    let ptr = DEVICE.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(ptr)
    }
}
