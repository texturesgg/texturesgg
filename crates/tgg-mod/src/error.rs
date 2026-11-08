//! What the distribution functions return when they can't do what was asked,
//! and how long work reports progress.
//!
//! Every [`Error`]'s `Display` is a sentence a player can read.

use crate::manifest::ModId;
use crate::net::NetError;
use crate::package::PackageError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// textures.gg (or the release mirror) can't be reached. What's
    /// installed still works.
    /// Why is its source.
    #[error("can't reach textures.gg; what's installed still works")]
    Offline(#[source] NetError),
    /// The registry has no catalog for this game's layout: no mod has been
    /// built for this tgg-melee yet.
    #[error("textures.gg has no mods for tgg-melee {0} yet")]
    NoCatalog(String),
    /// The server answered, but not with what was asked for.
    #[error("{0}")]
    Net(NetError),
    /// Another tgg (the app or the CLI) is changing the mods folder.
    #[error("another tgg is changing your mods; try again when it's done")]
    Busy,
    /// The game is running; the mods folder changes once it's closed.
    #[error("the game is running; close it first")]
    GameRunning,
    #[error("cancelled")]
    Cancelled,
    /// A downloaded package isn't the one the plan named.
    #[error("the download of {id} was damaged or changed (expected SHA-256 {expected}); try again")]
    Checksum { id: ModId, expected: String },
    /// A downloaded port archive isn't the one its release names.
    #[error("the download of {name} was damaged or changed; try again")]
    ReleaseChecksum { name: String },
    /// A downloaded package doesn't open.
    #[error("{id} from textures.gg isn't a valid package: {source}")]
    Package {
        id: ModId,
        #[source]
        source: PackageError,
    },
    /// A downloaded package holds another mod or version than the plan named.
    #[error("the package for {id} {version} holds something else")]
    PackageMismatch { id: ModId, version: semver::Version },
    /// The registry's answer isn't in the shape this tgg reads.
    #[error("textures.gg sent something this version of tgg can't read ({0}); update tgg")]
    Registry(String),
    #[error("{0} isn't installed")]
    NotInstalled(ModId),
    /// The registry lists no such mod: none by that id, or none approved.
    #[error("textures.gg has no mod {0}")]
    NoSuchMod(ModId),
    #[error("the set {0} isn't installed")]
    SetNotInstalled(String),
    /// The lock file names a format newer than this tgg reads.
    #[error("mods.lock.json is format {0}; this tgg reads format {fmt}; update tgg", fmt = crate::lock::FORMAT)]
    LockFormat(u32),
    #[error("mods.lock.json is damaged: {0}")]
    LockDamaged(String),
    /// tgg-melee has no build for this system.
    #[error(
        "tgg-melee is built for Linux on x86-64, Windows on x86-64 and macOS on Apple silicon; this is {0}"
    )]
    UnsupportedSystem(String),
    #[error("{0} has no tgg-melee releases")]
    NoReleases(String),
    #[error("no tgg-melee {version} in {mirror}")]
    NoRelease { version: String, mirror: String },
    #[error("tgg-melee {version} has no build for {system}")]
    NoBuild { version: String, system: String },
    #[error("{0}")]
    BadRelease(String),
    #[error("tgg-melee {version} loads {api} mods; this tgg handles {ours} mods. Update tgg.", ours = crate::API)]
    ReleaseApi { version: String, api: String },
    #[error("tgg-melee {0} isn't installed")]
    PortNotInstalled(String),
    /// A profile file names a format newer than this tgg reads.
    #[error("{path} is a profile of format {format}; this tgg reads format {ours}; update tgg", ours = crate::profile::FORMAT)]
    ProfileFormat { path: PathBuf, format: u32 },
    /// A profile file isn't one.
    #[error("{path} isn't a mod profile: {why}")]
    BadProfile { path: PathBuf, why: String },
    /// A replay file can't be read.
    #[error("{path} isn't a replay tgg can read: {why}")]
    BadReplay { path: PathBuf, why: String },
    /// A profile's mod needs the player's yes before it's fetched.
    #[error("{name} by {publisher} needs your yes before it's fetched")]
    NeedsTrust { name: String, publisher: String },
    /// The game version a profile needs isn't one tgg can get.
    #[error("tgg-melee {0} can't be had for this system")]
    GameUnavailable(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// The error for a failed network call: [`Error::Offline`] when nothing
    /// answered, [`Error::Cancelled`] when the caller cancelled.
    pub fn from_net(error: NetError) -> Self {
        match error {
            NetError::Cancelled => Error::Cancelled,
            error @ NetError::Unreachable { .. } => Error::Offline(error),
            error => Error::Net(error),
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Error::Io { path, source }
    }
}

/// How far long work has got. `step` counts from 0 up to `steps`; `bytes`
/// and `total` are of the current step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub step: usize,
    pub steps: usize,
    /// What the step does, such as "Downloading Legal stages".
    pub label: String,
    pub bytes: u64,
    pub total: Option<u64>,
}

/// The progress callback long work takes.
pub type OnProgress<'a> = &'a (dyn Fn(Progress) + Sync);

/// A progress callback that ignores progress.
pub fn no_progress(_: Progress) {}
