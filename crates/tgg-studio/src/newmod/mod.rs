//! New mod: starting a code mod on this computer, from the installed
//! tgg-melee's SDK. The player names it, picks where it goes and which of
//! the SDK's examples it starts from, and sees whether the tools that build
//! mods are installed; then it's made (its files, a git repository) and the
//! next steps show: the commands that build and run it, its folder, the
//! docs. The app never builds or publishes a mod: that's the `tgg` command
//! and textures.gg. `actions` changes it; `view` draws the dialog;
//! `library` is what it asks tgg-mod.

pub mod actions;
pub mod library;
pub mod view;

use library::{CreatedMod, Progress, Sdk, Tools};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// The example a new mod starts from unless the player picks another.
pub const TEMPLATE: &str = "template";

/// The New mod dialog's state.
#[derive(Default)]
pub struct NewMod {
    /// The dialog shows.
    pub open: bool,
    /// The mod's name, as the player types it.
    pub name: rune::TextEdit,
    /// Its id, when the player types one; else it comes from the name.
    pub id: rune::TextEdit,
    /// The folder the mod's own folder (named by its id) goes in.
    pub parent: PathBuf,
    /// The installed tgg-melee's SDK and its examples, once it's here, or
    /// why it couldn't be had.
    pub sdk: Option<Result<(Sdk, Vec<String>), String>>,
    /// The SDK on its way (the first time): how far, and stopping it.
    pub fetching: Option<Fetching>,
    /// The example it starts from.
    pub example: String,
    /// The examples' menu is open.
    pub menu: bool,
    /// Which of the tools that build mods are installed, once looked for.
    pub tools: Option<Tools>,
    pub stage: Stage,
}

#[derive(Default)]
pub enum Stage {
    /// The player is filling it in.
    #[default]
    Editing,
    Creating,
    Created(CreatedMod),
    Failed(String),
}

/// The SDK being downloaded.
#[derive(Clone, Default)]
pub struct Fetching {
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub cancel: Arc<AtomicBool>,
}

impl Fetching {
    /// How far along, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        let progress = self.progress.lock().ok().and_then(|latest| latest.clone());
        progress
            .and_then(|progress| {
                let total = progress.total.filter(|total| *total > 0)?;
                Some((progress.bytes as f32 / total as f32).clamp(0.0, 1.0))
            })
            .unwrap_or(0.0)
    }
}

impl NewMod {
    /// The SDK's examples, once it's here.
    pub fn examples(&self) -> Option<&[String]> {
        match &self.sdk {
            Some(Ok((_, examples))) => Some(examples),
            _ => None,
        }
    }

    /// The id the mod gets: the one typed, else one from the name.
    pub fn id_text(&self) -> String {
        let typed = self.id.text.trim();
        if typed.is_empty() {
            id_from(&self.name.text)
        } else {
            typed.to_owned()
        }
    }

    /// The id, when it's a valid one.
    pub fn mod_id(&self) -> Option<tgg_mod::ModId> {
        self.id_text().parse().ok()
    }

    /// The folder the mod is made in.
    pub fn folder(&self) -> PathBuf {
        self.parent.join(self.id_text())
    }
}

/// An id from a name: lowercase, spaces and other marks as `-`, only the
/// characters ids allow, at most 64 of them.
pub fn id_from(name: &str) -> String {
    let mut id = String::new();
    for character in name.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_') {
            id.push(character);
        } else if !id.ends_with('-') && !id.is_empty() {
            id.push('-');
        }
    }
    id.trim_end_matches('-').chars().take(64).collect()
}

/// Where new mods go unless the player picks a folder: `mods` in their
/// home folder.
pub fn default_parent() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mods")
}

#[cfg(test)]
mod tests {
    use super::id_from;

    #[test]
    fn an_id_comes_from_the_name() {
        assert_eq!(id_from("Neutral Spawns"), "neutral-spawns");
        assert_eq!(id_from("  Fox's  Shine!! "), "fox-s-shine");
        assert_eq!(id_from("ucf 0.84"), "ucf-0.84");
        assert_eq!(id_from("!!!"), "");
        assert_eq!(id_from(&"a".repeat(80)).len(), 64);
    }
}
