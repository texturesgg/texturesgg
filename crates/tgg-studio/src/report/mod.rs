//! Report a problem: the player sees exactly what would be sent (the app's
//! version, their system, and the end of the log, their home folder written
//! as `~`), then copies it, takes it to Discord, or sends it. Nothing leaves
//! without that choice. After a crash the next launch offers the same.
//! `actions` changes it; `view` draws the dialog.

pub mod actions;
pub mod view;

use std::path::Path;

/// How much of the log's end a report carries.
const LOG_BYTES: u64 = 48 * 1024;
const LOG_LINES: usize = 300;

/// A report, open in its dialog.
pub struct Report {
    /// Exactly what Copy copies and Send sends.
    pub text: String,
    /// Offered because the last session crashed.
    pub crashed: bool,
    pub progress: Progress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    Unsent,
    Copied,
    Sending,
    /// Sent, kept under this id.
    Sent(String),
    Failed(String),
}

impl Report {
    /// The report for the log as it stands.
    pub fn now(crashed: bool) -> Self {
        Self {
            text: compose(
                &tgg_app::log::recent(LOG_BYTES),
                dirs::home_dir().as_deref(),
            ),
            crashed,
            progress: Progress::Unsent,
        }
    }
}

/// What's running and the last lines of `log`, with the player's home
/// folder written as `~` so their account name stays out of it.
pub fn compose(log: &str, home: Option<&Path>) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let recent = lines[lines.len().saturating_sub(LOG_LINES)..].join("\n");
    let report = format!("{}\n\n{recent}", tgg_app::log::version());
    match home.and_then(Path::to_str) {
        Some(home) if !home.is_empty() => report.replace(home, "~"),
        _ => report,
    }
}

#[cfg(test)]
mod tests {
    use super::compose;
    use std::path::Path;

    #[test]
    fn a_report_keeps_the_logs_end_and_hides_the_home_folder() {
        let log: String = (0..400)
            .map(|n| format!("line {n} in /home/someone/x\n"))
            .collect();
        let report = compose(&log, Some(Path::new("/home/someone")));
        assert!(report.contains("line 399 in ~/x"));
        assert!(!report.contains("line 99 "), "only the last 300 lines");
        assert!(!report.contains("/home/someone"));
    }
}
