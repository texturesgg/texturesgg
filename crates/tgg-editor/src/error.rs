//! What can go wrong, in the player's words. Most messages finish a sentence
//! a notice starts ("Couldn't save: …"), so they read as clauses.

use crate::game::GameError;
use melee_dat::MeleeSlot;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Game(#[from] GameError),
    #[error(transparent)]
    Source(#[from] dat_parser::hsd::source::HsdSourceError),
    #[error(transparent)]
    Playback(#[from] melee_dat::MeleeError),
    #[error(transparent)]
    Render(#[from] hsd_render::HsdRenderError),
    #[error(transparent)]
    Document(#[from] dat_edit::DocumentError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Disc(#[from] gc_iso::Error),
    /// Why the library turned a file away.
    #[error("{0}")]
    Rejected(String),

    #[error("the textures couldn't be read")]
    NoDocument,
    #[error("the pixels don't match the size")]
    PixelSize,
    #[error("the bind pose has no animations")]
    NoAnimations,

    #[error("{program} has this ISO open. Close it, then try again.")]
    DiscHeld { program: String },
    #[error("{name} couldn't be written ({source})")]
    DiscWrite { name: String, source: gc_iso::Error },
    #[error("{name}: {source}")]
    DiscRead { name: String, source: gc_iso::Error },
    #[error("{name} didn't read back as written")]
    DiscReadBack { name: String },

    #[error("the library couldn't save it ({0})")]
    LibrarySave(#[source] io::Error),
    #[error("the library couldn't keep a copy ({0})")]
    LibraryKeep(#[source] io::Error),
    #[error("the library's copy is missing ({0})")]
    LibraryMissing(#[source] io::Error),
    #[error("the library's copy has changed on disk")]
    LibraryChanged,
    #[error("That skin isn't in your library.")]
    NotInLibrary,

    #[error("the install history couldn't be saved ({0})")]
    HistorySave(#[source] io::Error),
    #[error("{0} has no install to undo")]
    NothingToUndo(MeleeSlot),
    #[error(
        "No vanilla copy of {0} is at hand. It comes back once the app has seen it: \
         in this ISO before an install, or in another vanilla ISO in your game folders."
    )]
    NoVanilla(MeleeSlot),

    #[error("a {0} quote isn't closed")]
    UnclosedQuote(char),
    #[error("it ends in a lone \\")]
    LoneBackslash,
    #[error("it's empty")]
    EmptyCommand,
}
