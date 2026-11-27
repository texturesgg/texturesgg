//! Settings: the player's Melee disc and Slippi ISO, tgg-melee's version and
//! updates, the image editor textures open in, the app's own update check,
//! and reporting a problem, in a dialog over the window. Rows sit without
//! rules between them. Each change is kept as it's made, in `editor.json`;
//! the image editor's command when the dialog closes. `actions` changes it;
//! `view` draws the dialog.

use super::{Menu, actions};
use crate::mods::{Disc, Registry};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, button_small, dialog, select,
    select_item, select_menu, select_rule, setting_row_with, spacer, switch, text_action,
    text_field,
};
use crate::update::Check as AppUpdateCheck;
use rune::{El, col, row, text as label};

/// The image editor's choices and field, at their widest.
const CHOICES: f32 = 300.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    let parts = if app.mods.versions {
        Dialog {
            title: "tgg-melee versions".into(),
            line: "Keep an older version if your mods haven't updated yet. Your mods stay installed either way.".into(),
            body: Some(crate::mods::versions::body(app, p)),
            buttons: vec![
                button("Back", ButtonKind::Secondary, p)
                    .key("mods/versions/back")
                    .on_press(crate::mods::actions::hide_versions),
                spacer(),
                button("Done", ButtonKind::Secondary, p)
                    .key("mods/versions/done")
                    .on_press(actions::close),
            ],
        }
    } else {
        let body = col().children([
            disc(app, p),
            slippi(app, p),
            tgg_melee(app, p),
            image_editor(app, p),
            updates(app, p),
            report(p),
        ]);
        Dialog {
            title: "Settings".into(),
            line: "Changes save automatically.".into(),
            body: Some(body),
            buttons: vec![
                spacer(),
                button("Done", ButtonKind::Secondary, p)
                    .key("settings/done")
                    .on_press(actions::close),
            ],
        }
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
        Disc::Found(path) => {
            let name = path.file_name().and_then(|name| name.to_str());
            if let Some(warning) = crate::mods::disc_warning(&app.mods.disc_modified) {
                match name {
                    Some(name) => format!("{name}. {warning}"),
                    None => warning,
                }
            } else {
                match name {
                    Some(name) => format!("{name}. Clean Melee disc used by tgg-melee."),
                    None => "Clean Melee NTSC 1.02 disc used by tgg-melee.".to_owned(),
                }
            }
        }
        Disc::Missing => "No Melee NTSC 1.02 disc found.".to_owned(),
        Disc::Unknown | Disc::Looking => "Looking for your disc…".to_owned(),
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
    let line = if let Some(game) = &app.game {
        match game
            .game()
            .path()
            .file_name()
            .and_then(|name| name.to_str())
        {
            Some(name) => format!("{name}. Skins with \"Also in Slippi\" are written here."),
            None => "Skins with \"Also in Slippi\" are written to this disc.".to_owned(),
        }
    } else {
        "Skins with \"Also in Slippi\" are written to this disc.".to_owned()
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
    let asking = app.mods.registry == Registry::Asking;
    let working = app.mods.work.is_some();
    let update = app.mods.port_update();
    let (line_text, line_color) = if working {
        ("Updating tgg-melee…".to_owned(), p.muted)
    } else if asking {
        ("Checking textures.gg for updates…".to_owned(), p.muted)
    } else if let Some(ref release) = update {
        let text = match app.mods.port() {
            Some(port) => format!(
                "You have tgg-melee {}. {} is available.",
                port.version, release.version
            ),
            None => format!("tgg-melee {} is available.", release.version),
        };
        (text, p.accent)
    } else if app.preferences.port_checked {
        match &app.mods.registry {
            Registry::Offline => ("Couldn't check for updates (offline).".to_owned(), p.danger),
            Registry::Failed(_) => ("Couldn't check for updates.".to_owned(), p.danger),
            Registry::Answered => match app.mods.port() {
                Some(port) => (
                    format!("You have tgg-melee {} (up to date).", port.version),
                    p.success,
                ),
                None => ("tgg-melee isn't installed yet.".to_owned(), p.muted),
            },
            Registry::NotAsked | Registry::Asking => {
                ("Checking textures.gg for updates…".to_owned(), p.muted)
            }
        }
    } else {
        let text = match app.mods.port() {
            Some(port) => format!("You have tgg-melee {}.", port.version),
            None => "tgg-melee isn't installed yet.".to_owned(),
        };
        (text, p.muted)
    };

    let action = if working {
        None
    } else if update.is_some() {
        Some(
            button_small("Update", ButtonKind::Primary, p)
                .key("settings/update-port")
                .on_press(crate::mods::actions::install_port),
        )
    } else if asking {
        Some(
            text_action("Checking…", false, p)
                .key("settings/fetch")
                .disabled(true),
        )
    } else {
        Some(
            text_action("Check for updates", false, p)
                .key("settings/fetch")
                .on_press(actions::check_port_updates),
        )
    };

    let mut control = row()
        .gap(space::XS)
        .items_center()
        .child(
            text_action("Versions", false, p)
                .key("settings/versions")
                .on_press(|app: &mut App| crate::mods::actions::show_versions(app, true)),
        )
        .children(action);
    if update.is_none() {
        control = control.mr(-TEXT_ACTION_INSET);
    }

    setting_row_with(
        "tgg-melee",
        label(line_text)
            .key("settings/tgg-melee/status")
            .font(text::SMALL)
            .color(line_color)
            .wrap(),
        control,
        p,
    )
    .key("settings/tgg-melee")
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
                text_field(&preferences.editor, "Command or app path, like aseprite", p)
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
        "App used when editing a texture externally.",
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
    if !app.preferences.open || app.mods.versions {
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

/// The app's own updates: ask textures.gg at launch, check now, or install an available update.
fn updates(app: &App, p: &Palette) -> El {
    let on = app.settings.check_for_updates.unwrap_or(true);
    let current = env!("CARGO_PKG_VERSION");
    let checking = app.update.check == AppUpdateCheck::Checking;
    let (line_text, line_color) = if checking {
        ("Checking textures.gg for app updates…".to_owned(), p.muted)
    } else if let Some(release) = &app.update.available {
        (
            format!(
                "You're on {current}. textures.gg {} is available.",
                release.version
            ),
            p.accent,
        )
    } else {
        match &app.update.check {
            AppUpdateCheck::UpToDate => (
                format!("You're on {current} (up to date). Checks automatically at launch."),
                p.success,
            ),
            AppUpdateCheck::Failed(_) => (
                format!("Couldn't check for updates. You're on {current}."),
                p.danger,
            ),
            AppUpdateCheck::Idle | AppUpdateCheck::Checking => (
                format!("Check textures.gg for app updates at launch. You're on {current}."),
                p.muted,
            ),
        }
    };

    let action = if app.update.available.is_some() {
        button_small("Update", ButtonKind::Primary, p)
            .key("settings/update-app")
            .on_press(crate::update::actions::open)
    } else if checking {
        text_action("Checking…", false, p)
            .key("settings/check-app-update")
            .disabled(true)
    } else {
        text_action("Check now", false, p)
            .key("settings/check-app-update")
            .on_press(crate::update::actions::check_now)
    };

    let control = row().gap(space::SM).items_center().child(action).child(
        switch("settings/updates/switch", on, p)
            .label("Check for updates")
            .on_press(move |app: &mut App| actions::set_update_check(app, !on)),
    );

    setting_row_with(
        "App updates",
        label(line_text)
            .key("settings/updates/status")
            .font(text::SMALL)
            .color(line_color)
            .wrap(),
        control,
        p,
    )
    .key("settings/updates")
}

/// Sending what went wrong to textures.gg.
fn report(p: &Palette) -> El {
    setting(
        "settings/report",
        "Report a problem",
        "Send the app log to textures.gg.",
        button("Report a problem", ButtonKind::Secondary, p)
            .key("settings/report/open")
            .on_press(actions::report_problem),
        p,
    )
}
