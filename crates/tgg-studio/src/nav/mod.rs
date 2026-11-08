//! Where the player is: the place the sidebar chose, and whether the sidebar
//! shows. `actions` changes it.

pub mod actions;

use tgg_app::settings::Settings;

/// The places the sidebar lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// What's in the player's ISO: its fighters and stages.
    Melee,
    /// Their skins.
    Library,
    /// Mods from textures.gg for their port of the game.
    Mods,
    /// Their Slippi and tgg-melee replays.
    Replays,
}

impl Place {
    pub const ALL: [Place; 4] = [Place::Melee, Place::Library, Place::Mods, Place::Replays];

    pub fn label(self) -> &'static str {
        match self {
            Place::Melee => "Fighters & stages",
            Place::Library => "Library",
            Place::Mods => "Mods",
            Place::Replays => "Replays",
        }
    }

    /// The sidebar group it sits in: the player's own ISO (what Slippi or
    /// Dolphin plays), or tgg-melee, the port Mods installs into.
    pub fn group(self) -> Group {
        match self {
            Place::Melee | Place::Library => Group::Iso,
            Place::Mods | Place::Replays => Group::Port,
        }
    }

    /// The element key of its row in the sidebar.
    pub fn key(self) -> &'static str {
        match self {
            Place::Melee => "sidebar/place/melee",
            Place::Library => "sidebar/place/library",
            Place::Mods => "sidebar/place/mods",
            Place::Replays => "sidebar/place/replays",
        }
    }
}

/// The sidebar's groups of places, in order, so which game a place is about
/// shows: the player's ISO, or tgg-melee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Iso,
    Port,
}

impl Group {
    pub const ALL: [Group; 2] = [Group::Iso, Group::Port];

    pub fn label(self) -> &'static str {
        match self {
            Group::Iso => "Slippi",
            Group::Port => "tgg-melee",
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
        Self {
            place: Place::Melee,
            sidebar: settings.sidebar.unwrap_or(true),
        }
    }
}
