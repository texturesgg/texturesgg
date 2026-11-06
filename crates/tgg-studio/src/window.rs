//! The window itself. Linux and Windows windows are borderless, so the app
//! draws their controls; Rune can close a borderless window but not
//! minimize it, and double-clicking the title row maximizes it.

use crate::editor::{Pending, actions as editor};
use crate::state::App;
use rune::Tx;

/// Close the window, as its close button: after asking, when a costume has
/// edits unsaved.
pub fn close(app: &mut App, tx: &mut Tx) {
    if app.editor.is_some() {
        editor::leave_for(app, Pending::Close, tx);
    } else {
        tx.close_window();
    }
}
