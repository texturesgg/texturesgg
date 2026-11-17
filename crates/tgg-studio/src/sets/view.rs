//! The sheet asking before native code runs: who published it and which
//! mods, plainly. Scripts never ask.

use super::actions;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{ButtonKind, Dialog, DialogWidth, button, dialog, spacer, switch};
use rune::{El, col, row, text as label};

pub fn trust(app: &App, p: &Palette) -> Option<El> {
    let trust = app.sets.trust.as_ref()?;
    let lines = trust.publishers.iter().map(|(_, name, mods)| {
        label(format!(
            "{} runs native code on your computer. It's published by {name}, not textures.gg.",
            mods.join(", ")
        ))
        .font(text::BODY)
        .color(p.ink)
        .wrap()
    });
    let remember = trust.remember;
    let body = col().px(space::LG).gap(space::SM).children(lines).child(
        row()
            .gap(space::SM)
            .items_center()
            .child(
                switch("sets/trust/remember", remember, p)
                    .label("Don't ask again for these publishers")
                    .on_press(move |app: &mut App| actions::set_remember(app, !remember)),
            )
            .child(
                label("Don't ask again for these publishers")
                    .font(text::SMALL)
                    .color(p.muted),
            ),
    );
    Some(dialog(
        "sets/trust",
        "alertdialog",
        DialogWidth::Narrow,
        Dialog {
            title: "Run native code?".into(),
            line: String::new(),
            body: Some(body),
            buttons: vec![
                spacer(),
                button("Cancel", ButtonKind::Secondary, p)
                    .key("sets/trust/no")
                    .on_press(actions::trust_no),
                button("Download and run", ButtonKind::Primary, p)
                    .key("sets/trust/yes")
                    .on_press(actions::trust_yes),
            ],
        },
        actions::trust_no,
        p,
    ))
}
