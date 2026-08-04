//! Classifying a line of the analysis process's output — port of
//! `ImageViewerPanel.append_log`.

/// What a log line means, which decides how it is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Error,
    Warning,
    Success,
    Info,
    Progress,
    Plain,
}

/// Classify a line of output.
///
/// The order of the tests is the Python's and matters: a line carrying both an
/// error marker and an info marker is an error, because the reader needs to see
/// the worst thing that happened, not the first thing mentioned.
pub fn classify(line: &str) -> LogKind {
    if line.contains("[ERROR]") || line.contains('❌') {
        LogKind::Error
    } else if line.contains("[WARN]") || line.contains('⚠') {
        LogKind::Warning
    } else if line.contains("[OK]") || line.contains('✅') {
        LogKind::Success
    } else if line.contains("[INFO]") || line.contains("[QT_INFO]") {
        LogKind::Info
    } else if line.contains("[QT_PROGRESS]") {
        LogKind::Progress
    } else {
        LogKind::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_marker_is_recognised() {
        assert_eq!(classify("[ERROR] x"), LogKind::Error);
        assert_eq!(classify("[WARN] x"), LogKind::Warning);
        assert_eq!(classify("[OK] x"), LogKind::Success);
        assert_eq!(classify("[INFO] x"), LogKind::Info);
        assert_eq!(classify("[QT_PROGRESS] x"), LogKind::Progress);
        assert_eq!(classify("plain"), LogKind::Plain);
    }

    #[test]
    fn the_emoji_markers_work_too() {
        assert_eq!(classify("❌ failed"), LogKind::Error);
        assert_eq!(classify("⚠ careful"), LogKind::Warning);
        assert_eq!(classify("✅ done"), LogKind::Success);
    }

    #[test]
    fn the_worst_marker_wins() {
        assert_eq!(classify("[INFO] then [ERROR]"), LogKind::Error);
        assert_eq!(classify("[QT_PROGRESS] and [WARN]"), LogKind::Warning);
    }

    #[test]
    fn an_empty_line_is_plain() {
        assert_eq!(classify(""), LogKind::Plain);
    }
}
