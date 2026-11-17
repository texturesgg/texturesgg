//! Slippi's netplay Dolphin, as Slippi Launcher installs it: finding it, and
//! starting it on a game, the way the launcher's Play does (`-b -e <iso>`,
//! straight into the game).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// Slippi's netplay Dolphin, when Slippi Launcher has installed it.
pub fn dolphin() -> Option<PathBuf> {
    dolphin_in(&dirs::config_dir()?.join("Slippi Launcher").join("netplay"))
}

/// The netplay Dolphin in `folder`, the launcher's `netplay` folder, for
/// this system.
fn dolphin_in(folder: &Path) -> Option<PathBuf> {
    let program = if cfg!(windows) {
        folder.join("Slippi Dolphin.exe")
    } else if cfg!(target_os = "macos") {
        folder.join("Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin")
    } else {
        folder.join("Slippi_Online-x86_64.AppImage")
    };
    program.is_file().then_some(program)
}

/// Start `dolphin` on the game at `iso`. Without this app's library path:
/// Dolphin brings its own libraries (the Linux AppImage), and a path set
/// for this app, as a development shell sets it, breaks it.
pub fn launch(dolphin: &Path, iso: &Path) -> std::io::Result<Child> {
    Command::new(dolphin)
        .arg("-b")
        .arg("-e")
        .arg(iso)
        .env_remove("LD_LIBRARY_PATH")
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dolphin_is_found_where_the_launcher_puts_it() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(dolphin_in(folder.path()), None);
        let program = if cfg!(windows) {
            folder.path().join("Slippi Dolphin.exe")
        } else if cfg!(target_os = "macos") {
            folder
                .path()
                .join("Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin")
        } else {
            folder.path().join("Slippi_Online-x86_64.AppImage")
        };
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"").unwrap();
        assert_eq!(dolphin_in(folder.path()), Some(program));
    }
}
