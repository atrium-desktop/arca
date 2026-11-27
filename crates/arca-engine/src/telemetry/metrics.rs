//! Counters, gauges, and latency histograms (the APM data model).
//!
//! A [`Registry`] is a plain value so tests can own one; the process-wide
//! instance is [`global`], which the free functions [`counter`], [`gauge`]
//! and [`observe`] write to. Histograms use fixed bucket bounds chosen around
//! the UI's frame budget (16.7 ms) and human-perceptible pauses (100 ms+), so
//! a snapshot answers "how often did this take longer than a frame / a
//! second?" without storing samples.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Upper bucket bounds in microseconds; one implicit overflow bucket follows.
pub const BUCKET_BOUNDS_US: [u64; 16] = [
    100, 250, 500, 1_000, 2_500, 5_000, 10_000, 16_667, 33_333, 50_000, 100_000, 250_000, 500_000,
    1_000_000, 2_500_000, 10_000_000,
];

/// A latency distribution with fixed buckets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Histogram {
    buckets: [u64; BUCKET_BOUNDS_US.len() + 1],
    count: u64,
    sum_us: u64,
    max_us: u64,
}

impl Histogram {
    pub fn record(&mut self, elapsed: Duration) {
        let us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        let index = BUCKET_BOUNDS_US
            .iter()
            .position(|&bound| us <= bound)
            .unwrap_or(BUCKET_BOUNDS_US.len());
        self.buckets[index] += 1;
        self.count += 1;
        self.sum_us = self.sum_us.saturating_add(us);
        self.max_us = self.max_us.max(us);
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Upper bound of the bucket holding quantile `q` (0..=1), capped by the
    /// observed maximum so a single sample reports its real value's bucket
    /// rather than an absurd bound.
    #[must_use]
    pub fn quantile_us(&self, q: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let rank = ((q.clamp(0.0, 1.0) * self.count as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (index, &n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                let bound = BUCKET_BOUNDS_US.get(index).copied().unwrap_or(self.max_us);
                return bound.min(self.max_us);
            }
        }
        self.max_us
    }

    #[must_use]
    pub fn summary(&self) -> HistogramSummary {
        // Microsecond resolution: exact in JSON and all a reader needs.
        let ms = |us: u64| us as f64 / 1000.0;
        HistogramSummary {
            count: self.count,
            sum_ms: ms(self.sum_us),
            mean_ms: ms(self.sum_us.checked_div(self.count).unwrap_or(0)),
            p50_ms: ms(self.quantile_us(0.50)),
            p95_ms: ms(self.quantile_us(0.95)),
            p99_ms: ms(self.quantile_us(0.99)),
            max_ms: ms(self.max_us),
        }
    }
}

/// Serializable view of a [`Histogram`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistogramSummary {
    pub count: u64,
    pub sum_ms: f64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

/// Point-in-time copy of every metric.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub counters: BTreeMap<String, u64>,
    pub gauges: BTreeMap<String, i64>,
    pub histograms: BTreeMap<String, HistogramSummary>,
}

#[derive(Default)]
struct Inner {
    counters: BTreeMap<String, u64>,
    gauges: BTreeMap<String, i64>,
    histograms: BTreeMap<String, Histogram>,
}

/// Thread-safe metric store.
#[derive(Default)]
pub struct Registry {
    inner: Mutex<Inner>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Registry {
        Registry::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        // A panic while holding the lock leaves plain numbers behind, which
        // are still meaningful; keep recording instead of going dark.
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut inner)
    }

    pub fn add(&self, name: &str, delta: u64) {
        self.with(|inner| {
            if let Some(value) = inner.counters.get_mut(name) {
                *value = value.saturating_add(delta);
            } else {
                inner.counters.insert(name.to_string(), delta);
            }
        });
    }

    pub fn set(&self, name: &str, value: i64) {
        self.with(|inner| {
            inner.gauges.insert(name.to_string(), value);
        });
    }

    pub fn record(&self, name: &str, elapsed: Duration) {
        self.with(|inner| {
            if let Some(histogram) = inner.histograms.get_mut(name) {
                histogram.record(elapsed);
            } else {
                let mut histogram = Histogram::default();
                histogram.record(elapsed);
                inner.histograms.insert(name.to_string(), histogram);
            }
        });
    }

    #[must_use]
    pub fn counter_value(&self, name: &str) -> u64 {
        self.with(|inner| inner.counters.get(name).copied().unwrap_or(0))
    }

    #[must_use]
    pub fn gauge_value(&self, name: &str) -> Option<i64> {
        self.with(|inner| inner.gauges.get(name).copied())
    }

    #[must_use]
    pub fn histogram(&self, name: &str) -> Option<Histogram> {
        self.with(|inner| inner.histograms.get(name).cloned())
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.with(|inner| Snapshot {
            counters: inner.counters.clone(),
            gauges: inner.gauges.clone(),
            histograms: inner
                .histograms
                .iter()
                .map(|(name, histogram)| (name.clone(), histogram.summary()))
                .collect(),
        })
    }
}

/// The process-wide registry.
pub fn global() -> &'static Registry {
    static GLOBAL: OnceLock<Registry> = OnceLock::new();
    GLOBAL.get_or_init(Registry::new)
}

/// Add `delta` to the global counter `name`.
pub fn counter(name: &str, delta: u64) {
    global().add(name, delta);
}

/// Set the global gauge `name`.
pub fn gauge(name: &str, value: i64) {
    global().set(name, value);
}

/// Record one latency sample into the global histogram `name`.
pub fn observe(name: &str, elapsed: Duration) {
    global().record(name, elapsed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn empty_histogram_reports_zeroes() {
        let histogram = Histogram::default();
        assert_eq!(histogram.quantile_us(0.5), 0);
        assert_eq!(histogram.summary(), HistogramSummary::default());
    }

    #[test]
    fn quantiles_follow_bucket_bounds_and_cap_at_max() {
        let mut histogram = Histogram::default();
        for _ in 0..90 {
            histogram.record(ms(1)); // <= 1 ms bucket
        }
        for _ in 0..10 {
            histogram.record(ms(300)); // <= 500 ms bucket
        }
        assert_eq!(histogram.count(), 100);
        assert_eq!(histogram.quantile_us(0.50), 1_000);
        assert_eq!(histogram.quantile_us(0.90), 1_000);
        // p95 falls in the 500 ms bucket but is capped at the real maximum.
        assert_eq!(histogram.quantile_us(0.95), 300_000);
        let summary = histogram.summary();
        assert_eq!(summary.max_ms, 300.0);
        assert_eq!(summary.mean_ms, 30.9, "{summary:?}");
    }

    #[test]
    fn samples_beyond_the_last_bound_land_in_overflow() {
        let mut histogram = Histogram::default();
        histogram.record(Duration::from_secs(30));
        assert_eq!(histogram.quantile_us(0.99), 30_000_000);
    }

    #[test]
    fn registry_accumulates_and_snapshots() {
        let registry = Registry::new();
        registry.add("ops", 2);
        registry.add("ops", 3);
        registry.set("tabs", 4);
        registry.set("tabs", 1);
        registry.record("nav", ms(5));
        assert_eq!(registry.counter_value("ops"), 5);
        assert_eq!(registry.counter_value("missing"), 0);
        assert_eq!(registry.gauge_value("tabs"), Some(1));

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.counters["ops"], 5);
        assert_eq!(snapshot.histograms["nav"].count, 1);

        let json = serde_json::to_string(&snapshot).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snapshot);
    }

    #[test]
    fn registry_is_safe_under_concurrent_writers() {
        let registry = std::sync::Arc::new(Registry::new());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let registry = registry.clone();
                std::thread::spawn(move || {
                    for _ in 0..1000 {
                        registry.add("hits", 1);
                        registry.record("lat", Duration::from_micros(10));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(registry.counter_value("hits"), 8000);
        assert_eq!(registry.histogram("lat").unwrap().count(), 8000);
    }
}
