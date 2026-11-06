//! The Library's page.

use crate::state::App;
use crate::ui::page_head;
use crate::ui::tokens::{Palette, space};
use rune::{El, col};

pub fn view(_: &App, p: &Palette) -> El {
    col().px(space::LG + 4.0).pt(space::XS)
        .child(page_head("Library", "Your skins, and which are in your game.", p))
}
