//! Bounded polling for asynchronous effects.
//!
//! Arca's watcher, thumbnail pool, and IO worker are threads: a test asserts
//! their effect by waiting for observable state, never by sleeping a fixed
//! duration and hoping. Every helper is bounded, so a regression fails the
//! test instead of hanging the suite.

use std::time::{Duration, Instant};

/// Default upper bound for a filesystem/watcher/worker effect.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll `predicate` every 10 ms until it returns true or `timeout` elapses.
/// Returns whether the predicate became true.
pub fn wait_for(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// [`wait_for`] with [`DEFAULT_TIMEOUT`].
pub fn wait_until(mut predicate: impl FnMut() -> bool) -> bool {
    wait_for(DEFAULT_TIMEOUT, &mut predicate)
}

/// Sleep for `millis`, used only where the observable state is time-based
/// (double-click windows, debounce intervals).
pub fn sleep_ms(millis: u64) {
    std::thread::sleep(Duration::from_millis(millis));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_for_observes_a_later_condition() {
        let start = Instant::now();
        assert!(wait_for(Duration::from_secs(1), || {
            start.elapsed() >= Duration::from_millis(25)
        }));
    }

    #[test]
    fn wait_for_is_bounded() {
        let start = Instant::now();
        assert!(!wait_for(Duration::from_millis(30), || false));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
