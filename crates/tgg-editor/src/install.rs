//! Installing skins into the player's ISO, in place, with a way back.
//!
//! Before a slot is overwritten, its file is kept in the library and pushed
//! onto that slot's history for the game, so an install can be undone and a
//! slot that was ever vanilla can be restored. The history lives beside the
//! library, one file per game:
//!
//! ```text
//! <data dir>/textures.gg/games/<sha256 of the ISO's path>.json
//! ```

use crate::Error;
use crate::game::Game;
use crate::library::{Library, Skin};
use melee_dat::vanilla::is_vanilla;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a slot in the player's game holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotState {
    Vanilla,
    /// A skin from the player's library.
    Skin(Skin),
    /// A file the app doesn't know: installed by another tool, or edited.
    Custom,
}

impl SlotState {
    /// The state of slot `name` holding `bytes`.
    pub fn of(name: &str, bytes: &[u8], library: &Library) -> Self {
        if is_vanilla(name, bytes) {
            Self::Vanilla
        } else if let Some(skin) = library.skin_for(bytes) {
            Self::Skin(skin.clone())
        } else {
            Self::Custom
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameHistory {
    path: PathBuf,
    /// Each slot's earlier files, as library ids, newest last.
    slots: BTreeMap<String, Vec<String>>,
}

/// The install history of every game the player has installed into.
pub struct History {
    root: PathBuf,
}

impl History {
    /// The history beside the library in the app's data folder.
    pub fn open() -> Self {
        Self::open_at(
            dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("textures.gg")
                .join("games"),
        )
    }

    pub fn open_at(root: PathBuf) -> Self {
        Self { root }
    }

    /// Whether slot `name` of `game` has an install to undo.
    pub fn can_undo(&self, game: &Game, name: &str) -> bool {
        self.load(game.path())
            .slots
            .get(name)
            .is_some_and(|earlier| !earlier.is_empty())
    }

    fn file(&self, game: &Path) -> PathBuf {
        let key = format!("{:x}", Sha256::digest(game.to_string_lossy().as_bytes()));
        self.root.join(format!("{key}.json"))
    }

    fn load(&self, game: &Path) -> GameHistory {
        std::fs::read_to_string(self.file(game))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| GameHistory {
                path: game.to_owned(),
                ..GameHistory::default()
            })
    }

    fn save(&self, history: &GameHistory) -> Result<(), Error> {
        let mut json = serde_json::to_string_pretty(history).expect("history serializes");
        json.push('\n');
        std::fs::create_dir_all(&self.root)
            .and_then(|()| std::fs::write(self.file(&history.path), json))
            .map_err(Error::HistorySave)
    }
}

/// Install `bytes` into slot `name` of `game`, keeping the slot's current
/// file in `library` and on its history first, so it can come back.
pub fn install(
    game: &Game,
    name: &str,
    bytes: &[u8],
    library: &Library,
    history: &History,
) -> Result<(), Error> {
    let current = game.read(name)?;
    if current == bytes {
        return Ok(());
    }
    let kept = library.keep(&current)?;
    let mut record = history.load(game.path());
    record.slots.entry(name.to_owned()).or_default().push(kept);
    history.save(&record)?;
    if let Err(error) = game.replace(name, bytes) {
        // Nothing changed on the disc; forget the step.
        if let Some(earlier) = record.slots.get_mut(name) {
            earlier.pop();
        }
        history.save(&record)?;
        return Err(error);
    }
    Ok(())
}

/// Put back what slot `name` held before its last install.
pub fn undo(game: &Game, name: &str, library: &Library, history: &History) -> Result<(), Error> {
    let mut record = history.load(game.path());
    let earlier = record
        .slots
        .get(name)
        .and_then(|earlier| earlier.last())
        .cloned()
        .ok_or_else(|| Error::NothingToUndo(name.to_owned()))?;
    let bytes = library.read(&earlier)?;
    game.replace(name, &bytes)?;
    if let Some(earlier) = record.slots.get_mut(name) {
        earlier.pop();
    }
    history.save(&record)
}

/// Put slot `name` back to its vanilla file: one the history kept, or the
/// same file from another of the player's ISOs that still has it.
pub fn restore_vanilla(
    game: &Game,
    name: &str,
    others: &[PathBuf],
    library: &Library,
    history: &History,
) -> Result<(), Error> {
    let kept = history
        .load(game.path())
        .slots
        .get(name)
        .into_iter()
        .flatten()
        .filter_map(|id| library.read(id).ok())
        .find(|bytes| is_vanilla(name, bytes));
    let vanilla = kept.or_else(|| {
        others
            .iter()
            .filter(|other| *other != game.path())
            .filter_map(|other| Game::open(other).ok())
            .filter_map(|other| other.read(name).ok())
            .find(|bytes| is_vanilla(name, bytes))
    });
    let vanilla = vanilla.ok_or_else(|| Error::NoVanilla(name.to_owned()))?;
    install(game, name, &vanilla, library, history)
}

#[cfg(test)]
mod tests {
    use super::{History, SlotState, install, restore_vanilla, undo};
    use crate::Error;
    use crate::game::Game;
    use crate::library::Library;

    #[test]
    fn installs_undo_back_to_the_original_in_order() {
        let folder = tempfile::tempdir().expect("temp folder");
        let iso = folder.path().join("melee.iso");
        std::fs::write(&iso, crate::game::test_disc(b"GALE01", 2, b"original"))
            .expect("write disc");
        let game = Game::open(&iso).expect("a Melee 1.02 disc");
        let library = Library::open_at(folder.path().join("library"));
        let history = History::open_at(folder.path().join("games"));
        let slot = "PlFcRe.dat";

        install(&game, slot, b"skin one", &library, &history).expect("install");
        assert_eq!(game.read(slot).expect("read"), b"skin one");
        // A larger file moves to the end of the disc and still reads back.
        let larger = vec![7_u8; 4096];
        install(&game, slot, &larger, &library, &history).expect("install larger");
        assert_eq!(game.read(slot).expect("read"), larger);
        assert_eq!(
            SlotState::of(slot, &larger, &library),
            SlotState::Custom,
            "a file the library doesn't list is custom"
        );

        undo(&game, slot, &library, &history).expect("undo");
        assert_eq!(game.read(slot).expect("read"), b"skin one");
        undo(&game, slot, &library, &history).expect("undo again");
        assert_eq!(game.read(slot).expect("read"), b"original");
        assert!(!history.can_undo(&game, slot));
        assert!(matches!(
            undo(&game, slot, &library, &history),
            Err(Error::NothingToUndo(_))
        ));

        // No vanilla Falco Red has been seen, so there's nothing to restore.
        let restored = restore_vanilla(&game, slot, &[], &library, &history);
        assert!(matches!(restored, Err(Error::NoVanilla(_))));
    }
}
