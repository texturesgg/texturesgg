//! Opening You, and going from it to what was made.

use crate::nav::{Place, actions as nav};
use crate::state::App;
use tgg_mod::ModId;

/// You shows: read the mods folder afresh, and ask for the skins' pictures.
pub fn opened(app: &mut App) {
    crate::mods::actions::read_disk(app);
    crate::library::actions::request_renders(app);
}

/// Show the mod `id`, one the player is making, among them on Mods.
pub fn open_mod(app: &mut App, id: &ModId) {
    nav::go(app, Place::Mods);
    crate::mods::actions::show_tab(app, crate::mods::Tab::Installed);
    crate::mods::actions::show_made(app, true);
    if app.mods.selected.as_ref() != Some(id) {
        crate::mods::actions::select(app, id);
    }
}

/// Show the skins the player made, among their skins on Skins.
pub fn open_skins(app: &mut App) {
    crate::melee::actions::show_mine(app);
    crate::library::actions::show_filter(app, crate::library::Filter::Made);
}
