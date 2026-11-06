//! Moving between places, and the sidebar.

use super::Place;
use crate::state::App;
use rune::{KeyEvent, Tx};

/// Show `place`.
pub fn go(app: &mut App, place: Place) {
    app.nav.place = place;
}

/// Show or hide the sidebar, and remember which.
pub fn toggle_sidebar(app: &mut App) {
    let shown = !app.nav.sidebar;
    app.nav.sidebar = shown;
    app.change_settings(|settings| settings.sidebar = Some(shown));
}

/// The window's keys, when nothing focused takes them: Ctrl-\ (Cmd-\ on
/// macOS) shows or hides the sidebar once there's a game.
pub fn on_key(app: &mut App, key: &KeyEvent, _: &mut Tx) -> bool {
    if app.game.is_some() && key.is_command('\\') {
        toggle_sidebar(app);
        return true;
    }
    false
}
