//! Fighters: binding and playing their animations, the model parts the game
//! shows, what their moves are called, and where a costume draws its textures.

pub mod animation;
mod kind;
pub mod moves;
pub mod parts;
pub mod places;
pub mod playback;

pub use kind::{CostumeIndex, FighterKind};
