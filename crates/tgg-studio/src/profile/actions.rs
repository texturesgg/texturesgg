//! What You does: read what's on this computer, open a skin the player made
//! on Skins, a mod they're making on Mods, and its folder.

use crate::nav::{Place, actions as nav};
use crate::state::App;
use rune::Tx;
use tgg_mod::ModId;

/// You shows: read the mods folder afresh and ask for the skins' pictures.
pub fn opened(app: &mut App) {
    crate::mods::actions::read_disk(app);
    crate::library::actions::request_renders(app);
}

/// Open the skin `id` the player made, in its sheet on Skins.
pub fn open_skin(app: &mut App, id: tgg_app::ids::SkinId) {
    nav::go(app, Place::Skins);
    crate::melee::actions::show_tab(app, crate::melee::Tab::Made);
    crate::melee::actions::open_skin(app, id);
}

/// Show the mod `id` among the player's own on Mods.
pub fn open_mod(app: &mut App, id: &ModId) {
    nav::go(app, Place::Mods);
    crate::mods::actions::show_mod(app, id);
}

/// Open the folder a mod they're making is in.
pub fn open_folder(app: &mut App, id: &ModId, tx: &mut Tx) {
    let folder = app.data.mods.join(id.to_string());
    if let Err(error) = tx.open_url(&folder.display().to_string()) {
        app.tell(format!("Couldn't open its folder: {error}"), true);
    }
}
