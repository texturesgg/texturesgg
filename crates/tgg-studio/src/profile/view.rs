//! You's page: their picture and name, signing in or out, then the skins
//! they made and the mods they're making on this computer.

use super::actions;
use crate::nav::{Place, actions as nav};
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, button, button_small, icon_tile, list_row, page_head, person,
    spacer, text_action,
};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::LocalMod;

pub fn view(app: &App, p: &Palette) -> El {
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::XL)
        .child(head(app, p))
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

/// Who's signed in: their mark, name, and signing out. Signed out, what
/// signing in is for.
fn head(app: &App, p: &Palette) -> El {
    let Some(who) = &app.signin.who else {
        return row()
            .key("profile/signed-out")
            .gap(space::LG)
            .items_center()
            .child(
                el().size(size::PORTRAIT, size::PORTRAIT)
                    .radius(size::PORTRAIT / 2.0)
                    .fill(p.surface)
                    .center()
                    .child(person(p.faint)),
            )
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(space::XS)
                    .child(page_head("You're not signed in", "", p))
                    .child(
                        label(
                            "Sign in to textures.gg to find matches, and keep your skins with you.",
                        )
                        .font(text::BODY)
                        .color(p.muted)
                        .wrap(),
                    ),
            )
            .child(
                button("Sign in", ButtonKind::Primary, p)
                    .key("profile/sign-in")
                    .on_press(crate::signin::actions::begin),
            );
    };
    let name = crate::signin::shown_name(&who.account);
    let initial: String = name
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default();
    let handle = who.account.handle.clone();
    row()
        .key("profile/head")
        .gap(space::LG)
        .items_center()
        .child(
            el().size(size::PORTRAIT, size::PORTRAIT)
                .radius(size::PORTRAIT / 2.0)
                .fill(p.selected)
                .center()
                .child(label(initial).font(text::H2).color(p.accent_text)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(page_head(&name, &format!("@{handle} on textures.gg"), p)),
        )
        .child(
            button("Sign out", ButtonKind::Secondary, p)
                .key("profile/sign-out")
                .on_press(crate::signin::actions::sign_out),
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
            .on_press(crate::melee::actions::show_mine)
    });
    let body = if made.is_empty() {
        col()
            .key("profile/skins/empty")
            .gap(space::SM)
            .child(
                label("Skins you make and save show here.")
                    .font(text::BODY)
                    .color(p.muted),
            )
            .child(
                row().child(
                    button_small("Make a skin", ButtonKind::Secondary, p)
                        .key("profile/skins/make")
                        .on_press(crate::melee::actions::make),
                ),
            )
    } else {
        row()
            .flex_wrap()
            .gap(space::MD)
            .children(made.iter().map(|skin| {
                let id = skin.id;
                crate::library::view::card(app, skin, p)
                    .key(format!("profile/skin/{id}"))
                    .cursor(rune::Cursor::Pointer)
                    .role("button")
                    .label(crate::library::view::name(skin))
                    .focusable(true)
                    .on_press(move |app: &mut App| actions::open_skin(app, id))
            }))
    };
    section("Skins you made", made.len(), more, body, p).key("profile/skins")
}

/// The mods they're making with `tgg mod dev`.
fn mods(app: &App, p: &Palette) -> El {
    let making: Vec<&LocalMod> = app.mods.local.mods.iter().filter(|m| m.dev).collect();
    let body = if making.is_empty() {
        col()
            .key("profile/mods/empty")
            .gap(space::SM)
            .child(
                label("Mods you start with Start a mod, and run with tgg mod dev, show here.")
                    .font(text::BODY)
                    .color(p.muted)
                    .wrap(),
            )
            .child(
                row().child(
                    button_small("Start one on Mods", ButtonKind::Secondary, p)
                        .key("profile/mods/start")
                        .on_press(|app: &mut App| nav::go(app, Place::Mods)),
                ),
            )
    } else {
        col()
            .key("profile/mod-list")
            .children(making.iter().map(|m| mod_row(app, m, p)))
    };
    section("Mods you're making", making.len(), None, body, p).key("profile/mods")
}

/// A mod they're making: its icon, name and state, and its folder.
fn mod_row(app: &App, m: &LocalMod, p: &Palette) -> El {
    let id = m.id().clone();
    let name = m.manifest.name.clone();
    let pressed = id.clone();
    list_row(p)
        .key(format!("profile/mod/{id}"))
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name.clone())
        .focusable(true)
        .on_press(move |app: &mut App| actions::open_mod(app, &pressed))
        .child(icon_tile(app.mods.icon(m.id()), size::THUMBNAIL, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(label(name).font(text::STRONG).color(p.ink).truncate())
                .child(
                    label(if m.enabled {
                        "tgg mod dev · on"
                    } else {
                        "tgg mod dev · off"
                    })
                    .font(text::SMALL)
                    .color(p.muted),
                ),
        )
        .child(
            button_small("Open folder", ButtonKind::Secondary, p)
                .key(format!("profile/mod/{id}/folder"))
                .on_press_tx(move |app: &mut App, tx| actions::open_folder(app, &id, tx)),
        )
}

/// A section: its heading and count, a way to see all of them, and what's
/// in it.
fn section(title: &str, count: usize, more: Option<El>, body: El, p: &Palette) -> El {
    col()
        .gap(space::SM)
        .child(
            row()
                .gap(space::XS)
                .items_end()
                .child(crate::ui::section(title, p))
                .child(
                    // `section` sits on its own bottom padding; the count
                    // stands on the same line.
                    label(count.to_string())
                        .font(text::MONO_SMALL)
                        .color(p.faint)
                        .pb(space::XS + space::TIGHT),
                )
                .child(spacer())
                .children(more.map(|more| more.mr(-TEXT_ACTION_INSET))),
        )
        .child(body)
}
