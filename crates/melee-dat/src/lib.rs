//! Super Smash Bros. Melee on top of [`dat_parser`], which knows HAL's HSD
//! format but nothing about a particular game.
//!
//! [`MeleeModel`] is the way in: load any of the game's DATs and it is a
//! fighter, a stage, or a model that just draws in the pose it was saved in.
//! Evaluate it for a frame and hand the result to a renderer; nothing here
//! touches a GPU.
//!
//! - [`fighter`]: a fighter's animations and their playback, the model parts
//!   the game shows, the names of its moves, and where a costume draws each
//!   texture.
//! - [`stage`]: a stage's general points (camera range, blast zone), its
//!   load-time animations, and HAL's names for its textures.
//! - [`catalog`] and [`references`]: the checked-in table of every fighter's
//!   files, and the original game files a caller supplies to play them.
//! - [`file_names`] and [`vanilla`]: how the game names its files, and the
//!   size and hash of each as shipped.

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
pub use fighter::playback::MeleeFighterPlayback;
pub use file_names::*;
pub use model::MeleeModel;
pub use references::MeleeReferenceStore;
pub use stage::playback::MeleeStagePlayback;
