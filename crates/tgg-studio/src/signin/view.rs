//! The sidebar's account row, the sign-in dialog (the code, and the page
//! to confirm it on), and the account dialog (who's signed in, signing out).

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
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name.clone())
        .focusable(true)
        .on_press(actions::account_row)
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

/// The account dialog: who's signed in, and signing out.
pub fn account(app: &App, p: &Palette) -> Option<El> {
    let who = app
        .signin
        .who
        .as_ref()
        .filter(|_| app.signin.account_open)?;
    let line = if who.verified {
        "Signed in to textures.gg.".to_owned()
    } else {
        "Signed in to textures.gg. The site couldn't be reached to check just now.".to_owned()
    };
    let parts = Dialog {
        title: super::shown_name(&who.account),
        line,
        body: None,
        buttons: vec![
            button("Close", ButtonKind::Secondary, p)
                .key("account/close")
                .on_press(actions::hide_account),
            spacer(),
            button("Sign out", ButtonKind::Secondary, p)
                .key("account/sign-out")
                .on_press(actions::sign_out),
        ],
    };
    Some(dialog(
        "account",
        "dialog",
        DialogWidth::Narrow,
        parts,
        actions::hide_account,
        p,
    ))
}
