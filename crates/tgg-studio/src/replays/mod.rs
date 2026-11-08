//! Replays: the player's Slippi and tgg-melee replays, newest first, each
//! saying whether it plays as is with the mods they have, or what it needs.
//! Play runs it with exactly the mods its game was played with, through
//! tgg-mod's profiles: as installed when they match, else in a folder of
//! its own (fetched and built, the player's mods untouched), after a review
//! of what it needs, and a yes for mods textures.gg doesn't vouch for.
//!
//! Reading the replays (their players, stage and the mods they need) is
//! disk only, on a job; textures.gg is asked for the catalog and the Slippi
//! mapping when the page opens or the player asks, and what it said last is
//! kept on disk. `actions` changes it; `view` draws the page and `review`
//! the dialog before a replay that needs a profile plays.

pub mod actions;
pub mod review;
pub mod view;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tgg_mod::{
    Catalog, Error, Prepared, Profile, ProfilePlan, Progress, ReplayInfo, SlippiMapping,
};

/// The most replays listed: the newest.
pub const MOST: usize = 300;

#[derive(Default)]
pub struct Replays {
    /// Where replays are looked for: a name, and the folder.
    pub folders: Vec<(String, PathBuf)>,
    pub items: Vec<Item>,
    pub scanning: bool,
    /// The replays have been read once this session.
    pub scanned: bool,
    /// textures.gg's Slippi mapping, as last fetched or kept.
    pub mapping: Option<SlippiMapping>,
    /// What's built for each game layout, as last fetched or kept.
    pub catalogs: Vec<(String, Catalog)>,
    /// Asking textures.gg for the catalogs and mapping.
    pub refreshing: bool,
    /// A replay's review before it plays.
    pub review: Option<Review>,
    /// A replay's mods being fetched and its folder built.
    pub preparing: Option<Preparing>,
}

/// A replay in the list.
pub struct Item {
    pub path: PathBuf,
    /// The folder it was found in, by name.
    pub source: String,
    /// Who played, on what, and who won.
    pub info: Option<ReplayInfo>,
    /// The mods its game was played with, or why that can't be read.
    pub profile: Result<Profile, String>,
    pub status: Status,
}

/// Whether a replay plays as is, and if not, what it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not worked out yet: no catalog or mapping to resolve against.
    Unknown,
    /// The installed mods match it.
    AsIs,
    /// It needs mods fetched (`mods` of them, `size` bytes), or another
    /// tgg-melee version (`game`).
    Needs {
        mods: usize,
        size: u64,
        game: Option<String>,
    },
    /// tgg-melee `version`, which can't be had.
    NoGame(String),
    /// It can't be read.
    Unreadable(String),
}

impl Status {
    /// Whether some of what it was played with can't be had (it may not
    /// play as it was), else only minor codes (likely as played).
    pub fn of_missing(plan: &ProfilePlan) -> Option<bool> {
        if plan.missing.is_empty() {
            return None;
        }
        Some(plan.missing.iter().all(tgg_mod::Missing::is_minor))
    }
}

/// A replay's plan, shown before it plays.
pub struct Review {
    pub path: PathBuf,
    pub plan: ProfilePlan,
    /// Remember the yes for the publishers it asks about.
    pub remember: bool,
}

/// A replay's profile being prepared: its progress and cancel flag.
pub struct Preparing {
    pub path: PathBuf,
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub cancel: Arc<AtomicBool>,
}

impl Preparing {
    /// How far along, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        let Some(progress) = self
            .progress
            .lock()
            .ok()
            .and_then(|progress| progress.clone())
        else {
            return 0.0;
        };
        let within = progress
            .total
            .filter(|total| *total > 0)
            .map_or(0.0, |total| progress.bytes as f32 / total as f32);
        ((progress.step as f32 + within.min(1.0)) / progress.steps.max(1) as f32).clamp(0.0, 1.0)
    }
}

/// A replay's raw reading, before it's resolved.
pub struct Read {
    pub path: PathBuf,
    pub source: String,
    pub info: Option<ReplayInfo>,
    pub profile: Result<Profile, String>,
}

/// What a Replays job found.
pub enum Done {
    /// The replays in the folders, read.
    Scanned(Vec<Read>),
    /// What textures.gg says now: the mapping, and catalogs by layout.
    Refreshed {
        mapping: Option<SlippiMapping>,
        catalogs: Vec<(String, Catalog)>,
    },
    /// A replay's profile, prepared to play.
    Prepared {
        path: PathBuf,
        result: Box<Result<Prepared, Error>>,
    },
}
