//! Finding Slippi's Dolphin, for where Settings says the Slippi ISO goes.

use crate::state::App;

/// Look for Slippi's netplay Dolphin.
pub fn find(app: &mut App) {
    app.slippi.dolphin = tgg_app::slippi::dolphin();
}
