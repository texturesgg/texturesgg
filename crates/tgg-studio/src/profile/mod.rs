//! You: who's signed in to textures.gg, as their profile on the site shows
//! them, then what they've made. On this computer: the skins they saved
//! from the editor and the mods they're making. Published: their packs,
//! their code mods and the packs they liked, as the site lists them, each
//! pack a press from the player's library. It opens from the sidebar's
//! account row, and signing out is here. `actions` changes it; `view` draws
//! it.

pub mod actions;
pub mod view;

use serde::Deserialize;
use std::collections::HashMap;

/// What You has read from textures.gg, for the account it was read for.
#[derive(Default)]
pub struct Profile {
    /// The handle it was read for; read again when someone else signs in.
    pub of: Option<String>,
    /// Their profile as the site shows it; `None` while it's on its way.
    pub user: Option<Result<User, String>>,
    pub packs: Option<Result<Vec<Pack>, String>>,
    pub likes: Option<Result<Vec<Pack>, String>>,
    pub mods: Option<Result<Vec<CodeMod>, String>>,
    /// Which of the published lists shows.
    pub tab: Tab,
    /// Pictures by URL: the avatar and pack thumbnails, filled as they arrive.
    pub pictures: HashMap<String, rune::Pixels>,
    /// The pack being downloaded into the library.
    pub fetching: Option<String>,
}

/// The published lists, as the site's profile tabs them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Packs,
    Mods,
    Likes,
}

/// A profile, as `GET /api/users/:handle` gives it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub name: String,
    pub image: Option<String>,
    pub pronouns: Option<String>,
    pub bio: Option<String>,
    pub created_at: String,
    pub stats: Stats,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub pack_count: u64,
    pub download_count: u64,
    pub like_count: u64,
}

/// A pack, as the uploads and likes lists give it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub thumbnail_key: Option<String>,
    #[serde(default)]
    pub vote_count: u64,
    /// Uploads say the game's slug flat; likes nest the game.
    #[serde(default)]
    pub game_slug: Option<String>,
    #[serde(default)]
    pub game: Option<Named>,
    #[serde(default)]
    pub target_name: Option<String>,
    #[serde(default)]
    pub target: Option<Named>,
    /// Who made it: on likes, someone else.
    #[serde(default)]
    pub creator_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Named {
    pub name: Option<String>,
    pub slug: Option<String>,
}

impl Pack {
    pub fn game(&self) -> Option<&str> {
        self.game_slug
            .as_deref()
            .or_else(|| self.game.as_ref()?.slug.as_deref())
    }

    /// What it's for: "Zelda", "Final Destination".
    pub fn target(&self) -> Option<&str> {
        self.target_name
            .as_deref()
            .or_else(|| self.target.as_ref()?.name.as_deref())
    }

    /// Whether it's for Melee, the game the app installs into.
    pub fn for_melee(&self) -> bool {
        self.game() == Some(MELEE)
    }
}

/// A code mod, as `GET /api/code-mods?owner=` lists it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeMod {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The game's slug.
    #[serde(default)]
    pub game: Option<String>,
}

/// One page of a list, as the site pages them.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
}

/// A pack's page: its files, which say what a lone one is called.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPage {
    pub mods: Vec<PackFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub file_name: String,
}

/// Melee's slug on textures.gg.
pub const MELEE: &str = "melee";
/// Where textures.gg keeps pictures.
pub const ASSETS: &str = "https://assets.textures.gg";
/// textures.gg itself, for opening pages in the browser.
pub const SITE: &str = "https://textures.gg";
/// How many of each list You shows: the site has the rest.
pub const SHOWN: usize = 24;

/// What a profile job found.
pub enum Done {
    /// The profile and its lists, read for `handle`.
    Read {
        handle: String,
        user: Box<Result<User, String>>,
        packs: Result<Vec<Pack>, String>,
        likes: Result<Vec<Pack>, String>,
        mods: Result<Vec<CodeMod>, String>,
    },
    /// A pack downloaded to this file, or why not.
    Fetched {
        title: String,
        file: Result<std::path::PathBuf, String>,
    },
}
