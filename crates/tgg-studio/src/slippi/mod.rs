//! Slippi: the other game the app serves. Skins go into the player's ISO,
//! and Play opens it in Slippi's netplay Dolphin, as Slippi Launcher
//! installed it. `actions` finds and starts it; the sidebar draws Play.

pub mod actions;

use std::path::PathBuf;

#[derive(Default)]
pub struct Slippi {
    /// Slippi's netplay Dolphin, once found; `None` when Slippi Launcher
    /// hasn't installed it (or before it's looked for).
    pub dolphin: Option<PathBuf>,
    /// Dolphin, started from here, still running.
    pub running: bool,
}
