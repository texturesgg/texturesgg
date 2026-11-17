//! Settings: the player's Melee disc and Slippi ISO, tgg-melee's version and
//! updates, the image editor textures open in, the app's own update check,
//! and reporting a problem, in a dialog over the window. Rows sit without
//! rules between them. Each change is kept as it's made, in `editor.json`;
//! the image editor's command when the dialog closes. `actions` changes it;
//! `view` draws the dialog.

use super::{Menu, actions};
use crate::mods::Disc;
use crate::state::App;
use crate::ui::tokens::{Palette, space};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, dialog, select, select_item,
    select_menu, select_rule, spacer, switch, text_action, text_field,
};
use rune::{El, col, row};

/// The image editor's choices and field, at their widest.
const CHOICES: f32 = 300.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    let body = col().children([
        disc(app, p),
        slippi(app, p),
        tgg_melee(app, p),
        image_editor(app, p),
        updates(app, p),
        report(p),
    ]);
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
    crate::ui::setting_row(name, line, control, p).key(key.to_owned())
}

/// tgg-melee's disc: the game it plays, as the game shipped.
fn disc(app: &App, p: &Palette) -> El {
    let line = match &app.mods.disc {
        Disc::Found(_) => "The game tgg-melee plays, as it shipped.",
        Disc::Missing => "tgg-melee needs a Melee NTSC 1.02 disc, and none was found.",
        Disc::Unknown | Disc::Looking => "Looking for your disc…",
    };
    setting(
        "settings/disc",
        "Your Melee disc",
        line,
        text_action("Choose another…", false, p)
            .key("settings/choose-disc")
            .on_press_tx(crate::mods::actions::choose_disc),
        p,
    )
}

/// Slippi's ISO: where "Also in Slippi" writes a skin, and choosing another.
fn slippi(app: &App, p: &Palette) -> El {
    let line = if app.game.is_some() {
        "Also in Slippi writes a skin into this disc, so Slippi plays it too."
    } else {
        "Also in Slippi writes a skin into this disc."
    };
    setting(
        "settings/game",
        "Slippi ISO",
        line,
        text_action("Choose another…", false, p)
            .key("settings/choose-game")
            .on_press_tx(actions::choose_game),
        p,
    )
}

/// tgg-melee's version, its versions, and asking for updates.
fn tgg_melee(app: &App, p: &Palette) -> El {
    let line = match app.mods.port() {
        Some(port) => format!("You have tgg-melee {}.", port.version),
        None => "tgg-melee isn't installed yet.".to_owned(),
    };
    let control = row()
        .mr(-TEXT_ACTION_INSET)
        .gap(space::XXS)
        .items_center()
        .child(
            text_action("Versions…", false, p)
                .key("settings/versions")
                .on_press(|app: &mut App| crate::mods::actions::show_versions(app, true)),
        )
        .child(
            text_action("Check for updates", false, p)
                .key("settings/fetch")
                .on_press(crate::mods::actions::fetch),
        );
    setting("settings/tgg-melee", "tgg-melee", line, control, p)
}

/// The image editor: the system's default, an app on this computer that
/// opens PNGs, or another (a command typed, or a program browsed for), in a
/// select; another's field and Browse under it.
fn image_editor(app: &App, p: &Palette) -> El {
    let preferences = &app.preferences;
    let open = preferences.menu == Some(Menu::ImageEditor);
    let value = editor_value(app);
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
    let control = col()
        .w(CHOICES)
        .gap(space::XS)
        .child(
            select(&value, open, p)
                .key("settings/image-editor")
                .label("Image editor")
                .on_press(|app: &mut App| actions::toggle_menu(app, Menu::ImageEditor)),
        )
        .children(field);
    setting(
        "settings/image-editor/row",
        "Image editor",
        "Where Edit in your image editor opens a texture.",
        control,
        p,
    )
}

/// What the image editor select says: the app's name, the system's
/// default, or Other.
fn editor_value(app: &App) -> String {
    if app.preferences.other {
        return "Other…".into();
    }
    match app.settings.external_editor.as_deref() {
        None => "System default".into(),
        Some(command) => app
            .preferences
            .apps
            .iter()
            .find(|known| known.command == command)
            .map_or_else(|| command.to_owned(), |known| known.name.clone()),
    }
}

/// The open select's menu, at the window's root over Settings.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    match app.preferences.menu? {
        Menu::ImageEditor => {
            let preferences = &app.preferences;
            let kept = app.settings.external_editor.as_deref();
            let mut items = vec![
                select_item("System default", kept.is_none() && !preferences.other, p)
                    .key("settings/image-editor/default")
                    .on_press(|app: &mut App| actions::choose_editor(app, None)),
            ];
            items.extend(preferences.apps.iter().enumerate().map(|(index, known)| {
                let command = known.command.clone();
                select_item(
                    &known.name,
                    !preferences.other && kept == Some(command.as_str()),
                    p,
                )
                .key(format!("settings/image-editor/app/{index}"))
                .on_press(move |app: &mut App| actions::choose_editor(app, Some(command.clone())))
            }));
            items.push(select_rule(p));
            items.push(
                select_item("Other…", preferences.other, p)
                    .key("settings/image-editor/other")
                    .on_press(actions::choose_other),
            );
            Some(select_menu(
                "settings/image-editor",
                items,
                actions::close_menu,
                p,
            ))
        }
    }
}

/// The app's own updates: ask textures.gg at launch, or not.
fn updates(app: &App, p: &Palette) -> El {
    let on = app.settings.check_for_updates.unwrap_or(true);
    setting(
        "settings/updates",
        "Check for updates",
        format!(
            "Once at launch, the app asks textures.gg whether a newer version is out. You have {}.",
            env!("CARGO_PKG_VERSION")
        ),
        switch("settings/updates/switch", on, p)
            .label("Check for updates")
            .on_press(move |app: &mut App| actions::set_update_check(app, !on)),
        p,
    )
}

/// Sending what went wrong to textures.gg.
fn report(p: &Palette) -> El {
    setting(
        "settings/report",
        "Report a problem",
        "Sends the app's log to textures.gg, and nothing else.",
        button("Report a problem", ButtonKind::Secondary, p)
            .key("settings/report/open")
            .on_press(actions::report_problem),
        p,
    )
}
