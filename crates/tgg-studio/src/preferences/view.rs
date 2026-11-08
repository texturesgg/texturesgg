//! The Settings dialog: a row per setting, its name and what it does on
//! the left, its control on the right.

use super::actions;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, button, chip, dialog, rule, spacer, switch, text_field,
};
use rune::{El, col, row, text as label};
use tgg_app::settings::Appearance;

/// The image editor field's width.
const FIELD: f32 = 220.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    let rows = [
        game(app, p),
        image_editor(app, p),
        appearance(app, p),
        updates(app, p),
        files(app, p),
    ];
    let body = col().children(rows.into_iter().enumerate().flat_map(|(index, row)| {
        let line = (index > 0).then(|| rule(p));
        line.into_iter().chain([row])
    }));
    let parts = Dialog {
        title: "Settings".into(),
        line: "Kept as you change them.".into(),
        body: Some(body),
        buttons: vec![
            spacer(),
            button("Done", ButtonKind::Secondary, p)
                .key("settings/done")
                .on_press(actions::close),
        ],
    };
    Some(dialog(
        "settings",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::close,
        p,
    ))
}

/// A setting: its name and a line saying what it does, then its control.
fn setting(key: &str, name: &str, line: impl Into<String>, control: El, p: &Palette) -> El {
    row()
        .key(key.to_owned())
        .px(space::LG)
        .py(space::SM)
        .gap(space::MD)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(label(name.to_owned()).font(text::STRONG).color(p.ink))
                .child(label(line.into()).font(text::SMALL).color(p.muted).wrap()),
        )
        .child(control)
}

fn game(app: &App, p: &Palette) -> El {
    let path = app
        .game
        .as_ref()
        .map(|references| references.game().path().display().to_string())
        .unwrap_or_default();
    setting(
        "settings/game",
        "Your game",
        format!("Skins install into {path}."),
        button("Choose another…", ButtonKind::Secondary, p)
            .key("settings/choose-game")
            .on_press_tx(actions::choose_game),
        p,
    )
}

fn image_editor(app: &App, p: &Palette) -> El {
    let field = text_field(&app.preferences.editor, "Your system's default", p)
        .key("settings/image-editor/field")
        .w(FIELD)
        .on_edit(|app: &mut App| &mut app.preferences.editor);
    setting(
        "settings/image-editor",
        "Image editor",
        "The program Edit in your image editor opens textures in, such as aseprite or gimp.",
        field,
        p,
    )
}

fn appearance(app: &App, p: &Palette) -> El {
    let now = app.settings.appearance();
    let choice = |name: &str, appearance: Appearance, key: &str| {
        chip(name, now == appearance, p)
            .key(key.to_owned())
            .role("radio")
            .on_press(move |app: &mut App| actions::set_appearance(app, appearance))
    };
    setting(
        "settings/appearance",
        "Appearance",
        "Gallery is dark; Paper is light.",
        row()
            .gap(space::SNUG)
            .child(choice(
                "Gallery",
                Appearance::Gallery,
                "settings/appearance/gallery",
            ))
            .child(choice(
                "Paper",
                Appearance::Paper,
                "settings/appearance/paper",
            )),
        p,
    )
}

fn updates(app: &App, p: &Palette) -> El {
    let on = app.settings.check_for_updates.unwrap_or(true);
    setting(
        "settings/updates",
        "Check for updates",
        "Once at launch, the app asks textures.gg whether a newer version is out. Nothing downloads on its own.",
        switch("settings/updates/switch", on, p)
            .label("Check for updates")
            .on_press(move |app: &mut App| actions::set_update_check(app, !on)),
        p,
    )
}

/// Where the app keeps the player's files: shown, not changed here.
fn files(app: &App, p: &Palette) -> El {
    let place = |name: &str, path: &std::path::Path| {
        col()
            .gap(space::TIGHT)
            .child(label(name.to_owned()).font(text::SMALL).color(p.muted))
            .child(
                label(path.display().to_string())
                    .font(text::MONO_SMALL)
                    .color(p.faint)
                    .wrap(),
            )
    };
    col()
        .key("settings/files")
        .px(space::LG)
        .py(space::SM)
        .gap(space::XS)
        .child(label("Your files").font(text::STRONG).color(p.ink))
        .child(place("Skins", &app.data.library))
        .child(place("Mods", &app.data.mods))
        .child(place("tgg-melee", &app.data.ports))
}
