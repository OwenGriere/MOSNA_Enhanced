//! The queue of analyses — the Viewer's Queue tab.
//!
//! An analysis button no longer waits for the one before it to finish: it
//! adds a job here, and the jobs run one after the other, in the order they
//! were added. One at a time for the reason the sweep runs one at a time: a
//! run already uses every core, so two at once would only divide the machine
//! between them.
//!
//! A job carries everything it is launched with, decided when it is added — the
//! arguments, the working directory, and a copy of the configuration as the
//! panels showed it then. Changing a parameter after queuing an analysis
//! changes the next one, not the one already waiting.

use std::path::PathBuf;

use crate::model::runner::Step;

/// Where a job is in its life.
#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Waiting,
    Running,
    /// The cross was pressed while it ran; it ends as soon as the process does.
    Cancelling,
    Done,
    Failed(String),
    Cancelled,
}

impl JobState {
    /// Whether the job is over, one way or another.
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            JobState::Done | JobState::Failed(_) | JobState::Cancelled
        )
    }

    /// The word the Queue tab shows beside the bar.
    pub fn caption(&self) -> &str {
        match self {
            JobState::Waiting => "Waiting",
            JobState::Running => "Running",
            JobState::Cancelling => "Cancelling…",
            JobState::Done => "Done",
            JobState::Failed(_) => "Failed",
            JobState::Cancelled => "Cancelled",
        }
    }
}

/// One analysis in the queue.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub step: Step,
    pub arguments: Vec<String>,
    pub working_dir: PathBuf,
    /// The configuration copy the job reads, removed once it has ended.
    pub config_copy: Option<PathBuf>,
    /// Whether the job was queued "without plotting figures".
    pub without_figures: bool,
    pub state: JobState,
    /// `(current, total)` as the analysis last reported it.
    pub progress: Option<(usize, usize)>,
    /// The last thing the analysis said about itself.
    pub message: String,
    /// How long it ran, once it has ended.
    pub elapsed: Option<f64>,
}

impl Job {
    /// How far along the bar is, in `[0, 1]`; `None` while the analysis has
    /// not said yet.
    pub fn fraction(&self) -> Option<f32> {
        match self.state {
            JobState::Done => Some(1.0),
            JobState::Waiting => Some(0.0),
            _ => self
                .progress
                .filter(|(_, total)| *total > 0)
                .map(|(current, total)| (current as f32 / total as f32).clamp(0.0, 1.0)),
        }
    }
}

/// The jobs, in the order they were added.
#[derive(Debug, Default)]
pub struct Queue {
    pub jobs: Vec<Job>,
    next_id: u64,
}

impl Queue {
    /// Add a job at the end of the queue, and return its id.
    pub fn push(
        &mut self,
        step: Step,
        arguments: Vec<String>,
        working_dir: PathBuf,
        config_copy: Option<PathBuf>,
        without_figures: bool,
    ) -> u64 {
        let id = self.reserve_id();
        self.add(
            id,
            step,
            arguments,
            working_dir,
            config_copy,
            without_figures,
        );
        id
    }

    /// Add a job under an id taken earlier with [`Self::reserve_id`].
    pub fn add(
        &mut self,
        id: u64,
        step: Step,
        arguments: Vec<String>,
        working_dir: PathBuf,
        config_copy: Option<PathBuf>,
        without_figures: bool,
    ) {
        self.jobs.push(Job {
            id,
            step,
            arguments,
            working_dir,
            config_copy,
            without_figures,
            state: JobState::Waiting,
            progress: None,
            message: String::new(),
            elapsed: None,
        });
    }

    /// The id the next job will get, taken now — the configuration copy is
    /// named after it before the job exists.
    pub fn reserve_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn get(&self, id: u64) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|job| job.id == id)
    }

    /// The first job still waiting its turn.
    pub fn next_waiting(&self) -> Option<u64> {
        self.jobs
            .iter()
            .find(|job| job.state == JobState::Waiting)
            .map(|job| job.id)
    }

    /// How many jobs have not run yet.
    pub fn waiting(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| job.state == JobState::Waiting)
            .count()
    }

    /// Take a job out of the list, cleaning up its configuration copy.
    ///
    /// Only a job that is not running: one that is has to be killed first, and
    /// leaves the list once its process is reaped.
    pub fn remove(&mut self, id: u64) -> bool {
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return false;
        };
        if matches!(
            self.jobs[index].state,
            JobState::Running | JobState::Cancelling
        ) {
            return false;
        }
        let job = self.jobs.remove(index);
        discard(&job);
        true
    }

    /// Record how a job ended.
    pub fn finish(
        &mut self,
        id: u64,
        success: bool,
        elapsed: f64,
        reason: impl FnOnce() -> String,
    ) {
        let Some(job) = self.get_mut(id) else { return };
        job.elapsed = Some(elapsed);
        job.state = if job.state == JobState::Cancelling {
            JobState::Cancelled
        } else if success {
            JobState::Done
        } else {
            JobState::Failed(reason())
        };
        discard(job);
        job.config_copy = None;
    }

    /// Forget every job that has ended.
    pub fn clear_finished(&mut self) {
        self.jobs.retain(|job| !job.state.is_finished());
    }
}

/// Remove a job's configuration copy, if it still has one.
fn discard(job: &Job) {
    if let Some(path) = &job.config_copy {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_of(steps: &[Step]) -> (Queue, Vec<u64>) {
        let mut queue = Queue::default();
        let ids = steps
            .iter()
            .map(|step| queue.push(*step, Vec::new(), PathBuf::from("/work"), None, false))
            .collect();
        (queue, ids)
    }

    #[test]
    fn jobs_run_in_the_order_they_were_added() {
        let (mut queue, ids) = queue_of(&[Step::Tysserand, Step::Assortativity]);
        assert_eq!(queue.next_waiting(), Some(ids[0]));

        queue.get_mut(ids[0]).unwrap().state = JobState::Running;
        assert_eq!(queue.next_waiting(), Some(ids[1]));
        assert_eq!(queue.waiting(), 1);
    }

    #[test]
    fn a_waiting_job_can_be_removed_but_a_running_one_cannot() {
        let (mut queue, ids) = queue_of(&[Step::Tysserand, Step::Assortativity]);
        queue.get_mut(ids[0]).unwrap().state = JobState::Running;

        assert!(!queue.remove(ids[0]), "a running job left the list");
        assert!(queue.remove(ids[1]));
        assert_eq!(queue.jobs.len(), 1);
    }

    #[test]
    fn a_job_cancelled_while_running_ends_cancelled_not_failed() {
        let (mut queue, ids) = queue_of(&[Step::NicheAnalysis]);
        queue.get_mut(ids[0]).unwrap().state = JobState::Cancelling;

        queue.finish(ids[0], false, 1.0, || "killed".to_string());
        assert_eq!(queue.get(ids[0]).unwrap().state, JobState::Cancelled);
    }

    #[test]
    fn a_failure_keeps_its_reason() {
        let (mut queue, ids) = queue_of(&[Step::NicheAnalysis]);
        queue.get_mut(ids[0]).unwrap().state = JobState::Running;

        queue.finish(ids[0], false, 1.0, || "no such column".to_string());
        assert_eq!(
            queue.get(ids[0]).unwrap().state,
            JobState::Failed("no such column".to_string())
        );
    }

    #[test]
    fn the_configuration_copy_goes_when_the_job_does() {
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("job.yaml");
        std::fs::write(&copy, "Tysserand: {}\n").unwrap();

        let mut queue = Queue::default();
        let id = queue.push(
            Step::Tysserand,
            Vec::new(),
            dir.path().to_path_buf(),
            Some(copy.clone()),
            false,
        );
        queue.remove(id);
        assert!(!copy.exists());
    }

    #[test]
    fn clearing_keeps_what_has_not_ended() {
        let (mut queue, ids) = queue_of(&[Step::Tysserand, Step::Assortativity]);
        queue.get_mut(ids[0]).unwrap().state = JobState::Running;
        queue.finish(ids[0], true, 1.0, String::new);

        queue.clear_finished();
        assert_eq!(queue.jobs.len(), 1);
        assert_eq!(queue.jobs[0].id, ids[1]);
    }
}
