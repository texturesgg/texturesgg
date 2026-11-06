//! Moving between places, and the sidebar.

use super::Place;
use crate::state::App;

/// Show `place`.
pub fn go(app: &mut App, place: Place) {
    if place != app.nav.place {
        crate::melee::actions::back(app);
        app.notice = None;
    }
    app.nav.place = place;
    if place == Place::Library {
        crate::library::actions::request_renders(app);
    }
}

/// Show or hide the sidebar, and remember which.
pub fn toggle_sidebar(app: &mut App) {
    let shown = !app.nav.sidebar;
    app.nav.sidebar = shown;
    app.change_settings(|settings| settings.sidebar = Some(shown));
}

/// Up a level: from a fighter or stage back to the roster. Whether there
/// was a level to go up.
pub fn up(app: &mut App) -> bool {
    match app.nav.place {
        Place::Melee => crate::melee::actions::back(app),
        Place::Library | Place::Mods => false,
    }
}
