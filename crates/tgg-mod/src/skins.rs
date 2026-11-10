//! Skins in tgg-melee: `skins.json` in the game's data folder, which the
//! tgg.skins mod reads when the game starts, and whether a costume changes
//! only looks, as the game decides it.
//!
//! ```json
//! {
//!   "format": 1,
//!   "slots": {
//!     "PlFxOr.dat": {
//!       "path": "/home/me/.local/share/textures.gg/library/skins/9b88….dat",
//!       "sha256": "9b88…",
//!       "name": "Dark Fox"
//!     }
//!   }
//! }
//! ```
//!
//! - `format`: 1. A reader refuses a larger number; a writer keeps the
//!   number it read.
//! - `slots`: by the game file the skin replaces (a `.dat` or `.usd` in the
//!   disc's root, as the disc spells it), the skin's file, its SHA-256 when it
//!   was set, and the name it shows. Costumes and menu files load today;
//!   tgg.skins logs and skips other kinds.
//!
//! Writers write the whole file next to it and rename it over. The game reads
//! it once, when it starts, so a change applies at the next start.

use crate::error::Error;
use crate::ports::PortInstall;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// The file's name in the game's data folder.
pub const FILE: &str = "skins.json";
/// The id of the mod that loads them.
pub const LOADER: &str = "tgg.skins";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skins {
    pub format: u32,
    /// By the game file each skin replaces: `PlFxOr.dat`.
    #[serde(default)]
    pub slots: BTreeMap<String, SkinSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinSlot {
    /// The skin's file.
    pub path: PathBuf,
    /// Lowercase hex SHA-256 of the file when it was set.
    pub sha256: String,
    /// The name it shows.
    #[serde(default)]
    pub name: String,
}

impl Default for Skins {
    fn default() -> Self {
        Self {
            format: FORMAT,
            slots: BTreeMap::new(),
        }
    }
}

/// `skins.json` in the game's data folder ([`crate::game_data_dir`]).
pub fn skins_file() -> PathBuf {
    crate::install::game_data_dir().join(FILE)
}

/// The skins in `file`; none when it doesn't exist.
pub fn skins(file: &Path) -> Result<Skins, Error> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Skins::default());
        }
        Err(error) => return Err(Error::io(file)(error)),
    };
    let damaged = |why: String| Error::BadSkins {
        path: file.to_owned(),
        why,
    };
    #[derive(Deserialize)]
    struct Format {
        format: u32,
    }
    let format: Format = serde_json::from_slice(&bytes).map_err(|e| damaged(e.to_string()))?;
    if format.format > FORMAT {
        return Err(damaged(format!(
            "it's format {}; this tgg reads format {FORMAT}; update tgg",
            format.format
        )));
    }
    serde_json::from_slice(&bytes).map_err(|e| damaged(e.to_string()))
}

/// Whether `slot` is a game file a skin can replace: a `.dat` or `.usd` in
/// the disc's root whose name fits the game's file names.
pub fn is_slot(slot: &str) -> bool {
    let lower = slot.to_ascii_lowercase();
    (lower.ends_with(".dat") || lower.ends_with(".usd"))
        && slot.len() > 4
        && slot.len() < 32
        && slot
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
}

/// Put the skin at `path` in `slot` (`PlFxOr.dat`), shown as `name`,
/// replacing the slot's skin if it had one. Returns the skin's SHA-256.
pub fn set_skin(file: &Path, slot: &str, path: &Path, name: &str) -> Result<String, Error> {
    if !is_slot(slot) {
        return Err(Error::NotASkinSlot(slot.to_owned()));
    }
    let bytes = std::fs::read(path).map_err(Error::io(path))?;
    let sha256 = crate::package::sha256_hex(&bytes);
    let path = std::path::absolute(path).map_err(Error::io(path))?;
    let mut skins = skins(file)?;
    let name = if name.trim().is_empty() {
        slot
    } else {
        name.trim()
    };
    skins.slots.insert(
        slot.to_owned(),
        SkinSlot {
            path,
            sha256: sha256.clone(),
            name: name.to_owned(),
        },
    );
    write(file, &skins)?;
    Ok(sha256)
}

/// Take the skin out of `slot`. Returns whether it had one.
pub fn clear_skin(file: &Path, slot: &str) -> Result<bool, Error> {
    let mut skins = skins(file)?;
    let had = skins
        .slots
        .keys()
        .find(|have| have.eq_ignore_ascii_case(slot))
        .cloned();
    let Some(had) = had else {
        return Ok(false);
    };
    skins.slots.remove(&had);
    write(file, &skins)?;
    Ok(true)
}

fn write(file: &Path, skins: &Skins) -> Result<(), Error> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(Error::io(dir))?;
    }
    let staging = file.with_extension("json.new");
    let mut json = serde_json::to_string_pretty(skins).expect("skins serialize");
    json.push('\n');
    std::fs::write(&staging, json).map_err(Error::io(&staging))?;
    std::fs::rename(&staging, file).map_err(Error::io(file))
}

/// What the game makes of a costume, in netplay terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It changes only looks: it loads in netplay games too.
    LooksOnly,
    /// It changes more (what, in the game's words): it loads offline, and a
    /// netplay game shows the normal costume instead.
    Counts(String),
    /// The slot isn't a costume, which the game doesn't check.
    NotACostume,
}

/// Ask `port` whether the skin at `skin`, in place of the disc's `slot`,
/// changes only looks, against the disc image `iso`. The port compares it
/// with the clean disc's file, as it does when the game loads it.
pub fn costume_verdict(
    port: &PortInstall,
    iso: &Path,
    slot: &str,
    skin: &Path,
) -> Result<Verdict, Error> {
    if !is_slot(slot) {
        return Err(Error::NotASkinSlot(slot.to_owned()));
    }
    let executable = port.executable();
    let output = Command::new(&executable)
        .arg("--check-costume")
        .arg(slot)
        .arg(skin)
        .arg(iso)
        .env("TGG_HEADLESS", "1")
        .output()
        .map_err(Error::io(&executable))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().last().unwrap_or_default().trim();
    if line == "looks only" {
        Ok(Verdict::LooksOnly)
    } else if let Some(why) = line.strip_prefix("counts: ") {
        Ok(Verdict::Counts(why.to_owned()))
    } else if line == "not a costume" {
        Ok(Verdict::NotACostume)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Error::CheckCostume(
            stderr.lines().last().unwrap_or(line).trim().to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_holds_one_skin_and_clears_without_case() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join(FILE);
        assert_eq!(skins(&file).expect("none yet"), Skins::default());
        let red = dir.path().join("red.dat");
        std::fs::write(&red, b"red").expect("write");
        let blue = dir.path().join("blue.dat");
        std::fs::write(&blue, b"blue").expect("write");

        set_skin(&file, "PlFxOr.dat", &red, "Red").expect("set");
        let sha = set_skin(&file, "PlFxOr.dat", &blue, " ").expect("replace");
        let read = skins(&file).expect("read");
        let slot = &read.slots["PlFxOr.dat"];
        assert_eq!(
            (slot.path.as_path(), slot.sha256.as_str()),
            (blue.as_path(), sha.as_str())
        );
        assert_eq!(slot.name, "PlFxOr.dat", "no name shows the slot");

        assert!(matches!(
            set_skin(&file, "audio/x.dat", &red, ""),
            Err(Error::NotASkinSlot(_))
        ));
        assert!(clear_skin(&file, "plfxor.DAT").expect("clear"));
        assert!(!clear_skin(&file, "PlFxOr.dat").expect("already clear"));
        assert!(skins(&file).expect("read").slots.is_empty());

        std::fs::write(&file, r#"{"format": 2, "slots": {}}"#).expect("write");
        assert!(matches!(skins(&file), Err(Error::BadSkins { .. })));
    }
}
