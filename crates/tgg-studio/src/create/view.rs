//! Create's page: starting a code mod, or what it needs first.

use crate::nav::Place;
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{ButtonKind, button, page_head, panel};
use rune::{El, ScrollSpec, col, row, text as label};

pub fn view(app: &App, p: &Palette) -> El {
    let start = if app.mods.port().is_some() {
        button("New mod…", ButtonKind::Primary, p)
            .key("create/new-mod")
            .on_press(crate::newmod::actions::open)
    } else {
        button("Set up tgg-melee", ButtonKind::Secondary, p)
            .key("create/set-up")
            .on_press_tx(|app: &mut App, tx| crate::editor::actions::go(app, Place::Mods, tx))
    };
    let about = if app.mods.port().is_some() {
        "Start from tgg-melee's SDK: a folder with git, an example to build on, and the commands to build and try it."
    } else {
        "Mods build against tgg-melee, so it needs installing first."
    };
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG)
        .child(page_head(
            Place::Create.label(),
            "Make mods of your own for tgg-melee.",
            p,
        ))
        .child(
            panel(p)
                .key("create/code-mod")
                .w(size::GAME_CARD)
                .gap(space::MD)
                .child(
                    col()
                        .gap(space::SNUG)
                        .child(label("A code mod").font(text::H3).color(p.ink))
                        .child(label(about).font(text::BODY).color(p.muted).wrap()),
                )
                .child(row().child(start)),
        );
    col()
        .key("create/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}
