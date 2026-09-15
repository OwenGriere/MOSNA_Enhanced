//! The lock that lets two niche analyses share one working directory.
//!
//! # What went wrong without it
//!
//! `runs.json` is read at the start of a run and written at the end. Two runs
//! started together therefore both read an empty register, both were handed the
//! numbers `1-1-1`, and both wrote to that directory, that cache file, and that
//! label column. One of the two was lost outright, and what survived could be
//! the figures of one run beside the labels of the other. Nothing reported it.
//!
//! Sweeping a grid of parameters is exactly the case that hits this, so the
//! register has to be safe to share before a batch of runs can be built on it.
//!
//! # What is locked, and for how long
//!
//! Only the register, and only while it is being read and rewritten — never
//! while anything is computed. A run takes the lock to claim its numbers,
//! releases it, does the work, and takes it again to record the outcome. Two
//! runs therefore overlap freely; what they cannot do is choose the same
//! numbers.
//!
//! The lock is advisory and held on `Niche_Analysis/.runs.lock` through
//! `fs2`, which is `flock` on Unix and `LockFileEx` on Windows. Both are
//! released by the operating system when the process ends, so a run killed
//! half-way leaves nothing to clean up — which a lock file created with
//! `create_new` would not have managed.

use std::fs::File;
use std::path::Path;

use fs2::FileExt;

use crate::error::{create_dir_all, PipelineError, Result};

/// The lock file, beside the register it guards.
pub const LOCK: &str = ".runs.lock";

/// The lock file guarding the nodes files of a network directory.
///
/// Dot-prefixed, and not matching `nodes_*`, so the sample discovery walks past
/// it.
pub const NODES_LOCK: &str = ".mosna-nodes.lock";

/// An exclusive hold on a directory's run register.
///
/// Releasing is `Drop`, so a `?` on the way out of a guarded section cannot
/// leave the lock held.
#[derive(Debug)]
pub struct RegisterLock {
    file: File,
}

impl RegisterLock {
    /// Take the lock of `niche_dir`, waiting for whoever holds it.
    ///
    /// Blocking rather than failing: the section being guarded is a read and a
    /// write of one small file, so a caller that finds it busy has nothing to
    /// do but wait a few milliseconds. Returning "busy" would push a retry loop
    /// into every call site.
    pub fn acquire(niche_dir: &Path) -> Result<Self> {
        Self::at(niche_dir, LOCK)
    }

    /// The lock that serialises writing niche labels back into the nodes files.
    ///
    /// # Why the nodes files need one of their own
    ///
    /// `merge_niche_pheno` reads a nodes file, adds this run's label column, and
    /// writes it back. Two runs doing that at once both read the same version
    /// and both write their own: one of the two label columns is lost, and the
    /// reader that arrives mid-write gets a truncated parquet. The atomic
    /// rename in `write_parquet_atomic` closes the second window; this closes
    /// the first.
    ///
    /// It is a different lock from the register's, and held for a different
    /// span: the register is claimed in milliseconds at the start of a run,
    /// while this is held for as long as the cohort takes to rewrite. Sharing
    /// one lock between them would make every run wait for every other run's
    /// write-back before it could even claim its number.
    pub fn nodes(net_dir: &Path) -> Result<Self> {
        Self::at(net_dir, NODES_LOCK)
    }

    fn at(directory: &Path, name: &str) -> Result<Self> {
        create_dir_all(directory)?;
        let path = directory.join(name);
        let file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| PipelineError::Write {
                path: path.clone(),
                source,
            })?;
        file.lock_exclusive()
            .map_err(|source| PipelineError::Write { path, source })?;
        Ok(Self { file })
    }
}

impl Drop for RegisterLock {
    fn drop(&mut self) {
        // Nothing useful to do if this fails: the lock goes with the file
        // handle, which is closed either way, and the operating system drops it
        // when the process ends.
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn the_lock_file_sits_beside_the_register() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = RegisterLock::acquire(dir.path()).unwrap();
        assert!(dir.path().join(LOCK).is_file());
    }

    /// The directory need not exist yet: the first run of a working directory
    /// takes the lock before anything has created `Niche_Analysis`.
    #[test]
    fn an_absent_directory_is_created_for_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let niche_dir = dir.path().join("Niche_Analysis");
        let _guard = RegisterLock::acquire(&niche_dir).unwrap();
        assert!(niche_dir.is_dir());
    }

    /// Taking it twice in sequence is fine — the first hold is released when
    /// its guard is dropped.
    #[test]
    fn the_lock_is_released_when_the_guard_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        drop(RegisterLock::acquire(dir.path()).unwrap());
        drop(RegisterLock::acquire(dir.path()).unwrap());
    }

    /// The property the register depends on: while one holder is inside the
    /// guarded section, no other holder is.
    #[test]
    fn only_one_holder_is_inside_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().to_path_buf());
        let inside = Arc::new(AtomicUsize::new(0));
        let overlaps = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (path, inside, overlaps) =
                    (path.clone(), inside.clone(), overlaps.clone());
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let _guard = RegisterLock::acquire(&path).unwrap();
                        if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                            overlaps.fetch_add(1, Ordering::SeqCst);
                        }
                        // Long enough that an unguarded section would overlap.
                        std::thread::sleep(std::time::Duration::from_micros(200));
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            overlaps.load(Ordering::SeqCst),
            0,
            "two holders were inside the guarded section at once"
        );
    }

    /// The nodes lock is a different lock from the register's: a run holding
    /// one must not block a run that wants the other.
    #[test]
    fn the_register_and_the_nodes_have_separate_locks() {
        let dir = tempfile::tempdir().unwrap();
        let _register = RegisterLock::acquire(dir.path()).unwrap();
        // Would deadlock the test if the two shared a file.
        let _nodes = RegisterLock::nodes(dir.path()).unwrap();

        assert!(dir.path().join(LOCK).is_file());
        assert!(dir.path().join(NODES_LOCK).is_file());
    }

    /// And the nodes lock is not mistaken for a sample: the discovery walks a
    /// network directory looking for `nodes_*`, and would try to read this.
    #[test]
    fn the_nodes_lock_is_not_a_nodes_file() {
        assert!(NODES_LOCK.starts_with('.'));
        assert!(!NODES_LOCK.starts_with("nodes_"));
    }
}
