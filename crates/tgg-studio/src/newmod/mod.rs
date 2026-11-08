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

use library::{CreatedMod, Tools};
use std::path::PathBuf;

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
    /// The SDK's examples, once read, or why they couldn't be.
    pub examples: Option<Result<Vec<String>, String>>,
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

impl NewMod {
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
