//! Ending the game with tgg: when tgg is told to stop (Ctrl-C, or SIGTERM
//! from `timeout` or a service manager), the game it started is told too,
//! and killed if it doesn't end within a few seconds. Ctrl-C in a terminal
//! reaches the game anyway (it shares tgg's process group); SIGTERM goes to
//! tgg alone, so tgg passes it on.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// How long the game gets to end on its own before it's killed.
const GRACE: Duration = Duration::from_secs(5);

pub struct Signals {
    stop: AtomicBool,
    /// The game's process id, or 0 when none is running.
    game: AtomicU32,
}

static SIGNALS: OnceLock<Arc<Signals>> = OnceLock::new();

/// Start handling Ctrl-C and SIGTERM; the same handler for every caller.
pub fn handle() -> Result<Arc<Signals>> {
    if let Some(signals) = SIGNALS.get() {
        return Ok(signals.clone());
    }
    let signals = Arc::new(Signals {
        stop: AtomicBool::new(false),
        game: AtomicU32::new(0),
    });
    let handler = signals.clone();
    // ctrlc runs this on its own thread, so it may wait.
    ctrlc::set_handler(move || {
        handler.stop.store(true, Ordering::SeqCst);
        let pid = handler.game.load(Ordering::SeqCst);
        if pid != 0 {
            terminate(pid);
            std::thread::sleep(GRACE);
            if handler.game.load(Ordering::SeqCst) == pid {
                kill(pid);
            }
        }
    })
    .context("handling Ctrl-C and SIGTERM")?;
    Ok(SIGNALS.get_or_init(|| signals).clone())
}

impl Signals {
    /// Whether tgg has been told to stop.
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// The game now running, or none (`None` after it ends).
    pub fn set_game(&self, pid: Option<u32>) {
        self.game.store(pid.unwrap_or(0), Ordering::SeqCst);
    }
}

#[cfg(unix)]
fn terminate(pid: u32) {
    use nix::sys::signal::{Signal, kill};
    let _ = kill(nix::unistd::Pid::from_raw(pid as i32), Signal::SIGTERM);
}

#[cfg(unix)]
fn kill(pid: u32) {
    use nix::sys::signal::{Signal, kill};
    let _ = kill(nix::unistd::Pid::from_raw(pid as i32), Signal::SIGKILL);
}

#[cfg(not(unix))]
fn terminate(_: u32) {}

#[cfg(not(unix))]
fn kill(_: u32) {}

/// Run `command` to the end with its output inherited, passing Ctrl-C and
/// SIGTERM on to it.
pub fn run(command: &mut std::process::Command) -> Result<std::process::ExitStatus> {
    let signals = handle()?;
    let mut child = command.spawn()?;
    signals.set_game(Some(child.id()));
    let status = child.wait();
    signals.set_game(None);
    Ok(status?)
}
