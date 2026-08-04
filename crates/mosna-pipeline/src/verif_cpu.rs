//! Port of `package/utils/verif_cpu.py::verif_cpu`.

/// Number of worker threads to use.
///
/// Capped by the machine's core count and by the amount of work available:
/// spawning more workers than there are samples only adds scheduling overhead.
/// At least one is always returned.
pub fn verif_cpu(requested: usize, workload: usize) -> usize {
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    requested.min(available).min(workload.max(1)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_capped_by_the_workload() {
        assert_eq!(verif_cpu(64, 3), 3);
    }

    #[test]
    fn is_capped_by_the_machine() {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        assert!(verif_cpu(10_000, 10_000) <= available);
    }

    #[test]
    fn never_returns_zero() {
        assert_eq!(verif_cpu(0, 0), 1);
        assert_eq!(verif_cpu(0, 5), 1);
        assert_eq!(verif_cpu(4, 0), 1);
    }

    #[test]
    fn a_modest_request_is_honoured() {
        assert_eq!(verif_cpu(2, 100), 2);
    }
}
