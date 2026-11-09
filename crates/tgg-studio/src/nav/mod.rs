//! Where the player is: the place the sidebar chose, and whether the sidebar
//! shows. `actions` changes it.

pub mod actions;

use tgg_app::settings::Settings;

/// The places the sidebar lists, by job: which game a thing is about is
/// said where it matters, not by where it sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Each game, Slippi and tgg-melee, ready to start, and what's missing.
    Play,
    /// The skins in the player's game, and the skins they have.
    Skins,
    /// Mods from textures.gg for tgg-melee.
    Mods,
    /// Their Slippi and tgg-melee replays.
    Replays,
    /// Making mods of their own.
    Create,
}

impl Place {
    pub const ALL: [Place; 5] = [
        Place::Play,
        Place::Skins,
        Place::Mods,
        Place::Replays,
        Place::Create,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Place::Play => "Play",
            Place::Skins => "Skins",
            Place::Mods => "Mods",
            Place::Replays => "Replays",
            Place::Create => "Create",
        }
    }

    /// The element key of its row in the sidebar.
    pub fn key(self) -> &'static str {
        match self {
            Place::Play => "sidebar/place/play",
            Place::Skins => "sidebar/place/skins",
            Place::Mods => "sidebar/place/mods",
            Place::Replays => "sidebar/place/replays",
            Place::Create => "sidebar/place/create",
        }
    }
}

pub struct Nav {
    pub place: Place,
    /// The sidebar shows, as the player left it.
    pub sidebar: bool,
}

impl Nav {
    /// Play, with the sidebar as the settings remember it (shown by default).
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            place: Place::Play,
            sidebar: settings.sidebar.unwrap_or(true),
        }
    }
}
