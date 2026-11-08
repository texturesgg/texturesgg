//! Settings: the player's game (the ISO skins go into), the image editor
//! textures open in, the palette, the update check, and where the app keeps
//! their files, in a dialog over the window. Each change is kept as it's
//! made, in `editor.json`; the image editor's command when the dialog
//! closes. `actions` changes it; `view` draws the dialog.

pub mod actions;
pub mod view;

use rune::TextEdit;

#[derive(Default)]
pub struct Preferences {
    /// The dialog shows.
    pub open: bool,
    /// The image editor's command, as the player types it.
    pub editor: TextEdit,
}
