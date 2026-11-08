//! `mods.lock.json`, the record of what tgg installed into a mods folder and
//! why, and the two files that keep a mods folder from changing under
//! someone: the busy file and the game's pid file.
//!
//! # `mods.lock.json`
//!
//! ```json
//! {
//!   "format": 1,
//!   "sets": [
//!     {
//!       "name": "competitive",
//!       "title": "Competitive",
//!       "description": "The Slippi-style setup.",
//!       "version": 3,
//!       "mods": [{ "id": "tgg.netplay", "range": "^0.2" }]
//!     }
//!   ],
//!   "mods": {
//!     "tgg.netplay": {
//!       "version": "0.2.0",
//!       "sha256": "9f86d0…",
//!       "sets": ["competitive"],
//!       "direct": false
//!     }
//!   }
//! }
//! ```
//!
//! - `format`: this layout is 1. A reader refuses a larger number; a writer
//!   keeps the number it read.
//! - `sets`: each installed set as the registry served it when it was last
//!   installed or updated, so its status needs no network.
//! - `mods`: by id, each mod tgg installed: its `version`, the `sha256` of
//!   the package it came from (absent for one tgg found already installed),
//!   the installed `sets` that list it, and `direct` when the player
//!   installed it on its own. A mod with no sets and not direct stays
//!   installed until the player removes it.
//!
//! A mod's folder, not the lock, says whether it is installed and whether it
//! is on (`<id>` or `.<id>`). A mod in the folder without an entry was put
//! there another way (a dev build, or by hand). Writers write the whole file
//! to `mods.lock.json.new` and rename it over.
//!
//! # Busy and running
//!
//! `.tgg-busy` holds the process id of the tgg changing the folder; it is
//! created exclusively and removed when done, and one whose process has
//! ended is stale. `.tgg-game.pid` holds the running game's process id and,
//! on the next line, its executable's path; it counts while a process with
//! that id runs that program.

use crate::error::Error;
use crate::manifest::ModId;
use crate::registry::Set;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The lock format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// The lock's name in the mods folder.
pub const FILE: &str = "mods.lock.json";
/// The busy file's name in the mods folder.
pub const BUSY_FILE: &str = ".tgg-busy";
/// The running game's pid file in the mods folder.
pub const PID_FILE: &str = ".tgg-game.pid";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    pub format: u32,
    #[serde(default)]
    pub sets: Vec<Set>,
    #[serde(default)]
    pub mods: BTreeMap<ModId, LockEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub version: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// The installed sets that list the mod.
    #[serde(default)]
    pub sets: Vec<String>,
    /// Installed on its own.
    #[serde(default)]
    pub direct: bool,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            format: FORMAT,
            sets: Vec::new(),
            mods: BTreeMap::new(),
        }
    }
}

impl Lock {
    /// The lock in `mods`, or an empty one when there is none.
    pub fn read(mods: &Path) -> Result<Self, Error> {
        let path = mods.join(FILE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(Error::io(path)(error)),
        };
        #[derive(Deserialize)]
        struct Format {
            format: u32,
        }
        let format: Format = serde_json::from_slice(&bytes)
            .map_err(|error| Error::LockDamaged(error.to_string()))?;
        if format.format > FORMAT {
            return Err(Error::LockFormat(format.format));
        }
        serde_json::from_slice(&bytes).map_err(|error| Error::LockDamaged(error.to_string()))
    }

    /// Write the lock into `mods` in one step.
    pub fn write(&self, mods: &Path) -> Result<(), Error> {
        let path = mods.join(FILE);
        let staging = mods.join(format!("{FILE}.new"));
        let mut json = serde_json::to_string_pretty(self).expect("a lock serializes");
        json.push('\n');
        std::fs::write(&staging, json).map_err(Error::io(&staging))?;
        std::fs::rename(&staging, &path).map_err(Error::io(path))
    }

    pub fn set(&self, name: &str) -> Option<&Set> {
        self.sets.iter().find(|set| set.name == name)
    }
}

/// Whether the game is running from `mods`, by its pid file. A pid file
/// whose game has ended is removed.
pub fn game_running(mods: &Path) -> bool {
    let path = mods.join(PID_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let mut lines = text.lines();
    let pid = lines.next().and_then(|pid| pid.trim().parse::<u32>().ok());
    let executable = lines.next().map(PathBuf::from);
    match (pid, executable) {
        (Some(pid), Some(executable)) if crate::process::runs(pid, &executable) => true,
        _ => {
            let _ = std::fs::remove_file(&path);
            false
        }
    }
}

/// Write the game's pid file into `mods`.
pub(crate) fn write_pid_file(mods: &Path, pid: u32, executable: &Path) -> Result<PathBuf, Error> {
    let path = mods.join(PID_FILE);
    std::fs::create_dir_all(mods).map_err(Error::io(mods))?;
    std::fs::write(&path, format!("{pid}\n{}\n", executable.display()))
        .map_err(Error::io(&path))?;
    Ok(path)
}

/// Holding the mods folder: the busy file, removed on drop.
pub(crate) struct Busy {
    path: PathBuf,
}

impl Drop for Busy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Take the mods folder for a change: [`Error::GameRunning`] while the game
/// runs, [`Error::Busy`] while another tgg holds it.
pub(crate) fn take(mods: &Path) -> Result<Busy, Error> {
    if game_running(mods) {
        return Err(Error::GameRunning);
    }
    std::fs::create_dir_all(mods).map_err(Error::io(mods))?;
    let path = mods.join(BUSY_FILE);
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(format!("{}\n", std::process::id()).as_bytes())
                    .map_err(Error::io(&path))?;
                return Ok(Busy { path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let holder = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                if holder.is_some_and(crate::process::alive) {
                    return Err(Error::Busy);
                }
                // Left by a tgg that ended without removing it.
                let _ = std::fs::remove_file(&path);
            }
            Err(error) => return Err(Error::io(path)(error)),
        }
    }
    Err(Error::Busy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_holder_makes_the_folder_busy_and_a_dead_one_doesnt() {
        let dir = tempfile::tempdir().expect("temp dir");
        let held = take(dir.path()).expect("take");
        assert!(matches!(take(dir.path()), Err(Error::Busy)));
        drop(held);
        // A pid no process has.
        std::fs::write(dir.path().join(BUSY_FILE), "4294967\n").expect("write");
        take(dir.path()).expect("stale busy file");
    }

    #[test]
    fn a_newer_lock_format_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join(FILE), r#"{"format": 2, "mods": {}}"#).expect("write");
        assert!(matches!(Lock::read(dir.path()), Err(Error::LockFormat(2))));
    }
}
