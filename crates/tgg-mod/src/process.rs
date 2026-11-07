//! Whether a process is still alive, and whether it is a given program,
//! asked of the system without `unsafe`: `/proc` on Linux, `ps` on macOS
//! and `tasklist` on Windows.

use std::path::Path;

/// Whether a process `pid` exists.
pub(crate) fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if cfg!(target_os = "linux") {
        return Path::new("/proc").join(pid.to_string()).exists();
    }
    command_line(pid).is_some()
}

/// Whether a live process `pid` runs the program `executable`: the program
/// itself, or a script run by an interpreter.
pub(crate) fn runs(pid: u32, executable: &Path) -> bool {
    let Some(name) = executable.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(line) = command_line(pid) else {
        return false;
    };
    let matches = |part: &str| {
        let part = part.trim().trim_matches('"');
        Path::new(part).file_name().and_then(|n| n.to_str()) == Some(name)
            || part.eq_ignore_ascii_case(name)
    };
    line.iter().take(2).any(|part| matches(part))
}

/// The first parts of process `pid`'s command line, or `None` when there is
/// no such process.
fn command_line(pid: u32) -> Option<Vec<String>> {
    if pid == 0 {
        return None;
    }
    if cfg!(target_os = "linux") {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        // A zombie has an empty command line: it has ended.
        if bytes.is_empty() {
            return None;
        }
        return Some(
            bytes
                .split(|&b| b == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect(),
        );
    }
    if cfg!(windows) {
        // CSV: "image name","pid",...
        let output = quiet(std::process::Command::new("tasklist").args([
            "/FI",
            &format!("PID eq {pid}"),
            "/FO",
            "CSV",
            "/NH",
        ]))?;
        let line = output
            .lines()
            .find(|line| line.contains(&format!("\"{pid}\"")))?;
        let image = line.split("\",\"").next()?.trim_start_matches('"');
        return Some(vec![image.to_owned()]);
    }
    let output =
        quiet(std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "command="]))?;
    let line = output.trim();
    if line.is_empty() {
        return None;
    }
    Some(line.split_whitespace().take(2).map(str::to_owned).collect())
}

/// `command`'s standard output when it succeeds, with no console window.
fn quiet(command: &mut std::process::Command) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.stderr(std::process::Stdio::null()).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive() {
        assert!(alive(std::process::id()));
        assert!(!alive(0));
    }
}
