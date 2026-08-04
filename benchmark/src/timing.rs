//! Measuring how long something takes, and how much it cost to find out.

use std::time::{Duration, Instant};

/// The timings of several runs of the same work.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Samples(Vec<Duration>);

impl From<Vec<Duration>> for Samples {
    fn from(samples: Vec<Duration>) -> Self {
        Self(samples)
    }
}

impl Samples {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[Duration] {
        &self.0
    }

    /// The middle timing.
    ///
    /// Reported instead of the mean because the first run of anything pays for
    /// cold caches, a cold allocator and page faults it will never pay again.
    /// An average lets that one run set the number; a median ignores it.
    pub fn median(&self) -> Duration {
        median_of(&self.0)
    }

    /// The median absolute deviation: how far a typical run sits from the
    /// median.
    ///
    /// Reported next to the median so a reader can tell a measurement from a
    /// guess. A large deviation means the machine was busy and the number
    /// should not be quoted.
    pub fn mad(&self) -> Duration {
        if self.0.is_empty() {
            return Duration::ZERO;
        }
        let centre = self.median();
        let deviations: Vec<Duration> = self
            .0
            .iter()
            .map(|sample| sample.abs_diff(centre))
            .collect();
        median_of(&deviations)
    }

    /// The fastest run: the closest thing to the work without interference.
    pub fn best(&self) -> Duration {
        self.0.iter().copied().min().unwrap_or(Duration::ZERO)
    }
}

fn median_of(samples: &[Duration]) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();

    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        // Averaged in nanoseconds; a benchmark that ran for longer than the
        // 584 years a u64 of nanoseconds holds has other problems.
        let low = sorted[middle - 1].as_nanos() as u64;
        let high = sorted[middle].as_nanos() as u64;
        Duration::from_nanos((low + high) / 2)
    }
}

/// Run `work` `repeats` times, timing each run.
pub fn measure<F: FnMut()>(repeats: usize, mut work: F) -> Samples {
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let started = Instant::now();
        work();
        samples.push(started.elapsed());
    }
    Samples(samples)
}

/// Peak resident memory of this process, in bytes.
///
/// The high-water mark rather than the current usage: what matters for a
/// cohort that does not fit is the worst moment, not the moment the analysis
/// happened to finish at.
///
/// `None` off Linux, where `/proc` does not exist — the timings still stand.
pub fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kilobytes: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some(kilobytes * 1024);
        }
    }
    None
}

/// A byte count as a human reads it.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A duration as a human reads it.
pub fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds >= 1.0 {
        format!("{seconds:.2} s")
    } else if seconds >= 1e-3 {
        format!("{:.1} ms", seconds * 1e3)
    } else {
        format!("{:.0} µs", seconds * 1e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_run_is_the_fastest() {
        let samples = Samples::from(vec![
            Duration::from_millis(30),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ]);
        assert_eq!(samples.best(), Duration::from_millis(10));
    }

    #[test]
    fn bytes_are_rendered_in_the_largest_fitting_unit() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 kB");
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn durations_are_rendered_at_a_readable_scale() {
        assert_eq!(human_duration(Duration::from_secs(2)), "2.00 s");
        assert_eq!(human_duration(Duration::from_millis(15)), "15.0 ms");
        assert_eq!(human_duration(Duration::from_micros(30)), "30 µs");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_peak_memory_is_readable_on_this_platform() {
        let peak = peak_rss_bytes().expect("Linux exposes VmHWM");
        assert!(peak > 1024, "an impossibly small peak: {peak}");
    }
}
