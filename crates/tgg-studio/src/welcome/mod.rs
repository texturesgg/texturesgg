//! Welcome: finding the player's game. It shows until they have one: the
//! Melee NTSC 1.02 ISOs found in their game folders to pick from, and a way
//! to choose one anywhere. An ISO that can't be used says why, in their
//! words. `actions` finds and opens games; `view` draws the screen.

pub mod actions;
pub mod view;

use tgg_app::GameChoice;

#[derive(Default)]
pub struct Welcome {
    /// Melee ISOs in the player's game folders.
    pub games: Vec<GameChoice>,
    /// Why the last ISO given or chosen can't be used.
    pub problem: Option<String>,
}
