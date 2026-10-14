//! Steps running in the background.
//!
//! A tool call that blocks for the twenty minutes of a niche analysis would
//! outlast any client's timeout, so a step is started, given an id, and polled.
//! Its stdout is read in the progress protocol both implementations print:
//!
//! ```text
//! [QT_INFO] <message>
//! [QT_PROGRESS] current=<n> total=<n> desc=<text>
//! ```

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Lines of output kept per job: enough for a traceback.
const TAIL: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Running,
    Succeeded,
    Failed(Option<i32>),
    Cancelled,
}

#[derive(Debug)]
struct State {
    status: Status,
    progress: Option<(usize, usize, String)>,
    info: Option<String>,
    tail: VecDeque<String>,
    finished: Option<Instant>,
}

struct Job {
    description: Value,
    started: Instant,
    child: Arc<Mutex<Child>>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
pub struct Jobs {
    next: Mutex<usize>,
    jobs: Mutex<BTreeMap<String, Job>>,
}

impl Jobs {
    /// Start `command` and return its id at once.
    pub fn spawn(&self, mut command: Command, description: Value) -> anyhow::Result<String> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            anyhow::anyhow!(
                "cannot start `{}`: {error}",
                command.get_program().to_string_lossy()
            )
        })?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let state = Arc::new(Mutex::new(State {
            status: Status::Running,
            progress: None,
            info: None,
            tail: VecDeque::new(),
            finished: None,
        }));
        let child = Arc::new(Mutex::new(child));

        if let Some(stdout) = stdout {
            follow(stdout, Arc::clone(&state));
        }
        if let Some(stderr) = stderr {
            follow(stderr, Arc::clone(&state));
        }
        watch(Arc::clone(&child), Arc::clone(&state));

        let id = {
            let mut next = self.next.lock().unwrap();
            *next += 1;
            format!("job-{next}")
        };
        self.jobs.lock().unwrap().insert(
            id.clone(),
            Job {
                description,
                started: Instant::now(),
                child,
                state,
            },
        );
        Ok(id)
    }

    pub fn status(&self, id: &str) -> anyhow::Result<Value> {
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("no job `{id}`"))?;
        Ok(report(job))
    }

    pub fn list(&self) -> Value {
        let jobs = self.jobs.lock().unwrap();
        Value::Array(
            jobs.iter()
                .map(|(id, job)| {
                    let mut value = report(job);
                    value["id"] = json!(id);
                    value.as_object_mut().map(|o| o.remove("log"));
                    value
                })
                .collect(),
        )
    }

    pub fn cancel(&self, id: &str) -> anyhow::Result<Value> {
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("no job `{id}`"))?;
        let mut state = job.state.lock().unwrap();
        if state.status == Status::Running {
            job.child.lock().unwrap().kill().ok();
            state.status = Status::Cancelled;
            state.finished = Some(Instant::now());
        }
        drop(state);
        Ok(report(job))
    }

    /// Kill whatever is still running: the server is going away.
    pub fn shutdown(&self) {
        for job in self.jobs.lock().unwrap().values() {
            if job.state.lock().unwrap().status == Status::Running {
                job.child.lock().unwrap().kill().ok();
            }
        }
    }
}

fn report(job: &Job) -> Value {
    let state = job.state.lock().unwrap();
    let end = state.finished.unwrap_or_else(Instant::now);
    let status = match &state.status {
        Status::Running => "running".to_string(),
        Status::Succeeded => "succeeded".to_string(),
        Status::Failed(code) => match code {
            Some(code) => format!("failed (exit {code})"),
            None => "failed (killed by a signal)".to_string(),
        },
        Status::Cancelled => "cancelled".to_string(),
    };
    let mut value = json!({
        "status": status,
        "job": job.description,
        "elapsed_seconds": (end - job.started).as_secs(),
        "last_info": state.info,
    });
    if let Some((current, total, description)) = &state.progress {
        value["progress"] = json!({ "current": current, "total": total, "step": description });
    }
    if matches!(state.status, Status::Failed(_)) {
        value["reason"] = json!(failure_reason(&state.tail));
    }
    value["log"] = json!(state.tail.iter().rev().take(20).rev().collect::<Vec<_>>());
    value
}

/// The line that says why a step failed: the exception of a Python traceback,
/// or the last thing the Rust binary printed, which is its error.
pub fn failure_reason(tail: &VecDeque<String>) -> Option<String> {
    let meaningful = |line: &&String| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with("[QT_") && !line.starts_with("caused by")
    };
    tail.iter()
        .rev()
        .filter(meaningful)
        .find(|line| {
            let head = line.split(':').next().unwrap_or("");
            !line.starts_with(' ') && (head.ends_with("Error") || head.ends_with("Exception"))
        })
        .or_else(|| tail.iter().rev().find(meaningful))
        .cloned()
}

fn follow(stream: impl Read + Send + 'static, state: Arc<Mutex<State>>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let mut state = state.lock().unwrap();
            if let Some(info) = line.strip_prefix("[QT_INFO]") {
                state.info = Some(info.trim().to_string());
            } else if let Some(payload) = line.strip_prefix("[QT_PROGRESS]") {
                if let Some(progress) = parse_progress(payload) {
                    state.progress = Some(progress);
                }
                continue;
            }
            state.tail.push_back(line);
            if state.tail.len() > TAIL {
                state.tail.pop_front();
            }
        }
    });
}

fn watch(child: Arc<Mutex<Child>>, state: Arc<Mutex<State>>) {
    std::thread::spawn(move || loop {
        let outcome = child.lock().unwrap().try_wait();
        match outcome {
            Ok(Some(exit)) => {
                let mut state = state.lock().unwrap();
                if state.status == Status::Running {
                    state.status = if exit.success() {
                        Status::Succeeded
                    } else {
                        Status::Failed(exit.code())
                    };
                    state.finished = Some(Instant::now());
                }
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(_) => return,
        }
    });
}

/// `current=<n> total=<n> desc=<text>`; `None` rather than a guess when a
/// number is missing.
pub fn parse_progress(payload: &str) -> Option<(usize, usize, String)> {
    let field = |name: &str| {
        payload
            .split_whitespace()
            .find_map(|word| word.strip_prefix(name))
            .and_then(|v| v.parse().ok())
    };
    let description = payload
        .find("desc=")
        .map(|at| payload[at + 5..].trim().to_string())
        .unwrap_or_default();
    Some((field("current=")?, field("total=")?, description))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_parse() {
        assert_eq!(
            parse_progress(" current=3 total=10 desc=[PROCESS] Verification of all file"),
            Some((3, 10, "[PROCESS] Verification of all file".to_string()))
        );
        assert_eq!(parse_progress(" current=3 desc=x"), None);
    }

    #[test]
    fn the_exception_is_the_reason_not_the_noise_after_it() {
        let tail: VecDeque<String> = [
            "Traceback (most recent call last):",
            "  File \"niche_analysis.py\", line 186, in main",
            "KeyError: 'Cluster'",
            "[QT_INFO] figures drawn",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            failure_reason(&tail).as_deref(),
            Some("KeyError: 'Cluster'")
        );
    }

    #[test]
    fn a_rust_error_is_its_last_line() {
        let tail: VecDeque<String> =
            ["column `Cluster` is not present in /d/nodes_patient-1.parquet"]
                .into_iter()
                .map(String::from)
                .collect();
        assert!(failure_reason(&tail).unwrap().contains("Cluster"));
    }

    #[cfg(unix)]
    #[test]
    fn a_job_runs_to_completion_and_reports_its_progress() {
        let jobs = Jobs::default();
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo '[QT_PROGRESS] current=1 total=2 desc=half'; echo '[QT_INFO] done'",
        ]);
        let id = jobs.spawn(command, json!({})).unwrap();
        for _ in 0..40 {
            if jobs.status(&id).unwrap()["status"] == "succeeded" {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // The reader threads may lag the exit by a moment.
        std::thread::sleep(Duration::from_millis(100));
        let status = jobs.status(&id).unwrap();
        assert_eq!(status["status"], "succeeded");
        assert_eq!(status["progress"]["current"], 1);
        assert_eq!(status["last_info"], "done");
    }
}
