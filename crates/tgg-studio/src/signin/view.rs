//! The sidebar's account row and the sign-in dialog (the code, and the page
//! to confirm it on). Who's signed in, and signing out, are on You.

use super::actions;
use crate::keys;
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, PLACE_INSET, TEXT_ACTION_INSET, avatar, button, dialog,
    icon_button, settings_glyph, spacer, text_action,
};
use rune::{El, col, row, text as label};

/// The sidebar's foot: who's signed in (else Sign in), pressing it for
/// their account or to sign in; and Settings beside it.
pub fn foot(app: &App, p: &Palette) -> El {
    let who = app.signin.who.as_ref();
    let name = who.map_or_else(
        || "Sign in".to_owned(),
        |signed| super::shown_name(&signed.account),
    );
    let mark = who.map(|signed| avatar(&super::shown_name(&signed.account), p));
    let account = row()
        .key("sidebar/account")
        .grow(1.0)
        .min_w(0.0)
        .h(size::ROW)
        .px(PLACE_INSET)
        .gap(space::XS)
        .items_center()
        .radius(radius::CONTROL)
        // Marked while You shows, as the places above are.
        .fill(if app.nav.place == crate::nav::Place::Profile {
            p.hover
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name.clone())
        .focusable(true)
        .on_press_tx(actions::account_row)
        .children(mark)
        .child(
            label(name)
                .font(text::STRONG)
                .color(if who.is_some() { p.ink } else { p.muted })
                .truncate(),
        );
    let settings = icon_button(settings_glyph(p), "Settings", p)
        .key("sidebar/settings")
        .help(keys::hint("Settings", keys::Command::Settings))
        .on_press(crate::preferences::actions::open);
    row()
        .mx(space::XS)
        .my(space::SNUG)
        .gap(space::TIGHT)
        .items_center()
        .child(account)
        .child(settings)
}

/// The sign-in dialog, while signing in: the code to confirm on the site,
/// opening the page, and how it's going.
pub fn sign_in(app: &App, p: &Palette) -> Option<El> {
    let login = app.signin.login.as_ref()?;
    let code = login.code.clone().map_or_else(
        || {
            label("Getting a code…")
                .key("signin/code")
                .font(text::BODY)
                .color(p.muted)
        },
        |code| label(code).key("signin/code").font(text::H2).color(p.ink),
    );
    let (status, color) = match &login.problem {
        Some(problem) => (problem.clone(), p.danger),
        None if login.code.is_some() => (
            "Waiting for you to confirm it on textures.gg…".to_owned(),
            p.muted,
        ),
        None => (String::new(), p.muted),
    };
    let body =
        col()
            .px(space::LG)
            .py(space::MD)
            .gap(space::SM)
            .child(row().gap(space::SM).items_center().child(code).children(
                login.code.is_some().then(|| {
                    row().ml(-TEXT_ACTION_INSET).child(
                        text_action("Copy", false, p)
                            .key("signin/copy")
                            .on_press_tx(actions::copy_code),
                    )
                }),
            ))
            // The page's link, to open by hand when the browser doesn't.
            .children(login.link.clone().map(|link| {
                col()
                    .gap(space::TIGHT)
                    .child(
                        label("If your browser doesn't open, open this link in it:")
                            .font(text::SMALL)
                            .color(p.muted),
                    )
                    .child(
                        row()
                            .gap(space::XS)
                            .items_center()
                            .child(
                                label(link)
                                    .key("signin/link")
                                    .font(text::MONO_SMALL)
                                    .color(p.ink)
                                    .truncate_middle()
                                    .shrink()
                                    .min_w(0.0),
                            )
                            .child(
                                text_action("Copy link", false, p)
                                    .key("signin/copy-link")
                                    .on_press_tx(actions::copy_link),
                            ),
                    )
            }))
            .child(
                label(status)
                    .key("signin/status")
                    .font(text::SMALL)
                    .color(color)
                    .wrap(),
            );
    let action = if login.problem.is_some() {
        button("Start again", ButtonKind::Primary, p)
            .key("signin/again")
            .on_press(actions::begin)
    } else {
        button("Open textures.gg", ButtonKind::Primary, p)
            .key("signin/open")
            .on_press_tx(actions::open_page)
    };
    let parts = Dialog {
        title: "Sign in to textures.gg".into(),
        line: "Confirm this code on textures.gg to connect the app to your account.".into(),
        body: Some(body),
        buttons: vec![
            button("Cancel", ButtonKind::Secondary, p)
                .key("signin/cancel")
                .on_press(actions::cancel),
            spacer(),
            action,
        ],
    };
    Some(dialog(
        "signin",
        "dialog",
        DialogWidth::Narrow,
        parts,
        actions::cancel,
        p,
    ))
}
