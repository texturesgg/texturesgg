//! Where the player is while a slot is open: "Skins / Fox", where "Skins"
//! goes back to the page.

use super::{Subject, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use rune::{row, text as label};

/// The title row's breadcrumbs while a slot is open: "Skins / Fox", where
/// "Skins" goes back.
pub fn crumbs(subject: Subject, p: &Palette) -> rune::El {
    row()
        .gap(space::XS)
        .items_center()
        .child(
            label(crate::nav::Place::Skins.label())
                .key("melee/crumb")
                .font(text::STRONG)
                .color(p.muted)
                .role("button")
                .focusable(true)
                .cursor(rune::Cursor::Pointer)
                .on_press(|app: &mut App| {
                    actions::back(app);
                }),
        )
        .child(label("/").font(text::BODY).color(p.faint))
        .child(label(subject.name()).font(text::STRONG).color(p.ink))
}
