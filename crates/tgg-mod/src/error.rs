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
    #[error("can't reach textures.gg ({0}); what's installed still works")]
    Offline(NetError),
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
