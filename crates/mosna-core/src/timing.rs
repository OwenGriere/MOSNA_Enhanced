//! How long the computation itself took, apart from everything around it.
//!
//! A step's wall-clock time mixes the work with what surrounds it: checking the
//! files, reading them, writing the results, drawing the figures. A benchmark
//! of the algorithms needs the first without the rest, so the computing calls
//! are timed on their own and the times added up.
//!
//! The samples of a cohort are processed in parallel, so the total is the
//! *sum* of the time spent computing, over every sample and every thread that
//! did it — the work, not how long anyone waited for it. On one worker the two
//! are the same.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A sum of durations, safe to add to from several threads at once.
#[derive(Debug, Default)]
pub struct ComputeClock {
    nanos: AtomicU64,
}

impl ComputeClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `work`, adding the time it took to the clock.
    pub fn measure<T>(&self, work: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = work();
        self.add(start.elapsed());
        result
    }

    /// Add a duration measured elsewhere.
    pub fn add(&self, duration: Duration) {
        let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
        self.nanos.fetch_add(nanos, Ordering::Relaxed);
    }

    /// The total so far, in seconds.
    pub fn seconds(&self) -> f64 {
        self.nanos.load(Ordering::Relaxed) as f64 / 1e9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_add_up() {
        let clock = ComputeClock::new();
        clock.add(Duration::from_millis(1500));
        clock.add(Duration::from_millis(250));
        assert!((clock.seconds() - 1.75).abs() < 1e-9);
    }

    #[test]
    fn measuring_returns_what_the_work_returned() {
        let clock = ComputeClock::new();
        let value = clock.measure(|| {
            std::thread::sleep(Duration::from_millis(5));
            42
        });
        assert_eq!(value, 42);
        assert!(clock.seconds() >= 0.005);
    }

    /// Parallel samples add their times: the clock is the work done.
    #[test]
    fn threads_add_to_the_same_clock() {
        let clock = ComputeClock::new();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| clock.add(Duration::from_millis(100)));
            }
        });
        assert!((clock.seconds() - 0.4).abs() < 1e-9);
    }
}
