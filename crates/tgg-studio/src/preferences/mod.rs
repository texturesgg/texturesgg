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
    /// The apps on this computer that open PNGs, as found when it opened.
    pub apps: Vec<tgg_app::image_apps::ImageApp>,
    /// The image editor is another app or a command: the field shows.
    pub other: bool,
    /// That app's command, as the player types it or browses for it.
    pub editor: TextEdit,
    /// The select whose menu is open.
    pub menu: Option<Menu>,
}

/// Settings' selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    ImageEditor,
    Appearance,
}
