//! Port of `package/utils/emit_qt_progress.py`.
//!
//! These two lines are the whole protocol between a running analysis and the
//! GUI. The GUI matches `current=(\d+)`, `total=(\d+)` and `desc=(.*)$` on
//! `[QT_PROGRESS]` lines, and strips the `[QT_INFO]` prefix off status lines,
//! so the exact spelling and spacing below has to be preserved.

use std::io::Write;

/// Report progress through a step.
///
/// ```text
/// [QT_PROGRESS] current=3 total=12 desc=[MULTI PROCESS] Processing file
/// ```
///
/// Newlines are collapsed into spaces because the GUI reads one line at a time,
/// and a `desc` spanning two lines would leave the second one displayed as a
/// stray log entry.
pub fn emit_qt_progress(current: usize, total: usize, desc: &str) {
    let desc_clean = clean(desc);
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(
        stdout,
        "[QT_PROGRESS] current={current} total={total} desc={desc_clean}"
    );
    let _ = stdout.flush();
}

/// Report a status message.
///
/// ```text
/// [QT_INFO] Parameters are read correctly
/// ```
pub fn emit_qt_info(message: &str) {
    let msg = clean(message);
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "[QT_INFO] {msg}");
    let _ = stdout.flush();
}

/// `(desc or "").replace("\n", " ").strip()`
fn clean(text: &str) -> String {
    text.replace('\n', " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newlines_are_flattened_and_edges_trimmed() {
        assert_eq!(clean("  [INFO] a\nb  "), "[INFO] a b");
        assert_eq!(clean(""), "");
    }
}
