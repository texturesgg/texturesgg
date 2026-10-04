//! Fighters: binding and playing their animations, the model parts the game
//! shows, what their moves are called, where a costume draws its textures,
//! and the models every costume shares (a laser, a shine).

pub mod animation;
mod kind;
pub mod moves;
pub mod parts;
pub mod places;
pub mod playback;
pub mod script;
pub mod shared;

pub use kind::{CostumeIndex, FighterKind};
