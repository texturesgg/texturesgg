//! Where the player is: the place the sidebar chose, and whether the sidebar
//! shows. `actions` changes it.

pub mod actions;

use tgg_app::settings::Settings;

/// The places the sidebar lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// What's in the player's game.
    Melee,
    /// Their skins.
    Library,
    /// Mods from textures.gg for their port of the game.
    Mods,
}

impl Place {
    pub const ALL: [Place; 3] = [Place::Melee, Place::Library, Place::Mods];

    pub fn label(self) -> &'static str {
        match self {
            Place::Melee => "Melee",
            Place::Library => "Library",
            Place::Mods => "Mods",
        }
    }

    /// The element key of its row in the sidebar.
    pub fn key(self) -> &'static str {
        match self {
            Place::Melee => "sidebar/place/melee",
            Place::Library => "sidebar/place/library",
            Place::Mods => "sidebar/place/mods",
        }
    }
}

pub struct Nav {
    pub place: Place,
    /// The sidebar shows, as the player left it.
    pub sidebar: bool,
}

impl Nav {
    /// Melee, with the sidebar as the settings remember it (shown by default).
    pub fn from_settings(settings: &Settings) -> Self {
        Self { place: Place::Melee, sidebar: settings.sidebar.unwrap_or(true) }
    }
}
