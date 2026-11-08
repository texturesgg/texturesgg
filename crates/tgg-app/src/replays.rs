//! Finding the player's replays: the folder Slippi records into (the
//! Launcher's `rootSlpPath`, else `Slippi` in their home, or in Documents),
//! and listing the `.slp` files in folders, newest first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A replay file, and when it was last written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayFile {
    pub path: PathBuf,
    pub modified: SystemTime,
    /// Which of the folders listed it was found in.
    pub folder: usize,
}

/// The folder Slippi records replays into, if there is one.
pub fn slippi_folder() -> Option<PathBuf> {
    let from_settings = dirs::config_dir()
        .map(|config| config.join("Slippi Launcher").join("Settings"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json.get("settings")?.get("rootSlpPath")?.as_str().map(PathBuf::from));
    let home = dirs::home_dir();
    from_settings
        .into_iter()
        .chain(home.iter().map(|home| home.join("Slippi")))
        .chain(dirs::document_dir().map(|documents| documents.join("Slippi")))
        .find(|folder| folder.is_dir())
}

/// The `.slp` files in `folders` and the folders under them, newest first,
/// at most `limit` of them.
pub fn replays_in(folders: &[PathBuf], limit: usize) -> Vec<ReplayFile> {
    let mut found = Vec::new();
    for (index, folder) in folders.iter().enumerate() {
        walk(folder, index, 0, &mut found);
    }
    found.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    found.truncate(limit);
    found
}

/// How deep under a folder replays are looked for (Slippi files them by
/// month).
const DEPTH: usize = 4;

fn walk(folder: &Path, index: usize, depth: usize, found: &mut Vec<ReplayFile>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if depth < DEPTH {
                walk(&path, index, depth + 1, found);
            }
        } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("slp")) {
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            found.push(ReplayFile { path, modified, folder: index });
        }
    }
}

/// When a replay was played, from Slippi's file name (`Game_20260815T011035`),
/// as "2026-08-15 01:10".
pub fn played_at(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let stamp = stem.strip_prefix("Game_")?;
    let (date, time) = stamp.split_once('T')?;
    (date.len() == 8 && time.len() >= 4 && date.chars().chain(time.chars().take(4)).all(|c| c.is_ascii_digit())).then(|| {
        format!("{}-{}-{} {}:{}", &date[..4], &date[4..6], &date[6..8], &time[..2], &time[2..4])
    })
}

#[cfg(test)]
mod tests {
    use super::{played_at, replays_in};
    use std::path::Path;

    #[test]
    fn a_slippi_file_name_says_when() {
        assert_eq!(played_at(Path::new("/x/Game_20260815T011035.slp")).as_deref(), Some("2026-08-15 01:10"));
        assert_eq!(played_at(Path::new("/x/clip.slp")), None);
    }

    #[test]
    fn replays_are_found_under_folders_newest_first() {
        let folder = tempfile::tempdir().expect("temp");
        let month = folder.path().join("2026-10");
        std::fs::create_dir_all(&month).expect("month");
        std::fs::write(month.join("old.slp"), b"x").expect("write");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(folder.path().join("new.slp"), b"x").expect("write");
        std::fs::write(folder.path().join("notes.txt"), b"x").expect("write");
        let found = replays_in(&[folder.path().to_owned()], 10);
        let names: Vec<_> = found.iter().map(|file| file.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["new.slp", "old.slp"]);
        assert_eq!(replays_in(&[folder.path().to_owned()], 1).len(), 1);
    }
}
