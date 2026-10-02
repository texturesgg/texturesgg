//! The app's log. Diagnostics go to stderr and, once [`start`] has run, to
//! `textures.gg/logs/editor.log` under the platform's data directory, so a
//! player can send what happened. The log stays on their
//! disk until they choose to send it.
//!
//! A panic is written there with its backtrace, and leaves a marker beside
//! the log so the next launch can offer to report it. Only Rust panics are
//! caught; a crash inside a graphics driver leaves no trace here.

use std::backtrace::Backtrace;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The log file and when its session began; unset in tests and before
/// [`start`], when diagnostics go to stderr alone.
static SINK: OnceLock<(Mutex<File>, Instant)> = OnceLock::new();

/// A log longer than this keeps only its end when a session starts.
const KEEP_BYTES: u64 = 256 * 1024;

const LOG: &str = "editor.log";
/// Left by a panic, taken by the next launch.
const CRASHED: &str = "crashed";

/// Where the log is kept.
pub(crate) fn folder() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("textures.gg").join("logs"))
}

/// What is running, and on what: the first line of a session's log.
pub(crate) fn version() -> String {
    format!(
        "textures.gg editor {} ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Set by [`start`] from the marker the last session's panic left.
static CRASHED_BEFORE: OnceLock<bool> = OnceLock::new();

/// Begin this session's log in the player's log folder and record panics
/// there.
pub(crate) fn start() {
    if let Some(folder) = folder() {
        // Asked about once: the marker goes as it is read.
        let _ = CRASHED_BEFORE.set(std::fs::remove_file(folder.join(CRASHED)).is_ok());
        start_in(&folder);
    }
}

/// Whether the session before this one panicked.
pub(crate) fn crashed_before() -> bool {
    CRASHED_BEFORE.get().copied().unwrap_or(false)
}

/// The end of the log as it stands, at most `bytes` long.
pub(crate) fn recent(bytes: u64) -> String {
    folder().map_or_else(String::new, |folder| tail(&folder.join(LOG), bytes))
}

fn start_in(folder: &Path) {
    let file = match open(folder) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("no log file in {}: {error}", folder.display());
            return;
        }
    };
    if SINK.set((Mutex::new(file), Instant::now())).is_err() {
        return;
    }
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    write(&format!("--- {} started at {started}", version()));

    let marker = folder.join(CRASHED);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = Backtrace::force_capture().to_string();
        write(&format!("panic: {info}\n{}", backtrace.trim_end()));
        let _ = File::create(&marker);
        default(info);
    }));
}

/// The log file, opened to append, holding at most the end of what earlier
/// sessions wrote.
fn open(folder: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(LOG);
    let kept = tail(&path, KEEP_BYTES);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    file.write_all(kept.as_bytes())?;
    Ok(file)
}

/// The last `bytes` of the file at `path`, from the start of a line; empty
/// when there is no file.
fn tail(path: &Path, bytes: u64) -> String {
    let read = || -> std::io::Result<String> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(bytes)))?;
        let mut end = Vec::new();
        file.read_to_end(&mut end)?;
        let text = String::from_utf8_lossy(&end);
        Ok(if length > bytes {
            // The cut lands mid-line; start at the next whole one.
            text.split_once('\n')
                .map_or_else(String::new, |(_, rest)| rest.to_owned())
        } else {
            text.into_owned()
        })
    };
    read().unwrap_or_default()
}

/// Append `message` to the log file, stamped with the seconds since the
/// session began.
fn write(message: &str) {
    let Some((file, began)) = SINK.get() else {
        return;
    };
    // A panic while logging must still be able to log.
    let mut file = file.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = writeln!(file, "[{:9.3}] {message}", began.elapsed().as_secs_f64());
}

/// Record a diagnostic.
pub(crate) fn log(message: &str) {
    eprintln!("{message}");
    write(message);
}

#[cfg(test)]
mod tests {
    use super::{LOG, open, tail};
    use std::io::Write;

    #[test]
    fn a_long_log_keeps_its_end_from_a_whole_line() {
        let folder = tempfile::tempdir().expect("temporary folder");
        let path = folder.path().join(LOG);
        std::fs::write(&path, "first line\nsecond line\nthird line\n").expect("write");

        assert_eq!(tail(&path, 15), "third line\n");
        assert_eq!(tail(&path, 1024), "first line\nsecond line\nthird line\n");
        assert_eq!(tail(&folder.path().join("missing"), 1024), "");
    }

    #[test]
    fn a_session_appends_to_what_earlier_ones_wrote() {
        let folder = tempfile::tempdir().expect("temporary folder");
        let logs = folder.path().join("logs");
        let mut file = open(&logs).expect("first session");
        writeln!(file, "first session").expect("write");
        drop(file);
        let mut file = open(&logs).expect("second session");
        writeln!(file, "second session").expect("write");

        assert_eq!(
            std::fs::read_to_string(logs.join(LOG)).expect("read"),
            "first session\nsecond session\n"
        );
    }
}
