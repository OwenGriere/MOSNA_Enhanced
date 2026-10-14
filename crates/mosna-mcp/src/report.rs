//! What a check found, in a form a model can act on.
//!
//! Every finding carries a stable `code` — so a client can match on it rather
//! than on wording — and says which implementation it concerns. That last field
//! is the point of having one server for both: a file both implementations
//! reject is a data problem, a file only one of them mishandles is a portability
//! problem, and the two call for different fixes.

use serde_json::{json, Value};

/// How bad a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Context, nothing to fix.
    Info,
    /// The step runs, but its result is suspect or differs between the two
    /// implementations.
    Warning,
    /// The step fails, or silently produces a wrong result.
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Which implementation a finding concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affects {
    Both,
    Rust,
    Python,
}

impl Affects {
    pub fn as_str(self) -> &'static str {
        match self {
            Affects::Both => "both",
            Affects::Rust => "rust",
            Affects::Python => "python",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub affects: Affects,
    /// The file it is about, by name; `None` for the cohort as a whole.
    pub file: Option<String>,
    pub message: String,
}

impl Finding {
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "severity": self.severity.as_str(),
            "code": self.code,
            "affects": self.affects.as_str(),
            "message": self.message,
        });
        if let Some(file) = &self.file {
            value["file"] = json!(file);
        }
        value
    }
}

/// An ordered list of findings.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn push(
        &mut self,
        severity: Severity,
        code: &'static str,
        affects: Affects,
        file: Option<&str>,
        message: impl Into<String>,
    ) {
        self.findings.push(Finding {
            severity,
            code,
            affects,
            file: file.map(str::to_string),
            message: message.into(),
        });
    }

    pub fn error(&mut self, code: &'static str, file: Option<&str>, message: impl Into<String>) {
        self.push(Severity::Error, code, Affects::Both, file, message);
    }

    pub fn warning(&mut self, code: &'static str, file: Option<&str>, message: impl Into<String>) {
        self.push(Severity::Warning, code, Affects::Both, file, message);
    }

    pub fn info(&mut self, code: &'static str, file: Option<&str>, message: impl Into<String>) {
        self.push(Severity::Info, code, Affects::Both, file, message);
    }

    pub fn extend(&mut self, other: Report) {
        self.findings.extend(other.findings);
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    /// Whether the step this report prepares can be started.
    pub fn is_ok(&self) -> bool {
        self.count(Severity::Error) == 0
    }

    /// Worst first, then in the order found, so a truncated reading still
    /// sees what blocks the run.
    pub fn sorted(mut self) -> Self {
        self.findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
        self
    }

    /// The findings as JSON, at most `limit` of them.
    ///
    /// A cohort of a hundred files with the same defect would otherwise send a
    /// hundred identical lines into the model's context; the count of what was
    /// left out is kept so nothing is hidden.
    pub fn to_json(&self, limit: usize) -> Value {
        let shown: Vec<Value> = self
            .findings
            .iter()
            .take(limit)
            .map(Finding::to_json)
            .collect();
        json!({
            "ok": self.is_ok(),
            "errors": self.count(Severity::Error),
            "warnings": self.count(Severity::Warning),
            "infos": self.count(Severity::Info),
            "findings": shown,
            "omitted": self.findings.len().saturating_sub(limit),
        })
    }
}
