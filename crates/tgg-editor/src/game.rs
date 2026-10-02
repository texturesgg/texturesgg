//! The player's Melee: finding their NTSC 1.02 ISO, checking it, and reading
//! from it the reference files a stock costume's animation and texture names
//! need, a few at a time as each costume opens.
//!
//! The ISO is found where the player already keeps it: the one they chose
//! before, Slippi Launcher's ISO setting, or the ISO folders Slippi's Dolphin
//! scans.

use crate::Error;
use dat_parser::hsd::scene::HsdScene;
use gc_iso::Disc;
use melee_dat::vanilla::{is_vanilla, vanilla_files};
use melee_dat::{MeleeReferenceCatalog, MeleeReferenceStore};
use std::cell::RefCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Melee NTSC's game ID, and 1.02's revision: the version Slippi plays and
/// the reference catalog was verified against.
const GAME_ID: &str = "GALE01";
const REVISION: u8 = 2;

/// Why a disc image can't be the player's game, in words for the player.
#[derive(Clone, Debug, PartialEq)]
pub enum GameError {
    Unreadable { path: PathBuf, error: String },
    NotMelee { game_id: String, title: String },
    WrongRevision { revision: u8 },
}

impl std::error::Error for GameError {}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, error } => write!(
                f,
                "{} isn't a GameCube disc image the editor can read ({error}).",
                path.display()
            ),
            Self::NotMelee { game_id, title } => write!(
                f,
                "That's {title} ({game_id}), not Melee. Choose your NTSC Super Smash Bros. Melee ISO."
            ),
            Self::WrongRevision { revision } => write!(
                f,
                "That's Melee 1.0{revision}. The editor needs NTSC 1.02, the version Slippi plays."
            ),
        }
    }
}

/// Where a game was found, to tell the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// The ISO given on the command line.
    Given,
    /// The ISO they chose last time.
    Saved,
    SlippiLauncher,
    /// A folder Slippi's Dolphin scans for games.
    DolphinFolder,
}

/// A Melee NTSC 1.02 ISO in the player's game folders, for them to choose.
#[derive(Clone, Debug, PartialEq)]
pub struct GameChoice {
    pub path: PathBuf,
    /// Whether Slippi Launcher plays this ISO; skins installed elsewhere
    /// don't show in Slippi until the player switches to it there.
    pub slippi_plays: bool,
    /// How many costume and stage files differ from vanilla.
    pub changed: usize,
}

/// A checked Melee NTSC 1.02 disc image.
pub struct Game {
    path: PathBuf,
    disc: RefCell<Disc>,
}

impl Game {
    /// Open the image at `path` if it is Melee NTSC 1.02.
    pub fn open(path: &Path) -> Result<Self, GameError> {
        let disc = Disc::open(path).map_err(|error| GameError::Unreadable {
            path: path.to_owned(),
            error: error.to_string(),
        })?;
        let header = disc.header();
        if header.game_id != GAME_ID {
            return Err(GameError::NotMelee {
                game_id: header.game_id.clone(),
                title: header.title.clone(),
            });
        }
        if header.revision != REVISION {
            return Err(GameError::WrongRevision {
                revision: header.revision,
            });
        }
        Ok(Self {
            path: path.to_owned(),
            disc: RefCell::new(disc),
        })
    }

    /// Find the player's game where they already keep it.
    pub fn find(saved: Option<&Path>) -> Option<(Self, Found)> {
        let candidates = saved
            .map(|path| (path.to_owned(), Found::Saved))
            .into_iter()
            .chain(slippi_launcher_iso().map(|path| (path, Found::SlippiLauncher)))
            .chain(
                dolphin_iso_folders()
                    .into_iter()
                    .flat_map(|folder| disc_images_in(&folder))
                    .map(|path| (path, Found::DolphinFolder)),
            );
        for (path, found) in candidates {
            match Self::open(&path) {
                Ok(game) => return Some((game, found)),
                Err(error) => crate::log(&format!("not using {}: {error}", path.display())),
            }
        }
        None
    }

    /// Every Melee NTSC 1.02 ISO in the player's game folders: Slippi
    /// Launcher's ISO and the folder it's in, the folders Slippi's Dolphin
    /// scans, and `folders` the player added. Slippi's ISO comes first.
    pub fn discover(folders: &[PathBuf]) -> Vec<GameChoice> {
        let slippi = slippi_launcher_iso();
        let folders = slippi
            .iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .chain(dolphin_iso_folders())
            .chain(folders.iter().cloned());
        let mut seen = Vec::new();
        let mut choices = Vec::new();
        for path in slippi
            .iter()
            .cloned()
            .chain(folders.flat_map(|folder| disc_images_in(&folder)))
        {
            let identity = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.contains(&identity) {
                continue;
            }
            seen.push(identity.clone());
            match Self::open(&path) {
                Ok(game) => choices.push(GameChoice {
                    slippi_plays: slippi.as_ref().is_some_and(|slippi| {
                        std::fs::canonicalize(slippi).unwrap_or_else(|_| slippi.clone()) == identity
                    }),
                    changed: game.changed_files().len(),
                    path,
                }),
                Err(error) => crate::log(&format!("not listing {}: {error}", path.display())),
            }
        }
        choices.sort_by_key(|choice| !choice.slippi_plays);
        choices
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The costume and stage files on the disc that differ from vanilla.
    pub fn changed_files(&self) -> Vec<String> {
        let mut changed: Vec<String> = vanilla_files()
            .filter(|file| {
                self.read(&file.name)
                    .is_ok_and(|bytes| !is_vanilla(&file.name, &bytes))
            })
            .map(|file| file.name.clone())
            .collect();
        changed.sort();
        changed
    }

    /// The names of every file on the disc.
    pub fn file_names(&self) -> Vec<String> {
        let disc = self.disc.borrow();
        disc.files()
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Replace the game file `name` with `bytes`, in place (a larger file
    /// moves to the end of the disc), and read it back. Refused while another
    /// program, such as Dolphin, has the ISO open.
    pub fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        if let Some(program) = holder_of(&self.path) {
            return Err(Error::DiscHeld { program });
        }
        let name = || name.to_owned();
        gc_iso::replace_file(&self.path, &name(), bytes).map_err(|source| Error::DiscWrite {
            name: name(),
            source,
        })?;
        // The file table may now point elsewhere; read it afresh.
        *self.disc.borrow_mut() = Disc::open(&self.path)?;
        if self.read(&name())? != bytes {
            return Err(Error::DiscReadBack { name: name() });
        }
        Ok(())
    }

    /// Read the file called `name` from the disc.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        self.disc
            .borrow_mut()
            .read(name)
            .map_err(|source| Error::DiscRead {
                name: name.to_owned(),
                source,
            })
    }
}

/// The reference catalog and the player's game, which holds the files it
/// names.
pub struct References {
    pub catalog: MeleeReferenceCatalog,
    game: Game,
}

impl References {
    pub fn new(catalog: MeleeReferenceCatalog, game: Game) -> Self {
        Self { catalog, game }
    }

    /// The player's game.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The reference files the costume `scene` describes needs, read from
    /// the game, or `None` when it isn't a stock costume the catalog knows.
    /// The store checks each file against the catalog.
    pub fn store_for(&self, scene: &HsdScene) -> Option<Rc<MeleeReferenceStore>> {
        MeleeReferenceStore::for_costume(&self.catalog, scene, |name| {
            self.game
                .read(name)
                .inspect_err(|error| crate::log(&format!("reference unavailable: {error}")))
                .ok()
        })
        .map(Rc::new)
    }
}

/// The name of another program that has the file at `path` open, such as
/// Dolphin running the game. Known on Linux, from `/proc`; elsewhere `None`.
fn holder_of(path: &Path) -> Option<String> {
    let target = std::fs::canonicalize(path).ok()?;
    let own = std::process::id().to_string();
    let processes = std::fs::read_dir("/proc").ok()?;
    processes.flatten().find_map(|process| {
        let pid = process.file_name().to_string_lossy().into_owned();
        if pid == own || !pid.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let holds = std::fs::read_dir(process.path().join("fd"))
            .ok()?
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|link| link == target));
        holds.then(|| {
            std::fs::read_to_string(process.path().join("comm")).map_or_else(
                |_| "Another program".to_owned(),
                |name| name.trim().to_owned(),
            )
        })
    })
}

/// The ISO Slippi Launcher is set to play.
fn slippi_launcher_iso() -> Option<PathBuf> {
    let settings = std::fs::read_to_string(slippi_launcher_settings()?).ok()?;
    let json: serde_json::Value = serde_json::from_str(&settings).ok()?;
    json.get("settings")?
        .get("isoPath")?
        .as_str()
        .map(PathBuf::from)
}

fn slippi_launcher_settings() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Slippi Launcher").join("Settings"))
}

/// The folders Slippi's netplay Dolphin lists as `ISOPath0`, `ISOPath1`, ...
/// in its `Dolphin.ini`. Its location is known on Linux only.
fn dolphin_iso_folders() -> Vec<PathBuf> {
    let Some(ini) = dirs::config_dir().map(|config| config.join("SlippiOnline/Config/Dolphin.ini"))
    else {
        return Vec::new();
    };
    std::fs::read_to_string(ini)
        .map(|text| iso_paths(&text))
        .unwrap_or_default()
}

fn iso_paths(ini: &str) -> Vec<PathBuf> {
    ini.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let index = key.trim().strip_prefix("ISOPath")?;
            (!index.is_empty() && index.chars().all(|c| c.is_ascii_digit()))
                .then(|| PathBuf::from(value.trim()))
        })
        .collect()
}

/// The disc images directly in `folder`, by extension.
fn disc_images_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(listing) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut images: Vec<PathBuf> = listing
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|extension| {
                ["iso", "gcm"]
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            })
        })
        .collect();
    images.sort();
    images
}

/// A disc image of game `game_id` at `revision` holding one file,
/// `PlFcRe.dat`.
#[cfg(test)]
pub(crate) fn test_disc(game_id: &[u8; 6], revision: u8, contents: &[u8]) -> Vec<u8> {
    const FST: usize = 0x500;
    const FILE: usize = 0x800;
    const NAME: &[u8] = b"PlFcRe.dat";
    let mut disc = vec![0; FILE + contents.len()];
    disc[0..6].copy_from_slice(game_id);
    disc[7] = revision;
    disc[0x1C..0x20].copy_from_slice(&0xC233_9F3D_u32.to_be_bytes());
    disc[0x424..0x428].copy_from_slice(&(FST as u32).to_be_bytes());
    disc[0x428..0x42c].copy_from_slice(&((24 + NAME.len() + 1) as u32).to_be_bytes());
    disc[FST] = 1;
    disc[FST + 8..FST + 12].copy_from_slice(&2_u32.to_be_bytes());
    disc[FST + 16..FST + 20].copy_from_slice(&(FILE as u32).to_be_bytes());
    disc[FST + 20..FST + 24].copy_from_slice(&(contents.len() as u32).to_be_bytes());
    disc[FST + 24..FST + 24 + NAME.len()].copy_from_slice(NAME);
    disc[FILE..].copy_from_slice(contents);
    disc
}

#[cfg(test)]
mod tests {
    use super::{Game, GameError, iso_paths, test_disc};
    use std::path::PathBuf;

    #[test]
    fn dolphin_lists_its_iso_folders_by_index() {
        let ini =
            "[General]\nISOPaths = 2\nISOPath0 = /games/melee\nISOPath1 = /more\nISOPathX = no\n";
        assert_eq!(
            iso_paths(ini),
            [PathBuf::from("/games/melee"), PathBuf::from("/more")]
        );
    }

    /// Installing writes into the disc, so only the one the app knows is
    /// opened; another is refused in words that say what it is.
    #[test]
    fn only_melee_ntsc_1_02_opens() {
        let folder = tempfile::tempdir().expect("temp folder");
        let open = |game_id: &[u8; 6], revision: u8| {
            let iso = folder.path().join("game.iso");
            std::fs::write(&iso, test_disc(game_id, revision, b"file")).expect("write disc");
            Game::open(&iso).map(|_| ())
        };
        assert!(open(b"GALE01", 2).is_ok());

        let older = open(b"GALE01", 0).expect_err("1.00 is refused");
        assert!(matches!(older, GameError::WrongRevision { revision: 0 }));
        assert!(older.to_string().contains("1.00"), "{older}");

        let other = open(b"GMSE01", 2).expect_err("another game is refused");
        assert!(matches!(&other, GameError::NotMelee { game_id, .. } if game_id == "GMSE01"));
        assert!(other.to_string().contains("GMSE01"), "{other}");
    }
}
