//! The window itself. Linux and Windows windows are borderless, so the app
//! draws their controls; Rune can close a borderless window but not
//! minimize it, and double-clicking the title row maximizes it.

use crate::state::App;
use rune::Tx;

/// Close the window, as its close button.
pub fn close(_: &mut App, tx: &mut Tx) {
    tx.close_window();
}
