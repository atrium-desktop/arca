//! The iris-owned flux device, captured from the aperture lifecycle start callback.
//!
//! Both the icon glyph store and the thumbnail store upload textures through
//! it. `None` in headless tests or after shutdown, where upload paths are skipped
//! and widgets fall back to same-sized empty boxes.

use std::sync::atomic::{AtomicPtr, Ordering};

static DEVICE: AtomicPtr<flux_sys::flux_device> = AtomicPtr::new(std::ptr::null_mut());

/// Called from the aperture lifecycle start callback (see `lib.rs::run`).
pub(crate) fn set_device(device: *mut flux_sys::flux_device) {
    DEVICE.store(device, Ordering::Release);
}

/// Called from the aperture lifecycle stop callback during teardown.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The device slot is process-global and shared by two threads (the
    /// aperture lifecycle callback and the UI thread), swapped during startup and
    /// teardown. Its contract is narrow but load-bearing: until a start
    /// callback runs there is no device, and after a stop callback there must
    /// not be one — a stale pointer would be dereferenced for a texture
    /// upload during teardown.
    ///
    /// These tests serialize on a mutex because the slot is global state; the
    /// rest of the suite never touches it (headless tests take the `None`
    /// path), so a poisoned lock is the only interaction to guard against.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn device_multiplexes_a_null_and_a_live_pointer() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        clear_device();
        assert!(device().is_none(), "no device outside a lifecycle");

        // A dangling-but-non-null address is enough: the accessors only move
        // the pointer and test it for null, they never dereference it.
        let fake = 0x1000usize as *mut flux_sys::flux_device;
        set_device(fake);
        assert_eq!(device(), Some(fake), "the pointer is handed back verbatim");

        clear_device();
        assert!(device().is_none(), "teardown must reset the slot");
    }

    /// Clearing twice is harmless and idempotent (stop handlers can run for
    /// windows that never received a start callback).
    #[test]
    fn clearing_repeatedly_is_idempotent() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        clear_device();
        clear_device();
        assert!(device().is_none());
    }
}
