//! Slippi: the Dolphin Slippi Launcher installs. Skins go into the player's
//! ISO, and Settings says where Slippi's ISO is; the app no longer starts
//! Slippi's Dolphin itself. `actions` finds it, for what Settings says about
//! it.

pub mod actions;

use std::path::PathBuf;

#[derive(Default)]
pub struct Slippi {
    /// Slippi's netplay Dolphin, once found; `None` when Slippi Launcher
    /// hasn't installed it (or before it's looked for).
    pub dolphin: Option<PathBuf>,
}
