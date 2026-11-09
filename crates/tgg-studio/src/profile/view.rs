//! You's page: who's signed in and signing out, then the skins they made and
//! the mods they're making, each with a way to more of them, or to make one.

use super::actions;
use crate::nav::{Place, actions as nav};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, button, icon_tile, page_head, rule, spacer, status, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::LocalMod;

pub fn view(app: &App, p: &Palette) -> El {
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG)
        .children(head(app, p))
        .child(skins(app, p))
        .child(mods(app, p));
    col()
        .key("profile/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// Their name and handle, whether the site vouched for them just now, and
/// signing out.
fn head(app: &App, p: &Palette) -> Option<El> {
    let who = app.signin.who.as_ref()?;
    let name = crate::signin::shown_name(&who.account);
    let line = if who.verified {
        format!("@{} on textures.gg", who.account.handle)
    } else {
        format!(
            "@{} on textures.gg. The site couldn't be reached to check just now.",
            who.account.handle
        )
    };
    Some(
        row()
            .gap(space::MD)
            .items_start()
            .child(
                col()
                    .key("profile/name")
                    .grow(1.0)
                    .min_w(0.0)
                    .child(page_head(&name, &line, p)),
            )
            .child(
                button("Sign out", ButtonKind::Secondary, p)
                    .key("profile/sign-out")
                    .on_press(crate::signin::actions::sign_out),
            ),
    )
}

/// A section: its heading and count, a way to see all of them, and what's in it.
fn section(title: &str, count: usize, more: Option<El>, body: El, p: &Palette) -> El {
    col()
        .gap(space::SM)
        .child(
            row()
                .gap(space::XS)
                .items_end()
                .child(label(title.to_owned()).font(text::H3).color(p.ink))
                .child(
                    label(count.to_string())
                        .font(text::MONO_SMALL)
                        .color(p.faint),
                )
                .child(spacer())
                .children(more.map(|more| more.mr(-TEXT_ACTION_INSET))),
        )
        .child(body)
}

/// Words for an empty section, and the place to go to make one.
fn empty(key: &str, words: &str, action: &str, place: Place, p: &Palette) -> El {
    col()
        .key(key.to_owned())
        .gap(space::XS)
        .child(
            label(words.to_owned())
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        )
        .child(
            row().ml(-TEXT_ACTION_INSET).child(
                text_action(action, false, p)
                    .key(format!("{key}/go"))
                    .on_press(move |app: &mut App| nav::go(app, place)),
            ),
        )
}

/// The skins they saved from the editor, newest first.
fn skins(app: &App, p: &Palette) -> El {
    let made: Vec<_> = app
        .library
        .skins()
        .iter()
        .rev()
        .filter(|skin| crate::library::view::made(skin))
        .collect();
    let more = (!made.is_empty()).then(|| {
        text_action("Show on Skins", false, p)
            .key("profile/skins/all")
            .on_press(actions::open_skins)
    });
    let body = if made.is_empty() {
        empty(
            "profile/skins/empty",
            "Skins you edit and save show here.",
            "Pick a fighter on Skins",
            Place::Skins,
            p,
        )
    } else {
        row().flex_wrap().gap(space::MD).children(
            made.iter()
                .map(|skin| crate::library::view::card(app, skin, p)),
        )
    };
    section("Skins you made", made.len(), more, body, p).key("profile/skins")
}

/// The mods they're making: the ones that load from their own folders.
fn mods(app: &App, p: &Palette) -> El {
    let making: Vec<&LocalMod> = app.mods.local.mods.iter().filter(|m| m.dev).collect();
    let body = if making.is_empty() {
        empty(
            "profile/mods/empty",
            "Mods you start with New mod… show here while you make them.",
            "Start one on Mods",
            Place::Mods,
            p,
        )
    } else {
        col()
            .child(rule(p))
            .children(making.iter().flat_map(|m| [mod_row(app, m, p), rule(p)]))
    };
    section("Mods you're making", making.len(), None, body, p).key("profile/mods")
}

/// A mod they're making: its icon, name and version; pressing it shows it on Mods.
fn mod_row(app: &App, m: &LocalMod, p: &Palette) -> El {
    let id = m.id().clone();
    let key = format!("profile/mod/{id}");
    row()
        .key(key)
        .py(space::XS)
        .px(space::XS)
        .mx(-space::XS)
        .gap(space::SM)
        .items_center()
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(m.manifest.name.clone())
        .focusable(true)
        .on_press(move |app: &mut App| actions::open_mod(app, &id))
        .child(icon_tile(app.mods.icon(m.id()), size::THUMBNAIL, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    row()
                        .gap(space::XS)
                        .items_end()
                        .child(
                            label(m.manifest.name.clone())
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate(),
                        )
                        .child(
                            label(m.manifest.version.to_string())
                                .font(text::MONO_SMALL)
                                .color(p.faint),
                        ),
                )
                .child(status(
                    if m.enabled {
                        "A dev build · on"
                    } else {
                        "A dev build · off"
                    },
                    m.enabled,
                    p,
                )),
        )
}
