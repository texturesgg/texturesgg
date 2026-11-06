//! Moving between places, and the sidebar.

use super::Place;
use crate::state::App;
use rune::{Key, KeyEvent, Tx};

/// Show `place`.
pub fn go(app: &mut App, place: Place) {
    if place != app.nav.place {
        crate::melee::actions::back(app);
    }
    app.nav.place = place;
}

/// Show or hide the sidebar, and remember which.
pub fn toggle_sidebar(app: &mut App) {
    let shown = !app.nav.sidebar;
    app.nav.sidebar = shown;
    app.change_settings(|settings| settings.sidebar = Some(shown));
}

/// The window's keys, when nothing focused takes them: Escape goes up a
/// level, and Ctrl-\ (Cmd-\ on macOS) shows or hides the sidebar once there's
/// a game.
pub fn on_key(app: &mut App, key: &KeyEvent, _: &mut Tx) -> bool {
    if key.key == Key::Escape {
        return up(app);
    }
    if app.game.is_some() && key.is_command('\\') {
        toggle_sidebar(app);
        return true;
    }
    false
}

/// Up a level: from a fighter or stage back to the roster. Whether there
/// was a level to go up.
pub fn up(app: &mut App) -> bool {
    match app.nav.place {
        Place::Melee => crate::melee::actions::back(app),
        Place::Library | Place::Mods => false,
    }
}
