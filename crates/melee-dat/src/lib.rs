//! Super Smash Bros. Melee on top of [`dat_parser`], which knows HAL's HSD
//! format but nothing about a particular game.
//!
//! [`MeleeModel`] is the way in: load any of the game's DATs and it is a
//! fighter, a stage, or a model that just draws in the pose it was saved in.
//! Evaluate it for a frame and hand the result to a renderer; nothing here
//! touches a GPU.
//!
//! - [`fighter`]: a fighter's animations and their playback, the model parts
//!   the game shows, the names of its moves, where a costume draws each
//!   texture, and the models every costume shares ([`SharedModel`]).
//! - [`stage`]: a stage's general points (camera range, blast zone), its
//!   load-time animations, and HAL's names for its textures.
//! - [`catalog`] and [`references`]: the checked-in table of every fighter's
//!   files, and the original game files a caller supplies to play them.
//! - [`file_names`] and [`vanilla`]: how the game names its files
//!   ([`MeleeSlot`] is one a skin replaces), and the size and hash of each as
//!   shipped, with the fingerprint of each shared model.

pub mod catalog;
pub mod error;
pub mod fighter;
pub mod file_names;
#[cfg(all(test, feature = "melee-iso"))]
mod iso_tests;
pub mod model;
pub mod references;
pub mod stage;
pub mod vanilla;

pub use catalog::MeleeReferenceCatalog;
pub use error::{MeleeError, Result};
pub use fighter::playback::{FighterAttach, MeleeFighterPlayback};
pub use fighter::shared::{Firing, SharedModel, Shot, SpawnedModel};
pub use fighter::{CostumeIndex, FighterKind};
pub use file_names::{
    Character, CostumeColor, Effects, MeleeSlot, NotASlot, Stage, parse_filename,
};
pub use model::{FighterAttachOutcome, MeleeModel, MeleeModelKind};
pub use references::MeleeReferenceStore;
pub use stage::playback::MeleeStagePlayback;
