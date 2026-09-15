//! Port of `package/clear_temporary.py`.

use std::path::Path;

use crate::error::{PipelineError, Result};
use crate::progress::Progress;

/// Remove the `temp` directory of a working directory.
///
/// Does nothing when it is absent, which is the Python's behaviour and lets the
/// GUI's "Clear Temp Files" button be pressed at any time.
pub fn clear_temporary(working_dir: &Path, progress: &dyn Progress) -> Result<()> {
    let temp_dir = working_dir.join("temp");
    abandoned_figures(working_dir, progress)?;

    if temp_dir.is_dir() {
        // Said before the deletion, not after: what goes is more than the
        // button's caption suggests, and a user reading the log afterwards is
        // reading it too late.
        //
        // `temp` holds two quite different things. `net_dir_mosna` is the
        // networks — and every `niches_*` label column step 3 wrote into them,
        // which is what the interface colours the network view by.
        // `intermediate_files` is the niche cache, which on a real cohort is
        // hours of aggregation and projection. Neither was mentioned.
        let networks = temp_dir.join("net_dir_mosna").is_dir();
        let caches = temp_dir.join("intermediate_files").is_dir();
        if networks {
            progress.info(
                "[INFO] The networks go, and with them every niches_* label column \
                 step 3 wrote into them",
            );
        }
        if caches {
            progress.info(
                "[INFO] The niche cache goes: the next niche analysis recomputes its \
                 features, projections and partitions from scratch",
            );
        }

        std::fs::remove_dir_all(&temp_dir).map_err(|source| PipelineError::Remove {
            path: temp_dir.clone(),
            source,
        })?;
        progress.info(&format!(
            "[INFO] Temporary folder removed: {}",
            temp_dir.display()
        ));
    } else {
        progress.info(&format!(
            "[INFO] No temporary folder found at: {}",
            temp_dir.display()
        ));
    }

    Ok(())
}

/// Remove a figure queue no run is going to draw.
///
/// The queue of specifications is deleted by the run that drew them, so one is
/// left behind only by a run that was killed — or, before each queue had a
/// folder of its own, by a run whose queue another had deleted underneath it.
/// Nothing else ever cleans them up, and a sweep that crashes repeatedly would
/// fill the working directory with figures nobody is waiting for.
///
/// It is not an error if this fails: the queue costs disk space and nothing
/// else, and refusing to clear the temporary data over it would be worse than
/// leaving it.
fn abandoned_figures(working_dir: &Path, progress: &dyn Progress) -> Result<()> {
    let queues = working_dir.join(crate::figures::FIGURE_QUEUE_DIRECTORY);
    if !queues.is_dir() {
        return Ok(());
    }
    progress.info(
        "[INFO] A queue of figures left by an interrupted run goes too; \
         the results it described are unaffected",
    );
    if let Err(error) = std::fs::remove_dir_all(&queues) {
        progress.info(&format!(
            "[INFO] The figure queue at {} could not be removed: {error}",
            queues.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;

    #[test]
    fn removes_the_directory_and_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("temp/net_dir_mosna");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("nodes_patient-1.parquet"), b"x").unwrap();

        clear_temporary(dir.path(), &SilentProgress).unwrap();
        assert!(!dir.path().join("temp").exists());
    }

    #[test]
    fn an_absent_directory_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        clear_temporary(dir.path(), &SilentProgress).unwrap();
    }

    #[test]
    fn only_the_temp_directory_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("temp")).unwrap();
        std::fs::create_dir_all(dir.path().join("Tysserand_Network")).unwrap();
        std::fs::write(dir.path().join("Tysserand_Network/net_1.png"), b"x").unwrap();

        clear_temporary(dir.path(), &SilentProgress).unwrap();
        assert!(dir.path().join("Tysserand_Network/net_1.png").is_file());
    }

    /// The niche caches live under `temp` too, and clearing them throws away
    /// hours of work on a real cohort. The message has to say so: the button's
    /// caption talks about "intermediate networks", which is half of what goes.
    #[test]
    fn the_message_says_the_niche_caches_go_too() {
        let dir = tempfile::tempdir().unwrap();
        let caches = dir.path().join("temp/intermediate_files/var_aggreg-1");
        std::fs::create_dir_all(&caches).unwrap();
        std::fs::write(caches.join("var_aggreg_1.parquet"), b"x").unwrap();

        let spoken = Spoken::default();
        clear_temporary(dir.path(), &spoken).unwrap();

        assert!(
            spoken.said("niche"),
            "the niche caches went without being mentioned: {:?}",
            spoken.lines()
        );
    }

    /// And the label columns go with the networks that carried them, which is
    /// the part a user is most likely not to expect.
    #[test]
    fn the_message_says_the_niche_labels_go_with_the_networks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("temp/net_dir_mosna")).unwrap();

        let spoken = Spoken::default();
        clear_temporary(dir.path(), &spoken).unwrap();
        assert!(
            spoken.said("niches_"),
            "nothing said the label columns go: {:?}",
            spoken.lines()
        );
    }

    /// Records what the pipeline says, so a test can assert on a message.
    #[derive(Default)]
    struct Spoken(std::sync::Mutex<Vec<String>>);

    impl Progress for Spoken {
        fn info(&self, message: &str) {
            self.0.lock().unwrap().push(message.to_string());
        }
        fn step(&self, _current: usize, _total: usize, _description: &str) {}
    }

    impl Spoken {
        fn said(&self, fragment: &str) -> bool {
            self.0
                .lock()
                .unwrap()
                .iter()
                .any(|line| line.contains(fragment))
        }
        fn lines(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }
}
