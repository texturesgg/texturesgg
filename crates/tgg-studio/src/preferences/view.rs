//! The Settings dialog: a row per setting, its name and what it does on
//! the left, its control on the right, and help at the end.

use super::actions;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, chip, dialog, rule, spacer, switch,
    text_action, text_field,
};
use rune::{El, col, row, text as label};
use tgg_app::settings::Appearance;

/// The image editor's choices and field, at their widest.
const CHOICES: f32 = 300.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    let rows = [
        image_editor(app, p),
        appearance(app, p),
        updates(app, p),
        game(app, p),
        help(p),
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

/// The image editor: the system's default, an app on this computer that
/// opens PNGs, or another (a command typed, or a program browsed for).
fn image_editor(app: &App, p: &Palette) -> El {
    let preferences = &app.preferences;
    let kept = app.settings.external_editor.as_deref();
    let default = chip("System default", kept.is_none() && !preferences.other, p)
        .key("settings/image-editor/default")
        .role("radio")
        .on_press(|app: &mut App| actions::choose_editor(app, None));
    let apps = preferences.apps.iter().enumerate().map(|(index, known)| {
        let command = known.command.clone();
        chip(
            &known.name,
            !preferences.other && kept == Some(command.as_str()),
            p,
        )
        .key(format!("settings/image-editor/app/{index}"))
        .role("radio")
        .on_press(move |app: &mut App| actions::choose_editor(app, Some(command.clone())))
    });
    let other = chip("Other…", preferences.other, p)
        .key("settings/image-editor/other")
        .role("radio")
        .on_press(actions::choose_other);
    let field = preferences.other.then(|| {
        col()
            .gap(space::XXS)
            .child(
                text_field(&preferences.editor, "A program, such as aseprite", p)
                    .key("settings/image-editor/field")
                    .on_edit(|app: &mut App| &mut app.preferences.editor),
            )
            .child(
                row().ml(-TEXT_ACTION_INSET).child(
                    text_action("Browse…", false, p)
                        .key("settings/image-editor/browse")
                        .on_press_tx(actions::browse_editor),
                ),
            )
    });
    let choices = col()
        .w(CHOICES)
        .gap(space::XS)
        .child(
            row()
                .flex_wrap()
                .gap(space::SNUG)
                .child(default)
                .children(apps)
                .child(other),
        )
        .children(field);
    setting(
        "settings/image-editor",
        "Image editor",
        "Where Edit in your image editor opens a texture.",
        choices,
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

/// The game skins install into, and choosing another.
fn game(app: &App, p: &Palette) -> El {
    let path = app.game.as_ref().map(|references| references.game().path());
    let file = path
        .and_then(|path| path.file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    // The file's name says which; its whole path is a hover away.
    setting(
        "settings/game",
        "Game",
        format!("Skins install into {file}."),
        text_action("Choose another…", false, p)
            .key("settings/choose-game")
            .on_press_tx(actions::choose_game),
        p,
    )
    .help(
        path.map(|path| path.display().to_string())
            .unwrap_or_default(),
    )
}

/// Help: reporting a problem, the community, and what's running.
fn help(p: &Palette) -> El {
    let actions = row()
        .gap(space::XS)
        .items_center()
        .child(
            button("Report a problem", ButtonKind::Secondary, p)
                .key("settings/report")
                .on_press(actions::report_problem),
        )
        .child(
            text_action("Discord", false, p)
                .key("settings/discord")
                .on_press_tx(crate::report::actions::discord),
        );
    setting("settings/help", "Help", tgg_app::log::version(), actions, p)
}
